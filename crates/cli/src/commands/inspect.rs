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
}

pub fn inspect(file: Option<&Path>, flags: &InspectFlags, json: bool) -> i32 {
    let prepared = match prepare_project(file, json, &[]) {
        Ok(prepared) => prepared,
        Err(code) => return code,
    };
    let project = prepared.loaded;
    let entry = project.entry_file();
    let module = &entry.analysis.module;
    let selected_any =
        flags.ast || flags.units || flags.signature || flags.ir || flags.refs || flags.platform;
    if json {
        let mut object = serde_json::Map::new();
        object.insert("command".into(), json!("inspect"));
        object.insert("module".into(), json!(entry.module_id.to_string()));
        if !selected_any {
            object.insert("summary".into(), summary_json(&project));
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
            object.insert("refs".into(), json!(refs_json(&project)));
        }
        if flags.platform {
            object.insert(
                "platform".into(),
                serde_json::to_value(PlatformCapabilities::detect()).unwrap_or_default(),
            );
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
            serde_json::to_string_pretty(&refs_json(&project)).unwrap_or_default()
        );
    }
    if flags.platform {
        println!(
            "{}",
            serde_json::to_string_pretty(&PlatformCapabilities::detect()).unwrap_or_default()
        );
    }
    0
}
