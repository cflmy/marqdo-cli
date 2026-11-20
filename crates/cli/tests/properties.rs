//! Prose invariance property tests (blueprint section 14.2).
//!
//! For every base document the executable projection `ExecutableProjection(D)`
//! (the frontend IR) must be unchanged by pure narrative edits:
//!
//! * inserting a narration paragraph between top-level blocks/after headings,
//! * prepending/appending narration at the document boundaries,
//! * removing narration paragraphs that were already present.
//!
//! The comparison strips source spans (`span`, `heading_span`, `name_span`,
//! `condition_span`) because narration edits shift byte offsets. The test also
//! asserts that every variant still compiles without errors. The precondition
//! from the blueprint is respected: the transformations only touch `Text`
//! nodes and never add headings, lists, emphasis, links or code, so the
//! Markdown block structure — and therefore the executable structure — cannot
//! change.

use std::collections::BTreeMap;

/// A base program plus the narration fragments it contains.
struct Base {
    name: &'static str,
    source: &'static str,
    /// Narration fragments safe to delete: removing them leaves the block
    /// structure intact and only affects text payloads.
    narration: &'static [&'static str],
}

const BASES: &[Base] = &[
    Base {
        name: "factorial",
        source: r##"# 数学

## 阶乘

计算阶乘的叙述一。

1. `n <= 1`

   递归终点叙述。

   *1*

2. `n > 1`

   递归步骤叙述。

   **结果 = n * 阶乘(n=n-1)**

   *结果*
"##,
        narration: &["计算阶乘的叙述一。", "递归终点叙述。", "递归步骤叙述。"],
    },
    Base {
        name: "explicit-signature",
        source: r##"## 圆面积

`(r: Number)`

半径叙述。

*math.eval(ref("#公式"), {"r": r}, target="A")*

### 公式

$$
A = \pi r^2
$$
"##,
        narration: &["半径叙述。"],
    },
    Base {
        name: "countdown-loop",
        source: r##"## 倒计时

循环叙述。

**n = 3**

- `n > 0`

  循环体叙述。

  > 打印(n)

  **n = n - 1**

*n*
"##,
        narration: &["循环叙述。", "循环体叙述。"],
    },
    Base {
        name: "branch-classification",
        source: r##"## 分类

背景叙述。

**结果 = 0**

1. `n < 0`

   负数分支叙述。

   **结果 = -1**

2. `n >= 0`

   非负分支叙述。

   **结果 = 1**

*结果*
"##,
        narration: &["背景叙述。", "负数分支叙述。", "非负分支叙述。"],
    },
    Base {
        name: "call-and-print",
        source: r##"## 问候

说明为什么这样写。

> 打印("你好")

**问候 = "你好，Marqdo"**

再补充一句叙述。

*问候*
"##,
        narration: &["说明为什么这样写。", "再补充一句叙述。"],
    },
    Base {
        name: "inline-prose-in-executable-paragraph",
        source: r##"## 加一

这是行内叙述 **结果 = 1 + 1** 后面继续叙述。

*结果*
"##,
        narration: &["这是行内叙述 ", " 后面继续叙述。"],
    },
    Base {
        name: "nested-control-flow",
        source: r##"## 嵌套

说明嵌套结构。

**total = 0**

1. `n > 0`

   **m = 3**

   - `m > 0`

     **total = total + m**

     **m = m - 1**

2. `n <= 0`

   **total = 0**

*total*
"##,
        narration: &["说明嵌套结构。"],
    },
    Base {
        name: "two-namespaces",
        source: r##"# 数学

命名空间叙述。

## 平方

**结果 = n * n**

*结果*

# 文本

## 长度

*长度(s)*
"##,
        narration: &["命名空间叙述。"],
    },
];

const INSERTED_PROSE: &str = "这是一段与执行结构无关的纯叙述文字，它只改变 Markdown 文本节点。";

/// Strip source spans so narrative edits (which shift byte offsets) compare
/// equal. This mirrors the invariance definition in blueprint section 14.2.
fn without_spans(value: &serde_json::Value) -> serde_json::Value {
    match value {
        serde_json::Value::Object(map) => {
            let mut out = serde_json::Map::new();
            for (key, value) in map {
                if matches!(
                    key.as_str(),
                    "span" | "heading_span" | "name_span" | "condition_span" | "key_span"
                ) {
                    continue;
                }
                out.insert(key.clone(), without_spans(value));
            }
            serde_json::Value::Object(out)
        }
        serde_json::Value::Array(items) => {
            serde_json::Value::Array(items.iter().map(without_spans).collect())
        }
        other => other.clone(),
    }
}

/// Executable projection of one source: the function IR, span-free.
fn executable_projection(source: &str) -> serde_json::Value {
    let analysis = marqdo_frontend::analyze_source("index.mq.md", source);
    if analysis.has_errors() {
        let diagnostics: Vec<String> = analysis
            .diagnostics
            .iter()
            .filter(|diagnostic| diagnostic.is_error())
            .map(|diagnostic| format!("{}: {}", diagnostic.code, diagnostic.message))
            .collect();
        panic!("prose-invariance variant must compile without errors: {diagnostics:?}");
    }
    let json = serde_json::to_value(&analysis.module.functions)
        .expect("function IR is always serializable");
    without_spans(&json)
}

