//! Generic external CLI tool protocol (spec/tool-v1.md).
//!
//! The core never knows a tool's name. It validates a user-supplied
//! `tool.json`, verifies protocol compatibility, copies the whole bundle into a
//! controlled user directory and dispatches unknown top-level commands to the
//! registered executable. This is deliberately separate from C ABI plugins and
//! foreign kernels.

use std::collections::BTreeMap;
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};
use std::process::Command as ProcessCommand;

use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const SCHEMA: &str = "marqdo.cli-tool";
const PROTOCOL_VERSION: u32 = 1;
const RECORD_VERSION: u32 = 2;
const MAX_BUNDLE_FILES: usize = 4096;
const MAX_BUNDLE_BYTES: u64 = 256 * 1024 * 1024;

/// Protocols this core can provide to tools.
#[derive(Debug, Clone, Copy)]
struct SupportedProtocols {
    tool: u32,
    analysis: &'static [u32],
    trace: &'static [u32],
}

const SUPPORTED: SupportedProtocols = SupportedProtocols {
    tool: PROTOCOL_VERSION,
    analysis: &[2],
    trace: &[2],
};

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

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct RecordedCompatibility {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    marqdo: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    analysis: Option<Vec<u32>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    trace: Option<Vec<u32>>,
}

impl RecordedCompatibility {
    fn as_compatibility(&self) -> Compatibility {
        Compatibility {
            marqdo: self.marqdo.clone(),
            analysis: self.analysis.clone(),
            trace: self.trace.clone(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct BundleEntry {
    path: String,
    size: u64,
    sha256: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct ToolRecord {
    record_version: u32,
    command: String,
    name: String,
    version: String,
    entrypoint: PathBuf,
    bundle: Vec<BundleEntry>,
    bundle_sha256: String,
    source: String,
    schema_version: u32,
    #[serde(default)]
    compatibility: RecordedCompatibility,
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
                        "bundle_files": record.bundle.len(),
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
    if package_dir.join("record.json").exists() {
        return Err("tool package must not contain a root record.json".to_string());
    }
    let entrypoint_path = safe_entrypoint(&package_dir, &manifest.entrypoint)?;
    let entrypoint = relative_string(&package_dir, &entrypoint_path)?;
    let bundle = collect_bundle(&package_dir, false)?;
    if !bundle.iter().any(|entry| entry.path == entrypoint) {
        return Err("tool entrypoint is not part of the verified bundle".to_string());
    }
    let bundle_sha256 = bundle_digest(&bundle);

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
        copy_bundle(&package_dir, &staging, &bundle)?;
        let copied = collect_bundle(&staging, false)?;
        if copied != bundle || bundle_digest(&copied) != bundle_sha256 {
            return Err("copied tool bundle does not match the source bundle".to_string());
        }
        let record = ToolRecord {
            record_version: RECORD_VERSION,
            command: manifest.command.clone(),
            name: manifest.name.clone(),
            version: manifest.version.clone(),
            entrypoint: PathBuf::from(&entrypoint),
            bundle: bundle.clone(),
            bundle_sha256: bundle_sha256.clone(),
            source: manifest_file.display().to_string(),
            schema_version: manifest.schema_version,
            compatibility: RecordedCompatibility {
                marqdo: manifest.compatibility.marqdo.clone(),
                analysis: manifest.compatibility.analysis.clone(),
                trace: manifest.compatibility.trace.clone(),
            },
        };
        let record_json = serde_json::to_string_pretty(&record)
            .map_err(|error| format!("cannot serialize record: {error}"))?;
        std::fs::write(staging.join("record.json"), record_json)
            .map_err(|error| format!("cannot write record: {error}"))?;
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
    if manifest.schema_version != SUPPORTED.tool {
        return Err(format!(
            "unsupported tool schema_version {} (expected {})",
            manifest.schema_version, SUPPORTED.tool
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
    if manifest.name.trim().is_empty() {
        return Err("tool name must not be empty".to_string());
    }
    Version::parse(&manifest.version).map_err(|error| {
        format!(
            "tool version `{}` is not valid SemVer: {error}",
            manifest.version
        )
    })?;
    negotiate(&manifest.version, &manifest.compatibility)
}

fn negotiate(tool_version: &str, compatibility: &Compatibility) -> Result<(), String> {
    Version::parse(tool_version)
        .map_err(|error| format!("tool version `{tool_version}` is not valid SemVer: {error}"))?;
    let core_version = Version::parse(env!("CARGO_PKG_VERSION"))
        .map_err(|error| format!("core version is not valid SemVer: {error}"))?;
    if let Some(range) = &compatibility.marqdo {
        let requirement = VersionReq::parse(range)
            .map_err(|error| format!("invalid marqdo compatibility range `{range}`: {error}"))?;
        if !requirement.matches(&core_version) {
            return Err(format!(
                "tool requires marqdo `{range}`, this core is {}",
                env!("CARGO_PKG_VERSION")
            ));
        }
    }
    negotiate_protocol(
        "analysis",
        compatibility.analysis.as_deref(),
        SUPPORTED.analysis,
    )?;
    negotiate_protocol("trace", compatibility.trace.as_deref(), SUPPORTED.trace)?;
    Ok(())
}

fn negotiate_protocol(
    name: &str,
    requested: Option<&[u32]>,
    supported: &[u32],
) -> Result<(), String> {
    let Some(requested) = requested else {
        return Ok(());
    };
    if requested.is_empty() || !requested.iter().any(|version| supported.contains(version)) {
        return Err(format!(
            "tool.protocol_incompatible: tool requires {name} version(s) {requested:?}, this core supports {supported:?}"
        ));
    }
    Ok(())
}

fn is_valid_command(command: &str) -> bool {
    let mut bytes = command.bytes();
    matches!(bytes.next(), Some(byte) if byte.is_ascii_lowercase())
        && bytes.all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn relative_string(root: &Path, path: &Path) -> Result<String, String> {
    let relative = path
        .strip_prefix(root)
        .map_err(|_| "tool package path escaped its root".to_string())?;
    let mut parts = Vec::new();
    for component in relative.components() {
        match component {
            Component::Normal(part) => parts.push(
                part.to_str()
                    .ok_or_else(|| "tool package path is not valid UTF-8".to_string())?
                    .to_string(),
            ),
            _ => return Err("tool package path is not normalized".to_string()),
        }
    }
    if parts.is_empty() {
        return Err("tool package path is empty".to_string());
    }
    Ok(parts.join("/"))
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

fn collect_bundle(root: &Path, exclude_record: bool) -> Result<Vec<BundleEntry>, String> {
    if !root.is_dir() {
        return Err(format!(
            "tool package `{}` is not a directory",
            root.display()
        ));
    }
    let mut entries = Vec::new();
    let mut total = 0u64;
    for entry in walkdir::WalkDir::new(root)
        .follow_links(false)
        .sort_by_file_name()
    {
        let entry = entry.map_err(|error| format!("cannot walk tool package: {error}"))?;
        if entry.depth() == 0 {
            continue;
        }
        let path = relative_string(root, entry.path())?;
        if exclude_record && path == "record.json" {
            continue;
        }
        if entry.file_type().is_symlink() {
            return Err(format!(
                "tool package contains a symlink `{}`; symlinks are not installed",
                entry.path().display()
            ));
        }
        if entry.file_type().is_dir() {
            continue;
        }
        if !entry.file_type().is_file() {
            return Err(format!(
                "tool package contains special file `{}`",
                entry.path().display()
            ));
        }
        let size = entry
            .metadata()
            .map_err(|error| format!("cannot inspect `{}`: {error}", entry.path().display()))?
            .len();
        total = total
            .checked_add(size)
            .ok_or_else(|| "tool bundle size overflowed".to_string())?;
        if total > MAX_BUNDLE_BYTES {
            return Err(format!(
                "tool bundle exceeds the maximum size of {MAX_BUNDLE_BYTES} bytes"
            ));
        }
        if entries.len() >= MAX_BUNDLE_FILES {
            return Err(format!(
                "tool bundle exceeds the maximum of {MAX_BUNDLE_FILES} files"
            ));
        }
        entries.push(BundleEntry {
            path,
            size,
            sha256: hash_file(entry.path())?,
        });
    }
    entries.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(entries)
}

fn bundle_digest(entries: &[BundleEntry]) -> String {
    let mut hasher = Sha256::new();
    for entry in entries {
        hasher.update(entry.path.as_bytes());
        hasher.update([0u8]);
        hasher.update(entry.size.to_le_bytes());
        hasher.update(entry.sha256.as_bytes());
        hasher.update(b"\n");
    }
    hex::encode(hasher.finalize())
}

fn copy_bundle(source: &Path, target: &Path, bundle: &[BundleEntry]) -> Result<(), String> {
    for entry in bundle {
        let destination = target.join(&entry.path);
        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|error| format!("cannot create `{}`: {error}", parent.display()))?;
        }
        std::fs::copy(source.join(&entry.path), &destination).map_err(|error| {
            format!(
                "cannot copy `{}` to `{}`: {error}",
                source.join(&entry.path).display(),
                destination.display()
            )
        })?;
    }
    Ok(())
}

/// `marqdo tool list`.
pub fn list(json: bool) -> i32 {
    match installed_records() {
        Ok(records) => {
            if json {
                let tools: Vec<serde_json::Value> = records
                    .iter()
                    .map(|record| {
                        serde_json::json!({
                            "command": record.command,
                            "name": record.name,
                            "version": record.version,
                            "bundle_files": record.bundle.len(),
                            "compatibility": record.compatibility,
                        })
                    })
                    .collect();
                println!(
                    "{}",
                    serde_json::json!({"command":"tool.list","success":true,"tools":tools})
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
        let canonical_home = home
            .canonicalize()
            .map_err(|error| format!("cannot open tool home: {error}"))?;
        let canonical_target = target
            .canonicalize()
            .map_err(|error| format!("cannot open tool installation: {error}"))?;
        if canonical_target.parent() != Some(canonical_home.as_path()) {
            return Err("refusing to remove a path outside the tool home".to_string());
        }
        let trash =
            canonical_home.join(format!(".trash-{}-{}", std::process::id(), unique_suffix()));
        std::fs::rename(&canonical_target, &trash)
            .map_err(|error| format!("cannot detach tool `{command}`: {error}"))?;
        std::fs::remove_dir_all(&trash)
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
    if record.command != command {
        eprintln!("error: tool record does not match command `{command}`; reinstall it");
        return 1;
    }
    if let Err(error) = negotiate(&record.version, &record.compatibility.as_compatibility()) {
        eprintln!("error: tool `{command}` is not compatible with this core: {error}");
        return 1;
    }
    if record.record_version != RECORD_VERSION {
        eprintln!("error: tool `{command}` uses an old record; reinstall it");
        return 1;
    }
    let bundle = match collect_bundle(&dir, true) {
        Ok(bundle) => bundle,
        Err(error) => {
            eprintln!("error: {error}");
            return 1;
        }
    };
    if bundle != record.bundle || bundle_digest(&bundle) != record.bundle_sha256 {
        eprintln!("error: tool `{command}` bundle hash mismatch; reinstall it");
        return 1;
    }
    let entrypoint = match safe_record_entrypoint(&record).and_then(|relative| {
        if bundle.iter().any(|entry| entry.path == relative) {
            Ok(dir.join(relative))
        } else {
            Err("tool entrypoint is not part of the verified bundle".to_string())
        }
    }) {
        Ok(entrypoint) => entrypoint,
        Err(error) => {
            eprintln!("error: {error}; reinstall it");
            return 1;
        }
    };
    let mut process = ProcessCommand::new(&entrypoint);
    process.args(&args[1..]);
    if let Ok(bin) = std::env::current_exe() {
        process.env("MARQDO_BIN", bin);
    }
    process.env("MARQDO_TOOL_PROTOCOL", "1");
    if record
        .compatibility
        .analysis
        .as_ref()
        .is_some_and(|versions| versions.contains(&2))
    {
        process.env("MARQDO_PROTOCOL_ANALYSIS", "2");
    }
    if record
        .compatibility
        .trace
        .as_ref()
        .is_some_and(|versions| versions.contains(&2))
    {
        process.env("MARQDO_PROTOCOL_TRACE", "2");
    }
    match process.status() {
        Ok(status) => status.code().unwrap_or(1),
        Err(error) => {
            eprintln!("error: cannot start tool `{command}`: {error}");
            1
        }
    }
}

fn safe_record_entrypoint(record: &ToolRecord) -> Result<String, String> {
    let path = record
        .entrypoint
        .to_str()
        .ok_or_else(|| "tool record entrypoint is not valid UTF-8".to_string())?;
    if record.entrypoint.is_absolute() || path.is_empty() {
        return Err("tool record entrypoint must be package-relative".to_string());
    }
    let mut parts = Vec::new();
    for component in record.entrypoint.components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str().unwrap_or_default()),
            _ => return Err("tool record entrypoint must not escape the package".to_string()),
        }
    }
    Ok(parts.join("/"))
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
    let record: ToolRecord =
        serde_json::from_str(&text).map_err(|error| format!("invalid tool record: {error}"))?;
    if record.record_version != RECORD_VERSION {
        return Err("tool record is from an older protocol; reinstall it".to_string());
    }
    Ok(record)
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
