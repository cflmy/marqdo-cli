//! Generic external CLI tool protocol (spec/tool-v1.md).
//!
//! The core never knows a tool's name. It validates a user-supplied
//! `tool.json`, copies the package into a controlled user directory, and
//! dispatches unknown top-level commands to the registered executable. This is
//! deliberately separate from C ABI plugins and foreign kernels.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::process::Command as ProcessCommand;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SCHEMA: &str = "marqdo.cli-tool";
const PROTOCOL_VERSION: u32 = 1;
const BUILTIN_COMMANDS: &[&str] = &[
    "init", "run", "call", "check", "inspect", "fmt", "plugin", "add", "remove", "lock", "sync",
    "tool", "help",
];

#[derive(Debug, Clone, Deserialize)]
struct ToolManifest {
    schema: String,
    schema_version: u32,
    name: String,
    version: String,
    command: String,
    entrypoint: PathBuf,
    #[serde(default)]
    compatibility: Compatibility,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct Compatibility {
    #[serde(default)]
    marqdo: Option<String>,
    #[serde(default)]
    analysis: Option<Vec<u32>>,
    #[serde(default)]
    trace: Option<Vec<u32>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolRecord {
    command: String,
    name: String,
    version: String,
    entrypoint: PathBuf,
    sha256: String,
    source: String,
    schema_version: u32,
    #[serde(default)]
    compatibility: RecordedCompatibility,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RecordedCompatibility {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    marqdo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    analysis: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trace: Option<Vec<u32>>,
}

/// `marqdo tool install <manifest-or-directory>`.
pub fn install(manifest_path: &Path, json: bool) -> i32 {
    match install_inner(manifest_path) {
        Ok(record) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({
                        "command": "tool.install",
                        "success": true,
                        "tool": record.command,
                        "version": record.version,
                    })
                );
            } else {
                println!(
                    "installed tool `{}` version {} ({})",
                    record.command, record.version, record.name
                );
            }
            0
        }
        Err(error) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"command":"tool.install","success":false,"error":error})
                );
            } else {
                eprintln!("error: {error}");
            }
            1
        }
    }
}

fn install_inner(manifest_path: &Path) -> Result<ToolRecord, String> {
    let manifest_file = if manifest_path.is_dir() {
        manifest_path.join("tool.json")
    } else {
        manifest_path.to_path_buf()
    };
    let package_dir = manifest_file
        .parent()
        .ok_or_else(|| "manifest has no parent directory".to_string())?
        .canonicalize()
        .map_err(|error| format!("cannot open `{}`: {error}", manifest_file.display()))?;
    let manifest = ToolManifest::from_path(&manifest_file)?;
    validate_manifest(&manifest)?;
    let entrypoint = safe_entrypoint(&package_dir, &manifest.entrypoint)?;
    let sha256 = hash_file(&entrypoint)?;
    let home = tool_home()?;
    std::fs::create_dir_all(&home)
        .map_err(|error| format!("cannot create tool home `{}`: {error}", home.display()))?;
    let target = home.join(&manifest.command);
    if target.exists() {
        return Err(format!(
            "tool command `{}` is already installed; remove it first",
            manifest.command
        ));
    }
    let staging = home.join(format!(
        ".staging-{}-{}",
        std::process::id(),
        unique_suffix()
    ));
    if staging.exists() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    std::fs::create_dir_all(&staging)
        .map_err(|error| format!("cannot create staging directory: {error}"))?;
    let result = (|| -> Result<(), String> {
        copy_package(&package_dir, &staging)?;
        let relative = manifest.entrypoint.clone();
        let record = ToolRecord {
            command: manifest.command.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            entrypoint: relative.clone(),
            sha256: sha256.clone(),
            source: manifest_file.display().to_string(),
            schema_version: PROTOCOL_VERSION,
            compatibility: RecordedCompatibility {
                marqdo: manifest.compatibility.marqdo.clone(),
                analysis: manifest.compatibility.analysis.clone(),
                trace: manifest.compatibility.trace.clone(),
            },
        };
        let record_path = staging.join("record.json");
        let record_json = serde_json::to_string_pretty(&record)
            .map_err(|error| format!("cannot serialize record: {error}"))?;
        std::fs::write(&record_path, record_json)
            .map_err(|error| format!("cannot write record: {error}"))?;
        // Re-verify the copied entrypoint before publishing.
        let copied = staging.join(&relative);
        if hash_file(&copied)? != sha256 {
            return Err("copied tool entrypoint hash mismatch".to_string());
        }
        std::fs::rename(&staging, &target)
            .map_err(|error| format!("cannot publish tool installation: {error}"))?;
        Ok(())
    })();
    if result.is_err() {
        let _ = std::fs::remove_dir_all(&staging);
    }
    result.and_then(|()| load_record(&target))
}

