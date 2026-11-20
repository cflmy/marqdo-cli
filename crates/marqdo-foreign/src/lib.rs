//! Explicit foreign code kernels (Python/Node).
//!
//! A fenced `python` or `javascript` code block is an inert [`Code`] value in
//! Marqdo. Nothing in this crate is ever spawned while reading, parsing or
//! rendering a document: a process is created **only** when the host calls
//! [`run`] or [`call`] after the capability check succeeded.
//!
//! Every execution is one-shot and isolated:
//!
//! * the kernel program is written to a private, uniquely named file under
//!   [`std::env::temp_dir`] (`.py` or `.js`), the child is started by direct
//!   `argv` exec (no shell, no string interpolation) and its working directory
//!   is a fresh private temporary directory so relative writes never touch the
//!   project; modern Node performs syntax detection on `.js`, so both ESM
//!   `import` and CommonJS `require` kernels are supported;
//! * stdin carries the versioned request envelope
//!   `{"wire_version":1,"kind":"request","input":<value or null>}`;
//! * stdout and stderr are always piped and captured, never inherited;
//! * the environment is cleared except for `PATH`, so Marqdo secrets do not
//!   leak into the kernel by default;
//! * a wall-clock deadline is enforced by polling `try_wait` and killing the
//!   child on expiry, and a shared byte budget aborts output flooding;
//! * the temporary program file and working directory are removed afterwards,
//!   including on error paths.
//!
//! [`run`] returns captured output as data. [`call`] additionally requires
//! stdout to be exactly one versioned wire value (see
//! [`marqdo_value::from_wire`]); any stdout pollution or trailing noise is a
//! [`ForeignError::Protocol`] error rather than something silently ignored.
//!
//! This is process isolation, not a security sandbox. The `strict` sandbox
//! mode additionally needs OS-level isolation (namespaces/seccomp/cgroups or
//! platform equivalents) as described in the development blueprint; this crate
//! does not claim to provide it.
//!
//! [`Code`]: marqdo_value::CodeValue

use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use marqdo_value::Value;
use serde_json::Value as Json;

/// Value of the `kind` field in the stdin request envelope.
pub const REQUEST_KIND: &str = "request";

/// Timeout applied to `--version` probing in [`detect`].
const DETECT_TIMEOUT: Duration = Duration::from_secs(5);

/// Maximum bytes captured while probing a kernel version.
const DETECT_OUTPUT_LIMIT: usize = 64 * 1024;

/// Polling interval for `try_wait` while supervising a child.
const POLL_INTERVAL: Duration = Duration::from_millis(2);

/// How long to wait for the pipe readers after the child was reaped. A
/// misbehaving kernel that leaves a grandchild holding the pipes must not hang
/// the Marqdo host process.
const READER_GRACE: Duration = Duration::from_secs(2);

static UNIQUE_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Canonical language names supported by this crate.
pub const SUPPORTED_LANGUAGES: [&str; 2] = ["python", "javascript"];

/// Normalize a language alias to its canonical kernel name.
///
/// Recognized aliases are `python`/`python3`/`py` and
/// `javascript`/`js`/`node`/`nodejs`. The comparison is ASCII
/// case-insensitive and surrounding whitespace is ignored. Unknown languages
/// return `None`.
pub fn normalize_language(language: &str) -> Option<&'static str> {
    match language.trim().to_ascii_lowercase().as_str() {
        "python" | "python3" | "py" => Some("python"),
        "javascript" | "js" | "node" | "nodejs" => Some("javascript"),
        _ => None,
    }
}

/// What the host asks a foreign kernel to do.
#[derive(Debug, Clone, PartialEq)]
pub struct RunRequest {
    /// Language name or alias (for example `"python3"` or `"js"`).
    pub language: String,
    /// Program source executed by the kernel.
    pub source: String,
    /// Optional input value delivered inside the versioned stdin envelope.
    pub input: Option<Json>,
    /// Wall-clock budget for the whole child execution.
    pub timeout: Duration,
    /// Maximum number of bytes captured across stdout and stderr combined.
    pub max_output_bytes: usize,
}

impl RunRequest {
    /// Create a request with conservative defaults: 30 s timeout and 1 MiB of
    /// captured output.
    pub fn new(language: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            language: language.into(),
            source: source.into(),
            input: None,
            timeout: Duration::from_secs(30),
            max_output_bytes: 1024 * 1024,
        }
    }
}

