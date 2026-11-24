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
            // must not be able to bind `run` to a previous Analysis after the
            // project stopped loading or compiling.
            if let Some(path) = emit {
                let _ = std::fs::remove_file(path);
            }
            return code;
        }
    };
    let mut project = prepared.loaded;
    if refs {
        let ref_diagnostics = validate_refs(&project);
        project.diagnostics.extend(ref_diagnostics);
    }
    let errors = output::error_count(&project.diagnostics);
    if let Some(path) = emit {
        if errors > 0 {
            let _ = std::fs::remove_file(path);
        } else if let Err(error) = crate::analysis::write(&project, path) {
            eprintln!("error: {error}");
            return 2;
        }
    }
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
