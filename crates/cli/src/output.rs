//! Diagnostic rendering and JSON envelopes.

use serde_json::{json, Value as Json};

use crate::loader::FileDiagnostic;

/// Severity name.
fn severity_name(diagnostic: &marqdo_value::Diagnostic) -> &'static str {
    match diagnostic.severity {
        marqdo_value::Severity::Error => "error",
        marqdo_value::Severity::Warning => "warning",
        marqdo_value::Severity::Info => "info",
    }
}

/// Render diagnostics as human readable text (stderr).
pub fn render_text(diagnostics: &[FileDiagnostic]) -> String {
    let mut out = String::new();
    for item in diagnostics {
        let diagnostic = &item.diagnostic;
        let location = match item.line_col() {
            Some((line, column)) => format!("{}:{line}:{column}", item.path.display()),
            None => item.path.display().to_string(),
        };
        out.push_str(&format!(
            "{}[{}]: {}: {}\n",
            severity_name(diagnostic),
            diagnostic.code,
            location,
            diagnostic.message
        ));
        if let Some(line_text) = item.line_text() {
            let index = marqdo_span::LineIndex::new(&item.source);
            if let Some(span) = diagnostic.span {
                let position = index.line_col(span.start);
                let caret_offset = position.column.saturating_sub(1);
                out.push_str(&format!("  {}\n", line_text.trim_end()));
                out.push_str(&format!(
                    "  {}^\n",
                    " ".repeat(caret_offset.min(line_text.len()))
                ));
            }
        }
        for related in &diagnostic.related {
            let mut message = related.message.clone();
            if let Some(span) = related.span {
                let index = marqdo_span::LineIndex::new(&item.source);
                let position = index.line_col(span.start);
                message = format!("{}:{}: {}", position.line, position.column, message);
            }
            out.push_str(&format!("  note: {message}\n"));
        }
        for hint in &diagnostic.hints {
            out.push_str(&format!("  hint: {hint}\n"));
        }
    }
    out
}

/// Serialize one diagnostic with file/line/column metadata.
pub fn diagnostic_json(item: &FileDiagnostic) -> Json {
    let diagnostic = &item.diagnostic;
    let (line, column) = item
        .line_col()
        .map(|(line, column)| (Json::from(line), Json::from(column)))
        .unwrap_or((Json::Null, Json::Null));
    json!({
        "file": item.path.display().to_string(),
        "module": item.module_id.to_string(),
        "line": line,
        "column": column,
        "severity": severity_name(diagnostic),
        "code": diagnostic.code,
        "message": diagnostic.message,
        "hints": diagnostic.hints,
    })
}

/// Serialize diagnostics.
pub fn diagnostics_json(diagnostics: &[FileDiagnostic]) -> Json {
    Json::Array(diagnostics.iter().map(diagnostic_json).collect())
}

/// Print a JSON value to stdout.
pub fn print_json(value: &Json) {
    println!(
        "{}",
        serde_json::to_string_pretty(value).unwrap_or_default()
    );
}

/// Count errors.
pub fn error_count(diagnostics: &[FileDiagnostic]) -> usize {
    diagnostics
        .iter()
        .filter(|item| item.diagnostic.is_error())
        .count()
}