/// Terminal state of a foreign execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum RunStatus {
    /// The process exited with status 0 (for [`call`]: and its output decoded).
    Success,
    /// The process exited non-zero or was terminated by a signal.
    Failed,
    /// The wall-clock deadline expired and the process was killed.
    TimedOut,
}

impl RunStatus {
    /// Stable lowercase name for logging and diagnostics.
    pub const fn as_str(self) -> &'static str {
        match self {
            RunStatus::Success => "success",
            RunStatus::Failed => "failed",
            RunStatus::TimedOut => "timed_out",
        }
    }
}

/// Outcome of one foreign execution.
#[derive(Debug, Clone, PartialEq)]
pub struct RunResult {
    /// Success/failure/timeout classification.
    pub status: RunStatus,
    /// Captured stdout (lossy UTF-8 for [`run`], validated for [`call`]).
    pub stdout: String,
    /// Captured stderr; preserved even when the kernel fails.
    pub stderr: String,
    /// Exit code, or `None` when the child was killed by a signal or timed out.
    pub exit_code: Option<i32>,
    /// Measured wall-clock duration of the child execution.
    pub duration: Duration,
    /// Decoded wire value; only [`call`] sets this on success.
    pub value: Option<Value>,
}

/// A detected foreign kernel.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KernelInfo {
    /// Canonical language name (`"python"` or `"javascript"`).
    pub language: String,
    /// Resolved absolute path of the interpreter executable.
    pub program: PathBuf,
    /// First line of the interpreter's `--version` output.
    pub version: String,
}

/// Failure while detecting or executing a foreign kernel.
#[derive(Debug, Clone, thiserror::Error)]
pub enum ForeignError {
    /// No usable Python/Node interpreter was found on `PATH`.
    #[error("foreign kernel `{language}` is unavailable: {hint}")]
    KernelUnavailable {
        /// Requested (normalized when known) language name.
        language: String,
        /// Actionable installation guidance.
        hint: String,
    },
    /// The child process could not be created.
    #[error("failed to spawn foreign kernel: {message}")]
    Spawn {
        /// Operating-system error text.
        message: String,
    },
    /// A pipe, temporary file or wait operation failed.
    #[error("foreign kernel I/O error: {message}")]
    Io {
        /// Operating-system error text.
        message: String,
    },
    /// The wall-clock deadline expired; the child was killed.
    #[error("foreign kernel exceeded its wall-clock timeout of {seconds} seconds")]
    Timeout {
        /// Requested timeout, in seconds.
        seconds: f64,
    },
    /// The kernel violated the stdout wire protocol or sent invalid UTF-8.
    #[error("foreign kernel protocol violation: {message}")]
    Protocol {
        /// What went wrong.
        message: String,
    },
    /// Captured stdout/stderr exceeded `max_output_bytes`; the child was
    /// killed and the partial output dropped.
    #[error("foreign kernel output exceeded the {limit} byte limit")]
    OutputTooLarge {
        /// Configured byte budget.
        limit: usize,
    },
}

/// Detect a kernel for `language` by scanning `PATH`.
///
/// Candidates are `python3`, then `python` for Python, and `node` for
/// JavaScript. The returned [`KernelInfo::version`] is the first line of the
/// interpreter's `--version` output. Returns `None` for an unknown language or
/// when no working interpreter is installed; detection honors the same
/// cleared-environment rule as execution and is bounded by a short timeout.
pub fn detect(language: &str) -> Option<KernelInfo> {
    let canonical = normalize_language(language)?;
    let path_var = std::env::var_os("PATH")?;
    for name in program_candidates(canonical) {
        for directory in std::env::split_paths(&path_var) {
            let program = directory.join(name);
            if !is_executable_file(&program) {
                continue;
            }
            if let Some(version) = probe_version(&program) {
                return Some(KernelInfo {
                    language: canonical.to_string(),
                    program,
                    version,
                });
            }
        }
    }
    None
}

