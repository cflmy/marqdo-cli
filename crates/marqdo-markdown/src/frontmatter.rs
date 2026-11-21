//! YAML front matter handling.
//!
//! Front matter is only recognised at byte offset 0 (after an optional UTF-8
//! BOM) and only when the Markdown parser itself reports a `FrontMatter` node.
//! The parser node keeps the raw delimiters; this module extracts the YAML body
//! and exposes the explicit `import` mapping without interpreting anything
//! else. Front matter never executes.

use std::collections::BTreeMap;

use marqdo_span::Span;

/// Parsed (or unparseable) front matter.
#[derive(Debug, Clone, PartialEq)]
pub struct FrontMatter {
    /// Raw YAML body between the delimiter lines, excluding the delimiters.
    pub raw: String,
    /// Parsed YAML value; `Err` carries a human readable reason.
    pub yaml: Result<serde_yaml::Value, String>,
    /// Span of the whole front matter node (including delimiters).
    pub span: Span,
    /// Span of the YAML body only.
    pub yaml_span: Span,
}

impl FrontMatter {
    /// The explicit `import` mapping from front matter.
    ///
    /// The supported shape is a YAML mapping of local alias to module address:
    ///
    /// ```yaml
    /// import:
    ///   geometry: "@geometry"
    /// ```
    ///
    /// Anything else is an error; imports are never inferred.
    pub fn imports(&self) -> Result<BTreeMap<String, String>, String> {
        let yaml = self
            .yaml
            .as_ref()
            .map_err(|error| format!("front matter is not valid YAML: {error}"))?;
        let Some(imports) = yaml.get("import") else {
            return Ok(BTreeMap::new());
        };
        let Some(mapping) = imports.as_mapping() else {
            return Err("`import` must be a mapping of alias to module address".to_string());
        };
        let mut result = BTreeMap::new();
        for (alias, target) in mapping {
            let Some(alias) = alias.as_str() else {
                return Err("`import` aliases must be strings".to_string());
            };
            let Some(target) = target.as_str() else {
                return Err(format!("import target for `{alias}` must be a string"));
            };
            if alias.is_empty() {
                return Err("`import` alias must not be empty".to_string());
            }
            if result
                .insert(alias.to_string(), target.to_string())
                .is_some()
            {
                return Err(format!("duplicate import alias `{alias}`"));
            }
        }
        Ok(result)
    }

    /// Whether the top-level YAML mapping contains `key`.
    ///
    /// Returns `false` for invalid YAML. Used to reject removed syntax
    /// selectors such as `marqdo_semantics` or `syntax` without making the
    /// frontend depend on a YAML crate.
    pub fn has_key(&self, key: &str) -> bool {
        let Ok(yaml) = &self.yaml else {
            return false;
        };
        yaml.as_mapping()
            .is_some_and(|mapping| mapping.contains_key(key))
    }

    /// The raw YAML parses successfully.
    pub fn is_valid(&self) -> bool {
        self.yaml.is_ok()
    }
}

/// Split a `FrontMatter` parser literal into the YAML body.
///
/// The parser literal starts with the opening delimiter line and includes the
/// closing delimiter line plus trailing newlines, e.g.
/// `"---\nimport:\n  a: b\n---\n\n"`.
pub fn parse_literal(literal: &str, span: Span) -> FrontMatter {
    let (raw, body_offset) = split_delimiters(literal);
    let yaml = if raw.trim().is_empty() {
        Ok(serde_yaml::Value::Null)
    } else {
        serde_yaml::from_str::<serde_yaml::Value>(&raw).map_err(|error| error.to_string())
    };
    let start = span.start.saturating_add(body_offset);
    let end = start.saturating_add(raw.len()).min(span.end.max(start));
    FrontMatter {
        raw,
        yaml,
        span,
        yaml_span: Span::new(start, end),
    }
}

/// Return the YAML body and its byte offset within `literal`.
fn split_delimiters(literal: &str) -> (String, usize) {
    let mut offset = 0usize;
    let mut lines = literal.split_inclusive('\n');
    let Some(first) = lines.next() else {
        return (String::new(), 0);
    };
    offset += first.len();
    let mut current = offset;
    let body_start = offset;
    for line in lines {
        let trimmed = line.trim_end_matches(['\r', '\n']);
        if trimmed == "---" || trimmed == "..." {
            let body = literal[body_start..current].to_string();
            return (body, body_start);
        }
        current += line.len();
    }
    // Unterminated front matter cannot normally reach here: comrak only emits a
    // FrontMatter node when a closing delimiter exists. Be forgiving and treat
    // the rest as YAML so diagnostics can still point at it.
    (literal[body_start..].to_string(), body_start)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_import_mapping() {
        let literal = "---\nimport:\n  geometry: \"@geometry\"\n---\n\n";
        let fm = parse_literal(literal, Span::new(0, literal.len()));
        assert!(fm.is_valid());
        let imports = fm.imports().unwrap();
        assert_eq!(
            imports.get("geometry").map(String::as_str),
            Some("@geometry")
        );
        assert_eq!(fm.raw, "import:\n  geometry: \"@geometry\"\n");
        assert_eq!(fm.yaml_span.start, 4);
        assert_eq!(&literal[fm.yaml_span.start..fm.yaml_span.end], fm.raw);
    }

    #[test]
    fn empty_frontmatter_is_null() {
        let literal = "---\n---\n";
        let fm = parse_literal(literal, Span::new(0, literal.len()));
        assert_eq!(fm.raw, "");
        assert!(fm.imports().unwrap().is_empty());
    }

    #[test]
    fn invalid_yaml_is_reported_not_panicked() {
        let literal = "---\nimport: [unclosed\n---\n";
        let fm = parse_literal(literal, Span::new(0, literal.len()));
        assert!(!fm.is_valid());
        assert!(fm.imports().is_err());
    }

    #[test]
    fn import_shapes_are_validated() {
        let literal = "---\nimport: geometry\n---\n";
        let fm = parse_literal(literal, Span::new(0, literal.len()));
        assert!(fm.imports().is_err());
        assert!(fm.imports().unwrap_err().contains("mapping"));
    }
}