impl ToolManifest {
    fn from_path(path: &Path) -> Result<Self, String> {
        let text = std::fs::read_to_string(path)
            .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
        let manifest: Self =
            serde_json::from_str(&text).map_err(|error| format!("invalid tool.json: {error}"))?;
        Ok(manifest)
    }
}

fn validate_manifest(manifest: &ToolManifest) -> Result<(), String> {
    if manifest.schema != SCHEMA {
        return Err(format!("unsupported tool schema `{}`", manifest.schema));
    }
    if manifest.schema_version != PROTOCOL_VERSION {
        return Err(format!(
            "unsupported tool schema_version {} (expected {PROTOCOL_VERSION})",
            manifest.schema_version
        ));
    }
    if !is_valid_command(&manifest.command) {
        return Err(format!(
            "tool command `{}` must match ^[a-z][a-z0-9-]*$",
            manifest.command
        ));
    }
    if BUILTIN_COMMANDS.contains(&manifest.command.as_str()) {
        return Err(format!(
            "tool command `{}` conflicts with a built-in command",
            manifest.command
        ));
    }
    if manifest.name.trim().is_empty() || manifest.version.trim().is_empty() {
        return Err("tool name and version must not be empty".to_string());
    }
    if let Some(range) = &manifest.compatibility.marqdo {
        if !version_matches(env!("CARGO_PKG_VERSION"), range) {
            return Err(format!(
                "tool requires marqdo `{range}`, this core is {}",
                env!("CARGO_PKG_VERSION")
            ));
        }
    }
    Ok(())
}

fn is_valid_command(command: &str) -> bool {
    let mut bytes = command.bytes();
    matches!(bytes.next(), Some(byte) if byte.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn safe_entrypoint(package_dir: &Path, entrypoint: &Path) -> Result<PathBuf, String> {
    if entrypoint.is_absolute() {
        return Err("tool entrypoint must be package-relative".to_string());
    }
    for component in entrypoint.components() {
        match component {
            Component::Normal(_) => {}
            Component::CurDir => {}
            _ => return Err("tool entrypoint must not escape the package".to_string()),
        }
    }
    let candidate = package_dir.join(entrypoint);
    let canonical = candidate.canonicalize().map_err(|error| {
        format!(
            "tool entrypoint `{}` cannot be opened: {error}",
            candidate.display()
        )
    })?;
    if !canonical.starts_with(package_dir) {
        return Err("tool entrypoint resolves outside the package".to_string());
    }
    if !canonical.is_file() {
        return Err("tool entrypoint is not a file".to_string());
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if std::fs::metadata(&canonical)
            .map(|metadata| metadata.permissions().mode() & 0o111 == 0)
            .unwrap_or(true)
        {
            return Err("tool entrypoint is not executable".to_string());
        }
    }
    Ok(canonical)
}

fn copy_package(source: &Path, target: &Path) -> Result<(), String> {
    for entry in walkdir::WalkDir::new(source).follow_links(false) {
        let entry = entry.map_err(|error| format!("cannot walk tool package: {error}"))?;
        let relative = entry
            .path()
            .strip_prefix(source)
            .map_err(|_| "tool package path escaped its root".to_string())?;
        let destination = target.join(relative);
        if entry.file_type().is_symlink() {
            return Err(format!(
                "tool package contains a symlink `{}`; symlinks are not installed",
                entry.path().display()
            ));
        }
        if entry.file_type().is_dir() {
            std::fs::create_dir_all(&destination)
                .map_err(|error| format!("cannot create `{}`: {error}", destination.display()))?;
        } else if entry.file_type().is_file() {
            if let Some(parent) = destination.parent() {
                std::fs::create_dir_all(parent)
                    .map_err(|error| format!("cannot create `{}`: {error}", parent.display()))?;
            }
            std::fs::copy(entry.path(), &destination).map_err(|error| {
                format!(
                    "cannot copy `{}` to `{}`: {error}",
                    entry.path().display(),
                    destination.display()
                )
            })?;
        }
    }
    Ok(())
}

/// `marqdo tool list`.
pub fn list(json: bool) -> i32 {
    match installed_records() {
        Ok(records) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"command":"tool.list","success":true,"tools":records})
                );
            } else if records.is_empty() {
                println!("no tools installed");
            } else {
                for record in records {
                    println!("{} {} ({})", record.command, record.version, record.name);
                }
            }
            0
        }
        Err(error) => {
            eprintln!("error: {error}");
            1
        }
    }
}

