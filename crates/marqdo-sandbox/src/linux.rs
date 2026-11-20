//! Linux strict-isolation supervisor.
//!
//! The mechanism is Bubblewrap (`bwrap`) plus a small trusted supervisor in
//! this process:
//!
//! * user, mount, PID, network, UTS, IPC and cgroup namespaces
//!   (`--unshare-all --as-pid-1`);
//! * a read-only view of the runtime libraries, the project root and the
//!   program itself; no host `/etc`, `/root`, `$HOME` or other projects;
//! * a tmpfs `/tmp`, a private `/proc` and a minimal `/dev`;
//! * `no_new_privs`, all capabilities dropped, empty environment;
//! * hard rlimits for CPU, address space, file size, file descriptors,
//!   processes, stack and core dumps;
//! * a wall-clock supervisor with process-group cleanup and bounded captured
//!   output.
//!
//! Bubblewrap is a construction mechanism, not a certificate: [`probe`] runs
//! a real sandbox smoke test at runtime and strict is reported unavailable if
//! the host cannot create the namespaces. The sandboxed process additionally
//! verifies its own markers (`PID 1`, `NoNewPrivs`, unshared uid map, no
//! non-loopback interface) before executing user code.

use std::ffi::OsString;
use std::fs;
use std::io::{self, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::time::{Duration, Instant};

use crate::{SandboxError, StrictHostCommand};

/// Environment variable that marks a re-executed strict child.
pub const ACTIVE_ENV: &str = "MARQDO_STRICT_ACTIVE";

/// Path of the program inside the sandbox.
const SANDBOX_PROGRAM: &str = "/run/marqdo-program";

/// Candidate absolute Bubblewrap paths. `PATH` lookup is deliberately avoided
/// so a project-local binary cannot impersonate the supervisor tool.
const BWRAP_PATHS: [&str; 3] = ["/usr/bin/bwrap", "/bin/bwrap", "/usr/local/bin/bwrap"];

const PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const MAX_STACK_BYTES: u64 = 8 * 1024 * 1024;
const MAX_ADDRESS_SPACE: u64 = 512 * 1024 * 1024;
const MAX_PROCESSES: u64 = 64;
const MAX_OPEN_FILES: u64 = 64;

fn bwrap_path() -> Option<PathBuf> {
    BWRAP_PATHS
        .iter()
        .map(PathBuf::from)
        .find(|path| path.is_file())
}

/// Probe the real availability of the strict mechanism.
pub fn probe() -> Result<String, String> {
    let bwrap = bwrap_path().ok_or_else(|| {
        "bubblewrap (`bwrap`) is not installed; install it or use project mode".to_string()
    })?;
    let version = bwrap_version(&bwrap)?;
    let mut command = Command::new(&bwrap);
    command
        .args(common_args())
        .args(["--", "/bin/true"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let mut child = command
        .spawn()
        .map_err(|error| format!("cannot start the bubblewrap probe: {error}"))?;
    let status = wait_with_deadline(&mut child, PROBE_TIMEOUT)?;
    let mut stderr = String::new();
    if let Some(mut pipe) = child.stderr.take() {
        let _ = pipe.read_to_string(&mut stderr);
    }
    if !status.success() {
        return Err(format!(
            "bubblewrap cannot create the required namespaces: {}",
            stderr.trim()
        ));
    }
    Ok(format!(
        "bubblewrap {version} (user/mount/PID/net/UTS/IPC namespaces + no-new-privs + rlimits)"
    ))
}

fn bwrap_version(bwrap: &Path) -> Result<String, String> {
    let output = Command::new(bwrap)
        .arg("--version")
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("cannot run `{}`: {error}", bwrap.display()))?;
    if !output.status.success() {
        return Err("`bwrap --version` failed".to_string());
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let version = text
        .lines()
        .next()
        .unwrap_or("")
        .split_whitespace()
        .nth(1)
        .unwrap_or("unknown")
        .to_string();
    Ok(version)
}

fn common_args() -> Vec<OsString> {
    let mut args: Vec<OsString> = [
        "--unshare-all",
        "--as-pid-1",
        "--die-with-parent",
        "--new-session",
        "--cap-drop",
        "ALL",
        "--clearenv",
        "--setenv",
        "PATH",
        "/usr/bin:/bin",
        "--setenv",
        "HOME",
        "/tmp",
        "--setenv",
        "LANG",
        "C.UTF-8",
        "--proc",
        "/proc",
        "--dev",
        "/dev",
        "--tmpfs",
        "/tmp",
        "--dir",
        "/run",
    ]
    .into_iter()
    .map(OsString::from)
    .collect();
    for directory in ["/usr", "/bin", "/sbin", "/lib", "/lib64"] {
        let path = Path::new(directory);
        if path.exists() {
            args.extend([
                OsString::from("--ro-bind"),
                OsString::from(directory),
                OsString::from(directory),
            ]);
        }
    }
    if Path::new("/etc/ld.so.cache").is_file() {
        args.extend([
            OsString::from("--ro-bind"),
            OsString::from("/etc/ld.so.cache"),
            OsString::from("/etc/ld.so.cache"),
        ]);
    }
    args
}

/// Verify that this process really is the strict child created by [`launch`].
pub fn verify_active() -> Result<(), String> {
    if std::env::var(ACTIVE_ENV).ok().as_deref() != Some("1") {
        return Err("strict active marker is absent".to_string());
    }
    // `--as-pid-1` makes the supervised program PID 1 in its own PID namespace.
    let pid = unsafe { libc::getpid() };
    if pid != 1 {
        return Err(format!(
            "strict child is not PID 1 in a private PID namespace (pid {pid})"
        ));
    }
    let status = fs::read_to_string("/proc/self/status")
        .map_err(|error| format!("cannot read /proc/self/status: {error}"))?;
    if !status.lines().any(|line| line == "NoNewPrivs:\t1") {
        return Err("strict child does not have no_new_privs set".to_string());
    }
    let effective = status
        .lines()
        .find_map(|line| line.strip_prefix("CapEff:\t"))
        .unwrap_or("");
    if effective.bytes().any(|byte| byte != b'0') {
        return Err("strict child still has effective capabilities".to_string());
    }
    let uid_map = fs::read_to_string("/proc/self/uid_map")
        .map_err(|error| format!("cannot read uid_map: {error}"))?;
    let unshared = uid_map.lines().any(|line| {
        let fields: Vec<&str> = line.split_whitespace().collect();
        !(fields.len() == 3 && fields[0] == "0" && fields[1] == "0" && fields[2] == "4294967295")
    });
    if !unshared {
        return Err("strict child did not create a private user namespace".to_string());
    }
    let interfaces = fs::read_to_string("/proc/net/dev")
        .map_err(|error| format!("cannot read /proc/net/dev: {error}"))?;
    let non_loopback = interfaces.lines().skip(2).any(|line| {
        let name = line.split(':').next().unwrap_or("").trim();
        !name.is_empty() && name != "lo"
    });
    if non_loopback {
        return Err("strict child still has a non-loopback network interface".to_string());
    }
    Ok(())
}

/// Whether the current process claims to be a strict child. The marker alone
/// is not trusted; callers must use [`verify_active`].
pub fn marker_present() -> bool {
    std::env::var(ACTIVE_ENV).ok().as_deref() == Some("1")
}

/// Result of one supervised strict execution.
#[derive(Debug, Clone)]
pub struct StrictOutcome {
    /// Process exit code when the program exited normally.
    pub exit_code: Option<i32>,
    /// Terminating signal, when the program was killed by the supervisor,
    /// a resource limit or the kernel.
    pub signal: Option<i32>,
    /// Captured stdout (bounded by the output limit).
    pub stdout: Vec<u8>,
    /// Captured stderr (bounded by the output limit).
    pub stderr: Vec<u8>,
}

/// Launch `command` inside the strict sandbox and return its exit code,
/// forwarding captured output to this process's stdout/stderr.
pub fn launch(command: &StrictHostCommand<'_>) -> Result<i32, SandboxError> {
    let outcome = run(command)?;
    let mut stdout = io::stdout().lock();
    let _ = stdout.write_all(&outcome.stdout);
    let _ = stdout.flush();
    let mut stderr = io::stderr().lock();
    let _ = stderr.write_all(&outcome.stderr);
    let _ = stderr.flush();
    finish(outcome)
}

/// Launch `command` and return its captured outcome without forwarding it.
/// Used by the adversarial security lane.
pub fn launch_capture(command: &StrictHostCommand<'_>) -> Result<StrictOutcome, SandboxError> {
    run(command)
}

fn finish(outcome: StrictOutcome) -> Result<i32, SandboxError> {
    if let Some(code) = outcome.exit_code {
        return Ok(code);
    }
    let signal = outcome.signal.unwrap_or_default();
    let message = match signal {
        libc::SIGXCPU => "strict child exceeded its CPU budget".to_string(),
        libc::SIGXFSZ => "strict child exceeded its output budget".to_string(),
        libc::SIGKILL => "strict child was killed by the resource supervisor".to_string(),
        other => format!("strict child terminated by signal {other}"),
    };
    Err(SandboxError::LaunchFailed { message })
}

fn run(command: &StrictHostCommand<'_>) -> Result<StrictOutcome, SandboxError> {
    if command.project_root == Path::new("/") {
        return Err(SandboxError::LaunchFailed {
            message: "strict mode refuses to expose the host root as the project".to_string(),
        });
    }
    let project_root =
        fs::canonicalize(command.project_root).map_err(|error| SandboxError::LaunchFailed {
            message: format!(
                "cannot resolve the strict project root `{}`: {error}",
                command.project_root.display()
            ),
        })?;
    // The document (when supplied) must be inside the project root. It is
    // rewritten to the sandbox path, so the host's absolute layout never leaks
    // into the child and no other directory needs to be mounted.
    let document = match command.document {
        Some(document) => {
            let absolute =
                fs::canonicalize(document).map_err(|error| SandboxError::LaunchFailed {
                    message: format!("cannot resolve strict document: {error}"),
                })?;
            let relative =
                absolute
                    .strip_prefix(&project_root)
                    .map_err(|_| SandboxError::LaunchFailed {
                        message: format!(
                            "strict document `{}` is outside the project root `{}`",
                            absolute.display(),
                            project_root.display()
                        ),
                    })?;
            Some(Path::new("/project").join(relative))
        }
        None => None,
    };
    probe().map_err(|reason| SandboxError::LaunchFailed { message: reason })?;
    let bwrap = bwrap_path().ok_or_else(|| SandboxError::LaunchFailed {
        message: "bubblewrap disappeared after the probe".to_string(),
    })?;

    let mut args = common_args();
    args.extend([
        OsString::from("--ro-bind"),
        project_root.as_os_str().to_os_string(),
        OsString::from("/project"),
        OsString::from("--ro-bind"),
        command.program.as_os_str().to_os_string(),
        OsString::from(SANDBOX_PROGRAM),
        OsString::from("--setenv"),
        OsString::from(ACTIVE_ENV),
        OsString::from("1"),
        OsString::from("--chdir"),
        OsString::from("/project"),
        OsString::from("--"),
        OsString::from(SANDBOX_PROGRAM),
    ]);
    // Rewrite the document argument to its `/project/...` sandbox path. The
    // CLI passed the original token, so replace exactly that token; all other
    // arguments (flags, function name, typed --arg values) are unchanged.
    let mut replaced = document.is_none();
    for argument in command.args {
        if let Some(document) = &document {
            if Path::new(argument) == command.original_document.unwrap_or(Path::new("")) {
                args.push(document.as_os_str().to_os_string());
                replaced = true;
                continue;
            }
        }
        args.push(argument.clone());
    }
    if !replaced {
        return Err(SandboxError::LaunchFailed {
            message: "strict launcher could not rewrite the document argument".to_string(),
        });
    }

    let stdout = CaptureFile::create("stdout")?;
    let stderr = CaptureFile::create("stderr")?;
    let limit = command.output_limit.max(1024) as u64;
    let timeout = command.timeout;
    let cpu_limit = timeout.as_secs().saturating_add(2).max(1);
    let mut supervisor = Command::new(&bwrap);
    supervisor
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::from(stdout.reopen()?))
        .stderr(Stdio::from(stderr.reopen()?));
    // SAFETY: the closure only calls async-signal-safe libc functions
    // (`setpgid`, `setrlimit`) between fork and exec.
    unsafe {
        supervisor.pre_exec(move || {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            set_limit(libc::RLIMIT_CPU, cpu_limit, cpu_limit)?;
            set_limit(libc::RLIMIT_AS, MAX_ADDRESS_SPACE, MAX_ADDRESS_SPACE)?;
            set_limit(libc::RLIMIT_NOFILE, MAX_OPEN_FILES, MAX_OPEN_FILES)?;
            set_limit(libc::RLIMIT_NPROC, MAX_PROCESSES, MAX_PROCESSES)?;
            set_limit(libc::RLIMIT_FSIZE, limit, limit)?;
            set_limit(libc::RLIMIT_STACK, MAX_STACK_BYTES, MAX_STACK_BYTES)?;
            set_limit(libc::RLIMIT_CORE, 0, 0)?;
            Ok(())
        });
    }
    let mut child = supervisor
        .spawn()
        .map_err(|error| SandboxError::LaunchFailed {
            message: format!("cannot start the strict supervisor: {error}"),
        })?;

    let status = match wait_with_deadline(&mut child, timeout) {
        Ok(status) => status,
        Err(message) => {
            let _ = unsafe { libc::kill(-(child.id() as i32), libc::SIGKILL) };
            let _ = child.kill();
            let _ = child.wait();
            let _ = stdout.read_all();
            let _ = stderr.read_all();
            return Err(SandboxError::LaunchFailed { message });
        }
    };

    Ok(StrictOutcome {
        exit_code: status.code(),
        signal: status.signal(),
        stdout: stdout.read_all()?,
        stderr: stderr.read_all()?,
    })
}

fn set_limit(resource: libc::__rlimit_resource_t, soft: u64, hard: u64) -> io::Result<()> {
    let limit = libc::rlimit {
        rlim_cur: soft as libc::rlim_t,
        rlim_max: hard as libc::rlim_t,
    };
    if unsafe { libc::setrlimit(resource, &limit) } != 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

fn wait_with_deadline(child: &mut Child, timeout: Duration) -> Result<ExitStatus, String> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("cannot wait for the strict child: {error}"))?
        {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            return Err(format!(
                "strict child exceeded its wall-clock budget of {} second(s)",
                timeout.as_secs()
            ));
        }
        std::thread::sleep(Duration::from_millis(5));
    }
}

