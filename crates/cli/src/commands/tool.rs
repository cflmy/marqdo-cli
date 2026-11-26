//! CLI formatter and plugin/tool inspection commands.

use super::*;

/// `marqdo fmt`.
pub fn fmt(file: Option<&Path>, check_only: bool) -> i32 {
    let file = file.unwrap_or_else(|| Path::new("index.mq.md"));
    let Ok(source) = std::fs::read_to_string(file) else {
        eprintln!("error: cannot read `{}`", file.display());
        return 2;
    };
    let mut formatted = source.replace("\r\n", "\n");
    while formatted.ends_with("\n\n") {
        formatted.pop();
    }
    if !formatted.ends_with('\n') {
        formatted.push('\n');
    }
    if formatted == source {
        return 0;
    }
    if check_only {
        eprintln!(
            "error: `{}` is not formatted (trailing whitespace/newlines or CRLF)",
            file.display()
        );
        return 1;
    }
    if let Err(error) = std::fs::write(file, formatted) {
        eprintln!("error: cannot write `{}`: {error}", file.display());
        return 1;
    }
    0
}

/// `marqdo plugin inspect`.
pub fn plugin_inspect(target: &Path, json: bool) -> i32 {
    let manifest_path = if target.is_dir() {
        target.join("plugin.yaml")
    } else {
        target.to_path_buf()
    };
    let base_dir = manifest_path
        .parent()
        .map(|parent| parent.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let manifest = match marqdo_host::plugin::PluginManifest::load(&manifest_path) {
        Ok(manifest) => manifest,
        Err(error) => {
            report_plugin_error(&error, json);
            return 1;
        }
    };
    if let Err(error) = manifest.validate() {
        report_plugin_error(&error, json);
        return 1;
    }
    let host_target = marqdo_host::plugin::HostTarget::current();
    let artifact = match manifest.verify_artifact(&base_dir, &host_target) {
        Ok((path, artifact)) => {
            let abi = match manifest.abi_requirement() {
                Ok(requirement) => {
                    if requirement.is_compatible_with(marqdo_plugin_abi::AbiVersion::current()) {
                        "compatible"
                    } else {
                        "incompatible"
                    }
                }
                Err(error) => {
                    report_plugin_error(&error, json);
                    return 1;
                }
            };
            (path, artifact, abi)
        }
        Err(error) => {
            report_plugin_error(&error, json);
            return 1;
        }
    };
    let (artifact_path, artifact_info, abi) = artifact;
    if json {
        output::print_json(&json!({
            "command": "plugin.inspect",
            "success": true,
            "plugin": manifest.plugin.name,
            "version": manifest.plugin.version,
            "abi": manifest.plugin.abi,
            "abi_compatibility": abi,
            "target": host_target.triple().as_str(),
            "artifact": {
                "file": artifact_info.file,
                "path": artifact_path.display().to_string(),
                "sha256": artifact_info.sha256,
                "verified": true,
            },
            "exports": manifest.plugin.exports.iter().map(|export| json!({
                "name": export.name,
                "signature": export.signature,
            })).collect::<Vec<_>>(),
            "loaded": false,
        }));
    } else {
        println!(
            "plugin: {} {}",
            manifest.plugin.name, manifest.plugin.version
        );
        println!("abi: {} ({abi})", manifest.plugin.abi);
        println!("target: {}", host_target.triple().as_str());
        println!(
            "artifact: {} (sha256 verified: {})",
            artifact_path.display(),
            artifact_info.sha256
        );
        println!("exports:");
        for export in &manifest.plugin.exports {
            println!("  {} {}", export.name, export.signature);
        }
        println!("loaded: false (inspection never executes plugin code)");
    }
    0
}

fn report_plugin_error(error: &marqdo_plugin_abi::PluginError, json: bool) {
    if json {
        output::print_json(&json!({
            "command": "plugin.inspect",
            "success": false,
            "error": {
                "kind": error.kind,
                "code": error.code,
                "message": error.message,
            }
        }));
    } else {
        eprintln!("error[{}]: {}", error.code, error.message);
    }
}