/// One-shot isolated execution of foreign code.
///
/// The source is run in a fresh child process. `request.input` is delivered as
/// the versioned JSON envelope on stdin, stdout/stderr are captured, and the
/// following are enforced: `request.timeout` (wall clock) and
/// `request.max_output_bytes` (stdout + stderr combined).
///
/// A non-zero exit is reported as [`RunStatus::Failed`] with `stderr`
/// preserved, not as an `Err`. Timeouts are reported as
/// [`RunStatus::TimedOut`]. Infrastructure failures (unavailable kernel, spawn
/// failure, exceeding the output budget) are [`ForeignError`]s.
pub fn run(request: &RunRequest) -> Result<RunResult, ForeignError> {
    let raw = execute(request)?;
    Ok(RunResult {
        status: raw.status,
        stdout: String::from_utf8_lossy(&raw.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&raw.stderr).into_owned(),
        exit_code: raw.exit_code,
        duration: raw.duration,
        value: None,
    })
}

/// Typed one-shot call, like [`run`] but with a strict stdout wire protocol.
///
/// On success, stdout must be exactly one versioned wire value object (no
/// log lines, no trailing noise); it is decoded with
/// [`marqdo_value::from_wire`] and stored in [`RunResult::value`]. Any
/// protocol violation is [`ForeignError::Protocol`]. A non-zero exit still
/// returns `Ok` with [`RunStatus::Failed`] and `stderr` preserved so the host
/// can report the kernel's own failure faithfully; a timeout returns
/// [`ForeignError::Timeout`] because no typed value can exist.
pub fn call(request: &RunRequest) -> Result<RunResult, ForeignError> {
    let raw = execute(request)?;
    match raw.status {
        RunStatus::TimedOut => Err(ForeignError::Timeout {
            seconds: request.timeout.as_secs_f64(),
        }),
        RunStatus::Failed => Ok(RunResult {
            status: raw.status,
            stdout: String::from_utf8_lossy(&raw.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&raw.stderr).into_owned(),
            exit_code: raw.exit_code,
            duration: raw.duration,
            value: None,
        }),
        RunStatus::Success => {
            let text =
                std::str::from_utf8(&raw.stdout).map_err(|error| ForeignError::Protocol {
                    message: format!("stdout is not valid UTF-8: {error}"),
                })?;
            let wire: Json =
                serde_json::from_str(text).map_err(|error| ForeignError::Protocol {
                    message: format!(
                        "stdout must be exactly one JSON wire value without extra output: {error}"
                    ),
                })?;
            let value = marqdo_value::from_wire(&wire).map_err(|error| ForeignError::Protocol {
                message: format!("stdout is not a valid marqdo wire value: {error}"),
            })?;
            Ok(RunResult {
                status: RunStatus::Success,
                stdout: text.to_string(),
                stderr: String::from_utf8_lossy(&raw.stderr).into_owned(),
                exit_code: raw.exit_code,
                duration: raw.duration,
                value: Some(value),
            })
        }
    }
}

/// Owned bytes-level result shared by [`run`] and [`call`].
struct RawRun {
    status: RunStatus,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    exit_code: Option<i32>,
    duration: Duration,
}

fn execute(request: &RunRequest) -> Result<RawRun, ForeignError> {
    let canonical =
        normalize_language(&request.language).ok_or_else(|| ForeignError::KernelUnavailable {
            language: request.language.clone(),
            hint: format!(
                "supported languages are `python` (aliases: python3, py) and \
                 `javascript` (aliases: js, node); `{}` is not a foreign kernel",
                request.language
            ),
        })?;
    let program = find_program(canonical).ok_or_else(|| ForeignError::KernelUnavailable {
        language: canonical.to_string(),
        hint: kernel_hint(canonical).to_string(),
    })?;

    let artifacts = TempArtifacts::create(canonical)?;
    artifacts.write_program(&request.source)?;
    let payload = request_envelope(request)?;

    let mut command = Command::new(&program);
    command
        .arg(&artifacts.program_path)
        .current_dir(&artifacts.work_dir)
        .env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }

    let captured = spawn_and_capture(
        &mut command,
        request.timeout,
        request.max_output_bytes,
        Some(payload),
    )?;

    let (status, exit_code) = match captured.outcome {
        WaitOutcome::Exited(exit) => {
            let status = if exit.success() {
                RunStatus::Success
            } else {
                RunStatus::Failed
            };
            (status, exit.code())
        }
        WaitOutcome::TimedOut => (RunStatus::TimedOut, None),
    };

    Ok(RawRun {
        status,
        stdout: captured.stdout,
        stderr: captured.stderr,
        exit_code,
        duration: captured.duration,
    })
}