/// Whether a line is an ATX heading.
fn is_heading(line: &str) -> bool {
    let mut chars = line.chars();
    if chars.next() != Some('#') {
        return false;
    }
    let mut hashes = 1;
    for ch in chars {
        if ch == '#' {
            hashes += 1;
            if hashes > 6 {
                return false;
            }
        } else {
            return ch == ' ';
        }
    }
    false
}

/// Insert a narration paragraph after every heading. When the heading is
/// followed by an explicit signature paragraph, the narration goes *after* the
/// signature so the signature is still the first paragraph of the function.
fn insert_prose_after_headings(source: &str) -> String {
    let lines: Vec<&str> = source.lines().collect();
    let mut out: Vec<String> = Vec::new();
    let mut index = 0usize;
    while index < lines.len() {
        let line = lines[index];
        out.push(line.to_string());
        if is_heading(line) {
            let mut next = index + 1;
            while next < lines.len() && lines[next].trim().is_empty() {
                next += 1;
            }
            let signature = next < lines.len() && lines[next].trim_start().starts_with("`(");
            if signature {
                while index + 1 < lines.len() && lines[index + 1].trim().is_empty() {
                    index += 1;
                    out.push(lines[index].to_string());
                }
                index += 1;
                out.push(lines[index].to_string());
                out.push(String::new());
                out.push(INSERTED_PROSE.to_string());
            } else {
                out.push(String::new());
                out.push(INSERTED_PROSE.to_string());
            }
        }
        index += 1;
    }
    let mut text = out.join("\n");
    text.push('\n');
    text
}

fn variants(base: &Base) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    map.insert(
        "prepend-narration".to_string(),
        format!("{INSERTED_PROSE}\n\n{}", base.source),
    );
    map.insert(
        "append-narration".to_string(),
        format!("{}\n\n{INSERTED_PROSE}\n", base.source),
    );
    map.insert(
        "insert-after-headings".to_string(),
        insert_prose_after_headings(base.source),
    );
    // Remove each known narration fragment in turn.
    for (position, fragment) in base.narration.iter().enumerate() {
        map.insert(
            format!("remove-narration-{position}"),
            base.source.replace(fragment, ""),
        );
    }
    map
}

#[test]
fn pure_prose_edits_do_not_change_executable_ir() {
    // All fixtures are in-code strings. The test deliberately does not depend
    // on an on-disk `tests/properties/` directory: Git cannot track empty
    // directories, so such an assertion failed only on a fresh clone (B-01).
    let mut checked = 0usize;
    for base in BASES {
        let expected = executable_projection(base.source);
        let generated = variants(base);
        assert!(
            generated.len() >= 4,
            "base `{}` produced too few variants",
            base.name
        );
        for (variant, source) in generated {
            let actual = executable_projection(&source);
            assert_eq!(
                actual, expected,
                "prose edit `{variant}` changed the executable IR of base `{}`",
                base.name
            );
            checked += 1;
        }
    }
    println!(
        "prose invariance: checked {checked} variant(s) across {} base program(s)",
        BASES.len()
    );
}

/// Removing narration must also keep the program runnable: execute the
/// factorial base (and its narration-free form) with the same arguments and
/// compare values. This guards against prose accidentally becoming a
/// parameter or a statement.
#[test]
fn narration_removal_preserves_execution() {
    use marqdo_host::{Capabilities, StdHost};
    use marqdo_ir::ModuleId;
    use marqdo_runtime::{run, EvaluatedArg};
    use marqdo_sandbox::SandboxMode;

    let base = &BASES[0];
    let clean = base
        .narration
        .iter()
        .fold(base.source.to_string(), |text, fragment| {
            text.replace(fragment, "")
        });
    let project_root = std::env::temp_dir().join(format!(
        "marqdo-conformance-properties-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&project_root).expect("scratch project root");
    let module_id = ModuleId::from("index.mq.md");

    let mut values = Vec::new();
    for source in [base.source, clean.as_str()] {
        let analysis = marqdo_frontend::analyze_source(&module_id.to_string(), source);
        assert!(!analysis.has_errors(), "{:?}", analysis.diagnostics);
        let mut program = marqdo_ir::Program::default();
        program.insert(analysis.module.clone());
        program.main_module = Some(module_id.clone());
        let mut host = StdHost::new(
            SandboxMode::Project,
            Capabilities::pure(),
            project_root.clone(),
        );
        let outcome = run(
            &program,
            &mut host,
            &module_id,
            "阶乘",
            vec![EvaluatedArg::named(
                "n",
                marqdo_value::Value::int(5),
                marqdo_span::Span::default(),
            )],
        )
        .expect("factorial runs");
        values.push(outcome.value);
    }
    let _ = std::fs::remove_dir_all(&project_root);
    assert_eq!(
        values[0], values[1],
        "removing narration changed execution result"
    );
    assert_eq!(values[0], marqdo_value::Value::int(120));
}