/// `marqdo tool remove <command>`.
pub fn remove(command: &str, json: bool) -> i32 {
    let result = (|| -> Result<(), String> {
        if !is_valid_command(command) {
            return Err(format!("invalid tool command `{command}`"));
        }
        let home = tool_home()?;
        let target = home.join(command);
        if !target.is_dir() {
            return Err(format!("tool `{command}` is not installed"));
        }
        // The target must be a direct child of the controlled home.
        let canonical_home = home
            .canonicalize()
            .map_err(|error| format!("cannot open tool home: {error}"))?;
        let canonical_target = target
            .canonicalize()
            .map_err(|error| format!("cannot open tool installation: {error}"))?;
        if canonical_target.parent() != Some(canonical_home.as_path()) {
            return Err("refusing to remove a path outside the tool home".to_string());
        }
        std::fs::remove_dir_all(&canonical_target)
            .map_err(|error| format!("cannot remove tool `{command}`: {error}"))?;
        Ok(())
    })();
    match result {
        Ok(()) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"command":"tool.remove","success":true,"tool":command})
                );
            } else {
                println!("removed tool `{command}`");
            }
            0
        }
        Err(error) => {
            if json {
                println!(
                    "{}",
                    serde_json::json!({"command":"tool.remove","success":false,"error":error})
                );
            } else {
                eprintln!("error: {error}");
            }
            1
        }
    }
}

/// Dispatch an unknown top-level command to a registered tool.
pub fn dispatch(args: &[OsString]) -> i32 {
    let Some(first) = args.first() else {
        eprintln!("error: missing command");
        return 2;
    };
    let command = first.to_string_lossy();
    if BUILTIN_COMMANDS.contains(&command.as_ref()) {
        eprintln!("error: internal dispatch error for built-in command `{command}`");
        return 2;
    }
    let home = match tool_home() {
        Ok(home) => home,
        Err(error) => {
            eprintln!("error: {error}");
            return 2;
        }
    };
    let dir = home.join(command.as_ref());
    let record = match load_record(&dir) {
        Ok(record) => record,
        Err(_) => {
            eprintln!(
                "error: unknown command `{command}`; install a tool with `marqdo tool install <tool.json>`"
            );
            return 2;
        }
    };
    let entrypoint = dir.join(&record.entrypoint);
    if !entrypoint.starts_with(&dir) {
        eprintln!("error: tool entrypoint escapes its installation");
        return 1;
    }
    let actual = match hash_file(&entrypoint) {
        Ok(hash) => hash,
        Err(error) => {
            eprintln!("error: {error}");
            return 1;
        }
    };
    if actual != record.sha256 {
        eprintln!("error: tool `{command}` executable hash mismatch; reinstall it");
        return 1;
    }
    let mut process = ProcessCommand::new(&entrypoint);
    process.args(&args[1..]);
    if let Ok(bin) = std::env::current_exe() {
        process.env("MARQDO_BIN", bin);
    }
    process.env("MARQDO_TOOL_PROTOCOL", "1");
    match process.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("error: cannot start tool `{command}`: {error}");
            1
        }
    }
}