struct CaptureFile {
    path: PathBuf,
    file: Option<fs::File>,
}

impl CaptureFile {
    fn create(tag: &str) -> Result<Self, SandboxError> {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|duration| duration.as_nanos())
            .unwrap_or(0);
        let path = std::env::temp_dir().join(format!(
            "marqdo-strict-{tag}-{}-{nanos}",
            std::process::id()
        ));
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .open(&path)
            .map_err(|error| SandboxError::LaunchFailed {
                message: format!("cannot create strict capture file: {error}"),
            })?;
        Ok(Self {
            path,
            file: Some(file),
        })
    }

    fn reopen(&self) -> Result<fs::File, SandboxError> {
        self.file
            .as_ref()
            .ok_or_else(|| SandboxError::LaunchFailed {
                message: "strict capture file is closed".to_string(),
            })?
            .try_clone()
            .map_err(|error| SandboxError::LaunchFailed {
                message: format!("cannot clone strict capture file: {error}"),
            })
    }

    fn read_all(&self) -> Result<Vec<u8>, SandboxError> {
        let mut file = fs::File::open(&self.path).map_err(|error| SandboxError::LaunchFailed {
            message: format!("cannot read strict capture file: {error}"),
        })?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|error| SandboxError::LaunchFailed {
                message: format!("cannot read strict capture file: {error}"),
            })?;
        Ok(bytes)
    }
}

impl Drop for CaptureFile {
    fn drop(&mut self) {
        drop(self.file.take());
        let _ = fs::remove_file(&self.path);
    }
}
