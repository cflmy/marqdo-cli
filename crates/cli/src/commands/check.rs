//! `marqdo check` command.

use super::*;

pub fn check(
    file: Option<&Path>,
    json: bool,
    refs: bool,
    plugins: &[PathBuf],
    emit: Option<&Path>,
) -> i32 {
    let prepared = match prepare_project(file, json, plugins) {
        Ok(prepared) => prepared,
        Err(code) => {
            // A failed check never leaves a success artifact behind: a consumer
            // must not bind `run` to a previous Analysis after the project
            // stopped loading. The input document is never the cleanup target.
            if let Some(path) = emit {
                if let Err(error) = crate::analysis::discard_without_prepare(file, path) {
                    eprintln!("error: {error}");
                    return 2;
                }
            }
            return code;
        }
    };
    let mut prepared = prepared;
    if refs {
        let report = crate::document_refs::references_for_project(
            &prepared.loaded,
            crate::document_refs::ReferenceMode::VerifyLocal,
        );
        for issue in report.issues {
            let Some(file) = prepared.loaded.files.get(&issue.module) else {
                continue;
            };
            prepared.loaded.diagnostics.push(FileDiagnostic {
                module_id: issue.module,
                path: file.path.clone(),
                source: file.source.clone(),
                diagnostic: issue.diagnostic,
            });
        }
    }
    let errors = output::error_count(&prepared.loaded.diagnostics);
    if let Some(path) = emit {
        if errors > 0 {
            if let Err(error) = crate::analysis::discard(&prepared, path) {
                eprintln!("error: {error}");
                return 2;
            }
        } else if let Err(error) = crate::analysis::write(&prepared, path) {
            eprintln!("error: {error}");
            return 2;
        }
    }
    let project = &prepared.loaded;
    if json {
        output::print_json(&json!({
            "command": "check",
            "success": errors == 0,
            "errors": errors,
            "diagnostics": output::diagnostics_json(&project.diagnostics),
        }));
    } else {
        let text = output::render_text(&project.diagnostics);
        eprint!("{text}");
        if errors == 0 {
            let checked = project
                .program
                .modules
                .keys()
                .filter(|id| !id.instance.starts_with("std:"))
                .count();
            println!("checked {checked} module(s): ok");
        } else {
            eprintln!(
                "check failed: {} error(s), {} warning(s)",
                errors,
                project.diagnostics.len() - errors
            );
        }
    }
    if errors == 0 {
        0
    } else {
        1
    }
}