fn program_candidates(language: &str) -> &'static [&'static str] {
    match language {
        "python" => &["python3", "python"],
        _ => &["node"],
    }
}

fn kernel_hint(language: &str) -> &'static str {
    match language {
        "python" => {
            "install Python 3 (for example `uv python install` or your system package manager) \
             and make sure `python3` or `python` is on PATH"
        }
        _ => "install Node.js and make sure `node` is on PATH",
    }
}

fn is_executable_file(path: &Path) -> bool {
    match fs::metadata(path) {
        Ok(metadata) if metadata.is_file() => {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                metadata.permissions().mode() & 0o111 != 0
            }
            #[cfg(not(unix))]
            {
                true
            }
        }
        _ => false,
    }
}

fn find_program(language: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for name in program_candidates(language) {
        for directory in std::env::split_paths(&path_var) {
            let candidate = directory.join(name);
            if is_executable_file(&candidate) {
                return Some(candidate);
            }
        }
    }
    None
}

fn probe_version(program: &Path) -> Option<String> {
    let mut command = Command::new(program);
    command.arg("--version").env_clear();
    if let Some(path) = std::env::var_os("PATH") {
        command.env("PATH", path);
    }
    let captured =
        spawn_and_capture(&mut command, DETECT_TIMEOUT, DETECT_OUTPUT_LIMIT, None).ok()?;
    let WaitOutcome::Exited(exit) = captured.outcome else {
        return None;
    };
    if !exit.success() {
        return None;
    }
    let stdout = String::from_utf8_lossy(&captured.stdout);
    let first = stdout.lines().next().unwrap_or("").trim();
    if !first.is_empty() {
        return Some(first.to_string());
    }
    let stderr = String::from_utf8_lossy(&captured.stderr);
    let first = stderr.lines().next().unwrap_or("").trim();
    if first.is_empty() {
        None
    } else {
        Some(first.to_string())
    }
}

fn request_envelope(request: &RunRequest) -> Result<Vec<u8>, ForeignError> {
    let envelope = serde_json::json!({
        "wire_version": marqdo_value::WIRE_VERSION,
        "kind": REQUEST_KIND,
        "input": request.input.clone().unwrap_or_default(),
    });
    serde_json::to_vec(&envelope).map_err(|error| ForeignError::Io {
        message: format!("cannot encode the foreign request envelope: {error}"),
    })
}

/// Private temporary program file plus a fresh private working directory.
struct TempArtifacts {
    program_path: PathBuf,
    work_dir: PathBuf,
}

impl TempArtifacts {
    fn create(language: &str) -> Result<Self, ForeignError> {
        let base = fs::canonicalize(std::env::temp_dir()).unwrap_or_else(|_| std::env::temp_dir());
        let extension = if language == "python" { "py" } else { "js" };
        let mut last_error: Option<io::Error> = None;
        for _ in 0..16 {
            let token = unique_token();
            let work_dir = base.join(format!("marqdo-foreign-work-{token}"));
            match create_private_dir(&work_dir) {
                Ok(()) => {
                    let program_path =
                        base.join(format!("marqdo-foreign-program-{token}.{extension}"));
                    return Ok(Self {
                        program_path,
                        work_dir,
                    });
                }
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                    last_error = Some(error);
                }
                Err(error) => {
                    return Err(ForeignError::Io {
                        message: format!(
                            "cannot create the foreign working directory `{}`: {error}",
                            work_dir.display()
                        ),
                    });
                }
            }
        }
        Err(ForeignError::Io {
            message: format!(
                "cannot allocate a unique foreign temporary directory: {}",
                last_error
                    .map(|error| error.to_string())
                    .unwrap_or_else(|| "name collision".to_string())
            ),
        })
    }

    fn write_program(&self, source: &str) -> Result<(), ForeignError> {
        write_private_file(&self.program_path, source.as_bytes()).map_err(|error| {
            ForeignError::Io {
                message: format!(
                    "cannot write the foreign program `{}`: {error}",
                    self.program_path.display()
                ),
            }
        })
    }
}