fn installed_records() -> Result<Vec<ToolRecord>, String> {
    let home = tool_home()?;
    if !home.is_dir() {
        return Ok(Vec::new());
    }
    let mut records = BTreeMap::new();
    let entries = std::fs::read_dir(&home)
        .map_err(|error| format!("cannot read tool home `{}`: {error}", home.display()))?;
    for entry in entries {
        let entry = entry.map_err(|error| format!("cannot read tool entry: {error}"))?;
        if !entry.file_type().map(|kind| kind.is_dir()).unwrap_or(false) {
            continue;
        }
        if entry.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        if let Ok(record) = load_record(&entry.path()) {
            records.insert(record.command.clone(), record);
        }
    }
    Ok(records.into_values().collect())
}

fn load_record(dir: &Path) -> Result<ToolRecord, String> {
    let text = std::fs::read_to_string(dir.join("record.json"))
        .map_err(|error| format!("tool record is missing: {error}"))?;
    serde_json::from_str(&text).map_err(|error| format!("invalid tool record: {error}"))
}

fn tool_home() -> Result<PathBuf, String> {
    if let Some(path) = std::env::var_os("MARQDO_TOOL_HOME") {
        return Ok(PathBuf::from(path));
    }
    let base = if cfg!(windows) {
        std::env::var_os("APPDATA")
            .map(PathBuf::from)
            .ok_or_else(|| "APPDATA is not set; set MARQDO_TOOL_HOME".to_string())?
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".config")))
            .ok_or_else(|| "HOME is not set; set MARQDO_TOOL_HOME".to_string())?
    };
    Ok(base.join("marqdo").join("tools"))
}

fn hash_file(path: &Path) -> Result<String, String> {
    let bytes = std::fs::read(path)
        .map_err(|error| format!("cannot read `{}`: {error}", path.display()))?;
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    Ok(hex::encode(hasher.finalize()))
}

fn unique_suffix() -> u128 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|duration| duration.as_nanos())
        .unwrap_or(0)
}

/// Minimal version/range check for the `compatibility.marqdo` field.
fn version_matches(version: &str, range: &str) -> bool {
    let Some(parsed) = parse_version(version) else {
        return false;
    };
    range.split(',').all(|part| {
        let part = part.trim();
        if part.is_empty() {
            return true;
        }
        let (op, raw) = if let Some(rest) = part.strip_prefix(">=") {
            (">=", rest)
        } else if let Some(rest) = part.strip_prefix("<=") {
            ("<=", rest)
        } else if let Some(rest) = part.strip_prefix('>') {
            (">", rest)
        } else if let Some(rest) = part.strip_prefix('<') {
            ("<", rest)
        } else if let Some(rest) = part.strip_prefix('=') {
            ("=", rest)
        } else {
            ("=", part)
        };
        let Some(expected) = parse_version(raw.trim()) else {
            return false;
        };
        match op {
            ">=" => parsed >= expected,
            "<=" => parsed <= expected,
            ">" => parsed > expected,
            "<" => parsed < expected,
            _ => parsed == expected,
        }
    })
}

fn parse_version(value: &str) -> Option<(u64, u64, u64)> {
    let mut parts = value.split('.');
    let major = parts.next()?.parse().ok()?;
    let minor = parts.next().unwrap_or("0").parse().ok()?;
    let patch = parts
        .next()
        .unwrap_or("0")
        .split('-')
        .next()?
        .parse()
        .ok()?;
    Some((major, minor, patch))
}
