//! Collection of inert document resources (formulas, code fences) and their
//! owning units.
//!
//! A resource is addressable through its enclosing heading range; it is never
//! executed by reading or rendering the document. Multiple resources in one
//! unit are all kept (blueprint S-07: never silently pick the first one).

use sha2::{Digest, Sha256};

use marqdo_ir::{ModuleId, Resource, UnitId};
use marqdo_markdown::{MarkdownDocument, MdKind, MdNode};
use marqdo_span::Span;

use crate::units::UnitIndex;

/// Collect formulas and code fences with their innermost owning unit.
pub fn collect(doc: &MarkdownDocument, index: &UnitIndex, _module: &ModuleId) -> Vec<Resource> {
    let mut resources = Vec::new();
    let ranges = unit_ranges(doc, index);
    visit(&doc.root, &ranges, &mut resources);
    resources
}

/// Range of each unit, used to attribute resources to the innermost unit.
fn unit_ranges(doc: &MarkdownDocument, index: &UnitIndex) -> Vec<(UnitId, Span)> {
    let mut ranges = Vec::with_capacity(index.units.len());
    let total = doc.source.len();
    for (position, unit) in index.units.iter().enumerate() {
        let start = unit.span.start;
        let end = if unit.level == 0 {
            total
        } else {
            // The unit's range ends at the next heading at the same or a
            // shallower level, or at the end of the document.
            index
                .units
                .iter()
                .skip(position + 1)
                .find(|candidate| candidate.level != 0 && candidate.level <= unit.level)
                .map(|candidate| candidate.span.start)
                .unwrap_or(total)
        };
        ranges.push((unit.id.clone(), Span::new(start, end.max(start))));
    }
    ranges
}

fn visit(node: &MdNode, ranges: &[(UnitId, Span)], out: &mut Vec<Resource>) {
    match &node.kind {
        MdKind::Math { display } => {
            if let Some(unit) = innermost_unit(node.span, ranges) {
                out.push(Resource::Formula {
                    unit,
                    latex: node.literal.clone().unwrap_or_default(),
                    display: *display,
                    span: node.span,
                });
            }
        }
        MdKind::MathBlock => {
            if let Some(unit) = innermost_unit(node.span, ranges) {
                out.push(Resource::Formula {
                    unit,
                    latex: node.literal.clone().unwrap_or_default(),
                    display: true,
                    span: node.span,
                });
            }
        }
        MdKind::CodeBlock { info, .. } => {
            if let Some(unit) = innermost_unit(node.span, ranges) {
                let source = node.literal.clone().unwrap_or_default();
                out.push(Resource::Code {
                    unit,
                    language: info.split_whitespace().next().unwrap_or("").to_string(),
                    content_hash: content_hash(&source),
                    source,
                    span: node.span,
                });
            }
        }
        _ => {}
    }
    for child in &node.children {
        visit(child, ranges, out);
    }
}

fn innermost_unit(span: Span, ranges: &[(UnitId, Span)]) -> Option<UnitId> {
    ranges
        .iter()
        .filter(|(_, range)| range.start <= span.start && span.end <= range.end)
        .max_by_key(|(unit, range)| (unit.path.len(), range.start))
        .map(|(unit, _)| unit.clone())
}

/// SHA-256 of code source, stored on `Code` values so that artifacts can be
/// checked for content drift.
pub fn content_hash(source: &str) -> String {
    let digest = Sha256::digest(source.as_bytes());
    let mut hex = String::with_capacity(64);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::build_units;
    use marqdo_markdown::parse;

    fn resources_of(source: &str) -> Vec<Resource> {
        let doc = parse(source);
        let module = ModuleId::from("index.mq.md");
        let index = build_units(&doc, &module);
        collect(&doc, &index, &module)
    }

    #[test]
    fn formulas_and_code_are_attributed_to_functions() {
        let resources = resources_of(
            "# ns\n\n## f\n\n$$A = pi r^2$$\n\n```python\nprint(1)\n```\n\n## g\n\n$$B = 1$$\n",
        );
        assert_eq!(resources.len(), 3);
        let Resource::Formula { unit, latex, .. } = &resources[0] else {
            panic!("expected formula");
        };
        assert_eq!(latex, "A = pi r^2");
        assert!(unit.to_string().ends_with("f"));
        let Resource::Code { language, .. } = &resources[1] else {
            panic!("expected code");
        };
        assert_eq!(language, "python");
        let Resource::Formula { unit, .. } = &resources[2] else {
            panic!("expected formula");
        };
        assert!(unit.to_string().ends_with("g"));
    }

    #[test]
    fn content_hash_is_stable() {
        assert_eq!(content_hash("print(1)\n").len(), 64);
        assert_ne!(content_hash("a"), content_hash("b"));
    }
}