impl Drop for TempArtifacts {
    fn drop(&mut self) {
        let _ = fs::remove_file(&self.program_path);
        let _ = fs::remove_dir_all(&self.work_dir);
    }
}

fn create_private_dir(path: &Path) -> io::Result<()> {
    let mut builder = fs::DirBuilder::new();
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(path)
}

fn write_private_file(path: &Path, bytes: &[u8]) -> io::Result<()> {
    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(path)?;
    file.write_all(bytes)?;
    file.flush()
}

fn unique_token() -> String {
    let counter = UNIQUE_COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_nanos())
        .unwrap_or(0);
    format!("{}-{nanos}-{counter}", std::process::id())
}

/// How the supervised child ended.
enum WaitOutcome {
    /// The child was reaped normally (exit code or signal).
    Exited(ExitStatus),
    /// The deadline expired and the child was killed.
    TimedOut,
}

struct Captured {
    outcome: WaitOutcome,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
    duration: Duration,
}

struct ReaderState {
    data: Arc<Mutex<Vec<u8>>>,
    done: Arc<AtomicBool>,
    handle: Option<thread::JoinHandle<()>>,
}

fn spawn_reader<R: Read + Send + 'static>(
    mut stream: R,
    total: Arc<AtomicUsize>,
    overflow: Arc<AtomicBool>,
    limit: usize,
) -> Result<ReaderState, ForeignError> {
    let data = Arc::new(Mutex::new(Vec::new()));
    let done = Arc::new(AtomicBool::new(false));
    let thread_data = Arc::clone(&data);
    let thread_done = Arc::clone(&done);
    let handle = thread::Builder::new()
        .name("marqdo-foreign-output-reader".to_string())
        .spawn(move || {
            let mut buffer = [0_u8; 8192];
            loop {
                match stream.read(&mut buffer) {
                    Ok(0) => break,
                    Ok(read) => {
                        let previous = total.fetch_add(read, Ordering::AcqRel);
                        let allowed = limit.saturating_sub(previous).min(read);
                        if allowed > 0 {
                            if let Ok(mut guard) = thread_data.lock() {
                                guard.extend_from_slice(&buffer[..allowed]);
                            }
                        }
                        if allowed < read {
                            overflow.store(true, Ordering::Release);
                            break;
                        }
                    }
                    Err(ref error) if error.kind() == io::ErrorKind::Interrupted => continue,
                    Err(_) => break,
                }
            }
            thread_done.store(true, Ordering::Release);
        })
        .map_err(|error| ForeignError::Io {
            message: format!("cannot start the foreign output reader: {error}"),
        })?;
    Ok(ReaderState {
        data,
        done,
        handle: Some(handle),
    })
}

