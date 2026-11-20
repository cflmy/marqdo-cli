//! Adversarial Linux strict-isolation lane.
//!
//! These tests launch a native helper through the **same** `marqdo-sandbox`
//! supervisor used by `marqdo run --mode strict`, then exercise direct
//! syscalls for filesystem, network, process, privilege and resource escapes.
//!
//! They require a host where [`marqdo_sandbox::probe`] succeeds (Bubblewrap
//! plus permitted user/mount/PID/net namespaces). On an incapable host they
//! are skipped **unless** `MARQDO_REQUIRE_STRICT_SECURITY=1` is set, which the
//! G-C security lane must set so a missing mechanism can never count as a
//! pass.
//!
//! The helper path can be supplied with `MARQDO_SECURITY_HELPER` (used when
//! running this test binary inside the disposable security VM). Otherwise the
//! test compiles `tests/security/escape_helper.c` with `cc`.
#![cfg(target_os = "linux")]

use std::ffi::OsString;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use marqdo_sandbox::{launch_strict_capture, probe, StrictHostCommand, StrictOutcome};

fn required() -> bool {
    std::env::var("MARQDO_REQUIRE_STRICT_SECURITY")
        .ok()
        .as_deref()
        == Some("1")
}

fn helper() -> Option<PathBuf> {
    if let Ok(path) = std::env::var("MARQDO_SECURITY_HELPER") {
        let path = PathBuf::from(path);
        assert!(path.is_file(), "MARQDO_SECURITY_HELPER does not exist");
        return Some(path);
    }
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/security/escape_helper.c");
    let output = std::env::temp_dir().join(format!(
        "marqdo-escape-helper-{}{}",
        std::process::id(),
        std::env::consts::EXE_SUFFIX
    ));
    let status = Command::new("cc")
        .args(["-O2", "-o"])
        .arg(&output)
        .arg(&source)
        .status()
        .ok()?;
    if !status.success() {
        return None;
    }
    Some(output)
}

fn setup() -> Option<(PathBuf, PathBuf, PathBuf)> {
    if let Err(reason) = probe() {
        if required() {
            panic!("G-C security lane requires strict isolation but probe failed: {reason}");
        }
        eprintln!("skipping strict security lane: {reason}");
        return None;
    }
    let helper = match helper() {
        Some(helper) => helper,
        None => {
            if required() {
                panic!("G-C security lane could not build or find escape_helper");
            }
            eprintln!("skipping strict security lane: no escape helper/cc");
            return None;
        }
    };
    let project = std::env::temp_dir().join(format!("marqdo-security-{}", std::process::id()));
    let _ = fs::remove_dir_all(&project);
    fs::create_dir_all(&project).expect("create security project");
    fs::write(project.join("allowed.txt"), "allowed\n").expect("write allowed file");
    // A host-only path that is not mounted into the sandbox for any user. The
    // sandbox has no `$HOME`, so attempting to write here must fail.
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("/home"));
    let outside = home.join(format!("marqdo-escape-{}", std::process::id()));
    let _ = fs::remove_file(&outside);
    Some((helper, project, outside))
}

fn run(helper: &Path, project: &Path, outside: &Path, mode: &str) -> StrictOutcome {
    let args = vec![OsString::from(mode), outside.as_os_str().to_os_string()];
    let command = StrictHostCommand {
        program: helper,
        args: &args,
        project_root: project,
        document: None,
        original_document: None,
        output_limit: 64 * 1024,
        timeout: Duration::from_secs(3),
    };
    launch_strict_capture(&command).unwrap_or_else(|error| panic!("{mode}: {error}"))
}

fn text(outcome: &StrictOutcome) -> String {
    String::from_utf8_lossy(&outcome.stdout).to_string()
}

#[test]
fn strict_child_identity_is_unprivileged_and_namespaced() {
    let Some((helper, project, outside)) = setup() else {
        return;
    };
    let outcome = run(&helper, &project, &outside, "alive");
    assert_eq!(outcome.exit_code, Some(0), "{}", text(&outcome));
    let output = text(&outcome);
    assert!(
        output.contains("uid=1000") || output.contains("uid=1001"),
        "{output}"
    );
    assert!(
        !output.contains("uid=0 "),
        "strict child ran as root: {output}"
    );
    assert!(
        output.contains("pid=1"),
        "strict child is not PID 1: {output}"
    );
    assert!(output.contains("NoNewPrivs:\t1"), "{output}");
    let cap_eff = output
        .lines()
        .find_map(|line| line.strip_prefix("CapEff:"))
        .unwrap_or("")
        .trim();
    assert!(
        cap_eff.bytes().all(|byte| byte == b'0'),
        "strict child kept capabilities: {cap_eff}"
    );
    let _ = fs::remove_dir_all(&project);
}

#[test]
fn strict_child_cannot_read_host_files_but_reads_the_project() {
    let Some((helper, project, outside)) = setup() else {
        return;
    };
    let outcome = run(&helper, &project, &outside, "read");
    let output = text(&outcome);
    assert!(output.contains("read_project_allowed=OK"), "{output}");
    for key in ["read_etc_shadow", "read_root_key", "read_home_key"] {
        assert!(output.contains(&format!("{key}=DENIED")), "{key}: {output}");
        assert!(!output.contains(&format!("{key}=LEAK")), "{key}: {output}");
    }
    let _ = fs::remove_dir_all(&project);
}

