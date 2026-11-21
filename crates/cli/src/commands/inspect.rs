//! `marqdo inspect` command.

use super::*;

/// Options for `marqdo inspect`.
#[derive(Debug, Clone, Default)]
pub struct InspectFlags {
    /// Show the Markdown AST.
    pub ast: bool,
    /// Show heading units.
    pub units: bool,
    /// Show inferred/explicit signatures.
    pub signature: bool,
    /// Show executable IR.
    pub ir: bool,
    /// Show link reference edges.
    pub refs: bool,
    /// Show platform capability report.
    pub platform: bool,
    /// Show declared/loaded extension identities (never grants).
    pub extensions: bool,
}

pub fn inspect(file: Option<&Path>, flags: &InspectFlags, json: bool, plugins: &[PathBuf]) -> i32 {
    let prepared = match prepare_project(file, json, plugins) {
        Ok(prepared) => prepared,
        Err(code) => return code,
    };
    let project = &prepared.loaded;
    let entry = project.entry_file();
    let module = &entry.analysis.module;
    let selected_any = flags.ast
        || flags.units
        || flags.signature
        || flags.ir
        || flags.refs
        || flags.platform
        || flags.extensions;
    if json {
        let mut object = serde_json::Map::new();
        object.insert("command".into(), json!("inspect"));
        object.insert("module".into(), json!(entry.module_id.to_string()));
        if !selected_any {
            object.insert("summary".into(), summary_json(project));
        }
        if flags.ast {
            object.insert(
                "ast".into(),
                serde_json::to_value(&entry.analysis.document.root).unwrap_or_default(),
            );
        }
        if flags.units {
            object.insert(
                "units".into(),
                serde_json::to_value(&entry.analysis.units.units).unwrap_or_default(),
            );
        }
        if flags.signature {
            object.insert("signatures".into(), signatures_json(module));
        }
        if flags.ir {
            object.insert(
                "ir".into(),
                serde_json::to_value(&module.functions).unwrap_or_default(),
            );
        }
        if flags.refs {
            object.insert("refs".into(), json!(refs_json(project)));
        }
        if flags.platform {
            object.insert(
                "platform".into(),
                serde_json::to_value(PlatformCapabilities::detect()).unwrap_or_default(),
            );
        }
        if flags.extensions {
            object.insert("extensions".into(), extensions_json(&prepared, project));
        }
        output::print_json(&serde_json::Value::Object(object));
        return 0;
    }
    if !selected_any {
        println!("module: {}", entry.module_id);
        println!("path: {}", entry.path.display());
        println!("project root: {}", project.root.display());
        println!("functions:");
        for function in &module.functions {
            let params = function
                .params
                .iter()
                .map(|param| param.name.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let kind = match function.signature {
                marqdo_ir::SignatureKind::Explicit => "explicit",
                marqdo_ir::SignatureKind::Inferred => "inferred",
            };
            println!("  {} ({params}) [{kind}]", function.id.name);
        }
        println!(
            "resources: {} formula/code block(s)",
            module.resources.len()
        );
        return 0;
    }
    if flags.ast {
        println!(
            "{}",
            serde_json::to_string_pretty(&entry.analysis.document.root).unwrap_or_default()
        );
    }
    if flags.units {
        println!(
            "{}",
            serde_json::to_string_pretty(&entry.analysis.units.units).unwrap_or_default()
        );
    }
    if flags.signature {
        println!(
            "{}",
            serde_json::to_string_pretty(&signatures_json(module)).unwrap_or_default()
        );
    }
    if flags.ir {
        println!(
            "{}",
            serde_json::to_string_pretty(&module.functions).unwrap_or_default()
        );
    }
    if flags.refs {
        println!(
            "{}",
            serde_json::to_string_pretty(&refs_json(project)).unwrap_or_default()
        );
    }
    if flags.platform {
        println!(
            "{}",
            serde_json::to_string_pretty(&PlatformCapabilities::detect()).unwrap_or_default()
        );
    }
    if flags.extensions {
        let value = extensions_json(&prepared, project);
        println!(
            "{}",
            serde_json::to_string_pretty(&value).unwrap_or_default()
        );
    }
    0
}

/// Static extension identity projection for `inspect --extensions`.
///
/// Reports declared sources, lock identity, static plugin descriptors and
/// official stdlib aliases. It deliberately never shows environment variables,
/// secrets, binary contents or an authorization result beyond
/// `not_granted` (inspection does not grant anything).
fn extensions_json(
    prepared: &crate::project_prepare::PreparedProgram,
    project: &crate::loader::LoadedProject,
) -> serde_json::Value {
    let root = project.root.as_path();
    let relative = |path: &Path| {
        path.strip_prefix(root)
            .map(|value| value.to_string_lossy().to_string())
            .unwrap_or_else(|_| path.to_string_lossy().to_string())
    };
    let declared: Vec<serde_json::Value> = project
        .extension_sources
        .iter()
        .map(|extension| {
            let identity = prepared.extension_identities.iter().find(|identity| {
                identity.kind == extension.kind
                    && identity.source == extension.from
                    && identity.logical_owner == extension.logical_owner
            });
            let locked = project.lock.as_ref().is_some_and(|lock| {
                identity.is_some_and(|identity| {
                    lock.extension(identity.kind, &identity.name, &identity.logical_owner)
                        .is_some()
                })
            });
            json!({
                "kind": extension.kind.as_str(),
                "source": extension.from,
                "path": relative(&extension.path),
                "logical_owner": extension.logical_owner,
                "name": identity.map(|identity| identity.name.clone()),
                "version": identity.map(|identity| identity.version.clone()),
                "manifest_digest": identity.map(|identity| identity.manifest_digest.clone()),
                "artifact_digest": identity.map(|identity| identity.artifact_digest.clone()),
                "selected_target": identity.map(|identity| identity.selected_target.clone()),
                "status": if locked { "locked" } else { "unlocked" },
                "authorization": "not_granted",
            })
        })
        .collect();
    let plugins: Vec<serde_json::Value> = prepared
        .plugins
        .plugins
        .iter()
        .map(|plugin| {
            json!({
                "package": plugin.package,
                "version": plugin.version,
                "kind": plugin.kind.as_str(),
                "manifest_path": relative(&plugin.manifest_path),
                "manifest_digest": plugin.manifest_sha256,
                "selected_target": plugin.artifact_target,
                "artifact_digest": plugin.artifact_sha256,
                "artifact_path": relative(&plugin.artifact_path()),
                "exports": plugin.exports.iter().map(|export| json!({
                    "name": export.name,
                    "signature": export.signature,
                })).collect::<Vec<_>>(),
                "validation": "static_manifest_ok",
                "authorization": "not_granted",
            })
        })
        .collect();
    let aliases: Vec<serde_json::Value> = project
        .official_aliases
        .iter()
        .map(|alias| {
            json!({
                "public_names": alias.public_names,
                "target": alias.function.to_string(),
                "owner_package": alias.owner_package,
                "source": "verified stdlib bundle",
                "authorization": "official_bundle",
            })
        })
        .collect();
    json!({
        "declared": declared,
        "plugins": plugins,
        "stdlib_aliases": aliases,
        "note": "inspect never loads or executes an extension and never grants a permission",
    })
}