/// Spawn `command`, write the optional stdin payload, capture both output
/// streams within `max_output_bytes`, and supervise the child until it exits or
/// the deadline expires.
fn spawn_and_capture(
    command: &mut Command,
    timeout: Duration,
    max_output_bytes: usize,
    stdin_payload: Option<Vec<u8>>,
) -> Result<Captured, ForeignError> {
    command
        .stdin(if stdin_payload.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    let start = Instant::now();
    let mut child = command.spawn().map_err(|error| ForeignError::Spawn {
        message: error.to_string(),
    })?;

    let (Some(stdout), Some(stderr)) = (child.stdout.take(), child.stderr.take()) else {
        kill_and_reap(&mut child);
        return Err(ForeignError::Io {
            message: "the child stdio pipes could not be captured".to_string(),
        });
    };

    if let Some(payload) = stdin_payload {
        let Some(mut stdin) = child.stdin.take() else {
            kill_and_reap(&mut child);
            return Err(ForeignError::Io {
                message: "the child stdin pipe could not be captured".to_string(),
            });
        };
        // Writing must not block the supervisor, so it happens on its own
        // thread; closing the handle at the end signals EOF to the kernel.
        let writer = thread::Builder::new()
            .name("marqdo-foreign-stdin-writer".to_string())
            .spawn(move || {
                let _ = stdin.write_all(&payload);
                let _ = stdin.flush();
            });
        if let Err(error) = writer {
            kill_and_reap(&mut child);
            return Err(ForeignError::Io {
                message: format!("cannot start the foreign stdin writer: {error}"),
            });
        }
    }

    let total = Arc::new(AtomicUsize::new(0));
    let overflow = Arc::new(AtomicBool::new(false));
    let stdout_reader = match spawn_reader(
        stdout,
        Arc::clone(&total),
        Arc::clone(&overflow),
        max_output_bytes,
    ) {
        Ok(reader) => reader,
        Err(error) => {
            kill_and_reap(&mut child);
            return Err(error);
        }
    };
    let stderr_reader = match spawn_reader(
        stderr,
        Arc::clone(&total),
        Arc::clone(&overflow),
        max_output_bytes,
    ) {
        Ok(reader) => reader,
        Err(error) => {
            kill_and_reap(&mut child);
            drop(stdout_reader);
            return Err(error);
        }
    };

    let deadline = start.checked_add(timeout);
    let mut timed_out = false;
    let mut exit_status: Option<ExitStatus> = None;
    loop {
        if overflow.load(Ordering::Acquire) {
            kill_and_reap(&mut child);
            break;
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                exit_status = Some(status);
                break;
            }
            Ok(None) => {}
            Err(error) => {
                kill_and_reap(&mut child);
                return Err(ForeignError::Io {
                    message: format!("waiting for the foreign kernel failed: {error}"),
                });
            }
        }
        if let Some(deadline) = deadline {
            if Instant::now() >= deadline {
                kill_and_reap(&mut child);
                timed_out = true;
                break;
            }
        }
        thread::sleep(POLL_INTERVAL);
    }

    let stdout = collect_reader(stdout_reader);
    let stderr = collect_reader(stderr_reader);
    let duration = start.elapsed();

    if overflow.load(Ordering::Acquire) {
        return Err(ForeignError::OutputTooLarge {
            limit: max_output_bytes,
        });
    }

    let outcome = if timed_out {
        WaitOutcome::TimedOut
    } else if let Some(status) = exit_status {
        WaitOutcome::Exited(status)
    } else {
        // Unreachable: the loop only exits with one of the two states above.
        return Err(ForeignError::Io {
            message: "the foreign kernel ended in an unknown state".to_string(),
        });
    };

    Ok(Captured {
        outcome,
        stdout,
        stderr,
        duration,
    })
}

fn collect_reader(state: ReaderState) -> Vec<u8> {
    let ReaderState { data, done, handle } = state;
    let deadline = Instant::now().checked_add(READER_GRACE);
    while !done.load(Ordering::Acquire) {
        match deadline {
            Some(deadline) if Instant::now() >= deadline => break,
            _ => thread::sleep(Duration::from_millis(1)),
        }
    }
    let bytes = data.lock().map(|guard| guard.clone()).unwrap_or_default();
    if done.load(Ordering::Acquire) {
        if let Some(handle) = handle {
            let _ = handle.join();
        }
    }
    bytes
}

fn kill_and_reap(child: &mut Child) {
    let _ = child.kill();
    let _ = child.wait();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn languages_are_normalized() {
        assert_eq!(normalize_language("python"), Some("python"));
        assert_eq!(normalize_language(" Python3 "), Some("python"));
        assert_eq!(normalize_language("py"), Some("python"));
        assert_eq!(normalize_language("javascript"), Some("javascript"));
        assert_eq!(normalize_language("JS"), Some("javascript"));
        assert_eq!(normalize_language("node"), Some("javascript"));
        assert_eq!(normalize_language("cobol"), None);
        assert_eq!(normalize_language(""), None);
    }

    #[test]
    fn request_envelope_is_versioned() {
        let mut request = RunRequest::new("python", "print(1)");
        request.input = Some(serde_json::json!({"r": 2}));
        let payload = request_envelope(&request).expect("encode");
        let decoded: Json = serde_json::from_slice(&payload).expect("decode");
        assert_eq!(decoded["wire_version"], marqdo_value::WIRE_VERSION);
        assert_eq!(decoded["kind"], REQUEST_KIND);
        assert_eq!(decoded["input"]["r"], 2);

        let request = RunRequest::new("python", "");
        let payload = request_envelope(&request).expect("encode");
        let decoded: Json = serde_json::from_slice(&payload).expect("decode");
        assert!(decoded["input"].is_null());
    }
}