#[test]
fn strict_child_cannot_write_outside_or_into_the_project() {
    let Some((helper, project, outside)) = setup() else {
        return;
    };
    let outcome = run(&helper, &project, &outside, "write");
    let output = text(&outcome);
    assert!(output.contains("write_outside=DENIED"), "{output}");
    assert!(output.contains("write_project=DENIED"), "{output}");
    assert!(output.contains("write_tmp=WROTE"), "{output}");
    assert!(
        !outside.exists(),
        "strict child created a host file at {}",
        outside.display()
    );
    let _ = fs::remove_dir_all(&project);
}

#[test]
fn strict_child_cannot_escape_via_proc_or_privileges() {
    let Some((helper, project, outside)) = setup() else {
        return;
    };
    let proc = run(&helper, &project, &outside, "proc");
    let output = text(&proc);
    assert!(output.contains("proc_pid1_root_etc=DENIED"), "{output}");
    assert!(
        output.contains("inherited_fds_above_stdio=NONE"),
        "{output}"
    );
    let privileges = run(&helper, &project, &outside, "priv");
    let output = text(&privileges);
    assert!(output.contains("setuid_root=DENIED"), "{output}");
    assert!(output.contains("mount_tmpfs=DENIED"), "{output}");
    assert!(!output.contains("setuid_root=OK"), "{output}");
    assert!(!output.contains("mount_tmpfs=OK"), "{output}");
    let _ = fs::remove_dir_all(&project);
}

#[test]
fn strict_child_cannot_reach_the_network() {
    let Some((helper, project, outside)) = setup() else {
        return;
    };
    let outcome = run(&helper, &project, &outside, "net");
    let output = text(&outcome);
    assert!(!output.contains("CONNECTED"), "{output}");
    assert!(output.contains("socket_ipv4=DENIED"), "{output}");
    assert!(output.contains("socket_raw=DENIED"), "{output}");
    let _ = fs::remove_dir_all(&project);
}

#[test]
fn strict_supervisor_enforces_fd_and_process_limits() {
    let Some((helper, project, outside)) = setup() else {
        return;
    };
    let fds = run(&helper, &project, &outside, "fds");
    let output = text(&fds);
    assert!(output.contains("fd_limit=BOUNDED"), "{output}");
    let count: usize = output
        .lines()
        .find_map(|line| line.strip_prefix("open_fds="))
        .and_then(|value| value.parse().ok())
        .unwrap_or(usize::MAX);
    assert!(count <= 64, "strict child opened {count} fds");

    let forks = run(&helper, &project, &outside, "fork");
    let output = text(&forks);
    assert!(output.contains("process_limit=BOUNDED"), "{output}");
    let forked: usize = output
        .lines()
        .find_map(|line| line.strip_prefix("forked="))
        .and_then(|value| value.parse().ok())
        .unwrap_or(usize::MAX);
    assert!(forked < 200, "strict child forked {forked} processes");
    let _ = fs::remove_dir_all(&project);
}

#[test]
fn strict_supervisor_enforces_memory_cpu_and_output_limits() {
    let Some((helper, project, outside)) = setup() else {
        return;
    };
    let memory = run(&helper, &project, &outside, "mem");
    let output = text(&memory);
    assert!(
        !output.contains("malloc_768m=OK"),
        "768 MiB allocation escaped the memory limit: {output}"
    );

    let args = vec![OsString::from("cpu")];
    let command = StrictHostCommand {
        program: &helper,
        args: &args,
        project_root: &project,
        document: None,
        original_document: None,
        output_limit: 64 * 1024,
        timeout: Duration::from_secs(2),
    };
    let cpu = launch_strict_capture(&command).expect_err("CPU loop must be stopped");
    let message = cpu.to_string();
    assert!(
        message.contains("wall-clock") || message.contains("CPU"),
        "unexpected CPU failure: {message}"
    );

    let output_limit = launch_strict_capture(&StrictHostCommand {
        program: &helper,
        args: &[OsString::from("output")],
        project_root: &project,
        document: None,
        original_document: None,
        output_limit: 64 * 1024,
        timeout: Duration::from_secs(3),
    })
    .expect("output helper should be captured, not spawn-failed");
    assert!(
        output_limit.signal == Some(libc::SIGXFSZ) || output_limit.exit_code != Some(0),
        "output limit was not enforced: signal={:?} exit={:?}",
        output_limit.signal,
        output_limit.exit_code
    );
    assert!(
        output_limit.stdout.len() <= 64 * 1024,
        "captured {} bytes beyond the limit",
        output_limit.stdout.len()
    );
    // The helper cannot always report after the file hits RLIMIT_FSIZE: the
    // report itself would exceed the limit. A non-zero exit, a terminating
    // signal, and a bounded capture are the observable contract.
    let written = output_limit
        .stdout
        .windows(b"output_4m=WRITTEN".len())
        .any(|window| window == b"output_4m=WRITTEN");
    assert!(!written, "the helper reported writing 4 MiB");
    let _ = fs::remove_dir_all(&project);
}
