//! End-to-end interpreter tests: Markdown source → frontend IR → execution.

use marqdo_host::{Capabilities, StdHost};
use marqdo_host_contract::Host;
use marqdo_ir::{ModuleId, Program};
use marqdo_runtime::{run, run_with_limits, EvaluatedArg, MemoryLimits, RuntimeError, Value};
use marqdo_sandbox::SandboxMode;
use marqdo_span::Span;

fn build(source: &str) -> Program {
    let result = marqdo_frontend::analyze_source("index.mq.md", source);
    assert!(
        !result.has_errors(),
        "unexpected compile diagnostics: {:#?}",
        result.diagnostics
    );
    let module_id = ModuleId::from("index.mq.md");
    let mut program = Program {
        main_module: Some(module_id.clone()),
        ..Program::default()
    };
    program.insert(result.module);
    let registry = program.callable_registry();
    let bound = marqdo_frontend::calls::bind_program_calls(&program, &registry);
    assert!(
        bound.diagnostics.is_empty(),
        "unexpected binding diagnostics: {:#?}",
        bound.diagnostics
    );
    program.set_call_bindings(bound.bindings);
    program
}

fn host() -> StdHost {
    StdHost::new(
        SandboxMode::Project,
        Capabilities::pure(),
        std::env::temp_dir(),
    )
}

fn run_named(source: &str, function: &str, args: &[(&str, Value)]) -> Result<Value, RuntimeError> {
    let program = build(source);
    let mut host = host();
    let args = args
        .iter()
        .map(|(name, value)| EvaluatedArg::named(*name, value.clone(), Span::default()))
        .collect();
    run(
        &program,
        &mut host,
        &ModuleId::from("index.mq.md"),
        function,
        args,
    )
    .map(|outcome| outcome.value)
}

fn run_and_output(source: &str, function: &str) -> (Result<Value, RuntimeError>, String) {
    let program = build(source);
    let mut host = host();
    let result = run(
        &program,
        &mut host,
        &ModuleId::from("index.mq.md"),
        function,
        Vec::new(),
    )
    .map(|outcome| outcome.value);
    let output = String::from_utf8(host.take_output()).unwrap();
    (result, output)
}

#[test]
fn recursion_branch_and_return() {
    let source = "## @exec 阶乘\n\n`if n <= 1`\n\n`return 1`\n\n`else`\n\n`if n > 1`\n\n`结果 = n * 阶乘(n=n-1)`\n\n`return 结果`\n\n`end`\n\n`end`\n";
    let value = run_named(source, "阶乘", &[("n", Value::int(5))]).unwrap();
    assert_eq!(value, Value::int(120));
}

#[test]
fn loop_mutates_and_returns_unit_for_fallthrough() {
    let source = "## @exec 倒计时\n\n`n = 3`\n\n`while n > 0`\n\n`n = n - 1`\n\n`end`\n";
    let (result, _) = run_and_output(source, "倒计时");
    assert_eq!(result.unwrap(), Value::Unit);
}

#[test]
fn return_inside_loop_propagates() {
    let source = "## @exec 找\n\n`i = 0`\n\n`while true`\n\n`i = i + 1`\n\n`if i >= 3`\n\n`return i`\n\n`end`\n\n`end`\n";
    let (result, _) = run_and_output(source, "找");
    assert_eq!(result.unwrap(), Value::int(3));
}

#[test]
fn explicit_signature_supports_positional_arguments() {
    let source = "## @exec 加\n\n`params(a: Int, b: Int = 10)`\n\n`return a + b`\n";
    let program = build(source);
    let mut host = host();
    let result = run(
        &program,
        &mut host,
        &ModuleId::from("index.mq.md"),
        "加",
        vec![EvaluatedArg::positional(Value::int(2), Span::default())],
    )
    .unwrap();
    assert_eq!(result.value, Value::int(12));
}

#[test]
fn builtins_for_conversions_and_length() {
    let source = "## @exec f\n\n`return 长度(\"你好\") + 整数(\"40\") + 整数(2.0)`\n";
    let value = run_named(source, "f", &[]).unwrap();
    assert_eq!(value, Value::int(44));
}

#[test]
fn function_reference_can_be_called() {
    let source =
        "## @exec 平方\n\n`params(n: Int)`\n\n`return n * n`\n\n## @exec 主\n\n`f = ref(\"#平方\")`\n\n`return f(n=4)`\n";
    let value = run_named(source, "主", &[]).unwrap();
    assert_eq!(value, Value::int(16));
}

#[test]
fn non_bool_condition_is_a_runtime_error() {
    let source = "## @exec f\n\n`x = 1`\n\n`while x`\n\n`x = x + 1`\n\n`end`\n\n`return x`\n";
    let error = run_named(source, "f", &[]).unwrap_err();
    assert_eq!(error.code(), "runtime.type");
}

#[test]
fn fuel_budget_is_enforced() {
    let program = build("## @exec f\n\n`while true`\n\n`x = 1`\n\n`end`\n\n`return 0`\n");
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::pure(),
        std::env::temp_dir(),
    );
    host.budget.fuel = 200;
    let error = run(
        &program,
        &mut host,
        &ModuleId::from("index.mq.md"),
        "f",
        Vec::new(),
    )
    .unwrap_err();
    assert_eq!(error.code(), "resource.exhausted");
}

#[test]
fn recursion_depth_is_bounded() {
    let source = "## @exec f\n\n`return f(n=n + 1)`\n";
    let program = build(source);
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::pure(),
        std::env::temp_dir(),
    );
    host.budget.stack_limit = 8;
    let error = run(
        &program,
        &mut host,
        &ModuleId::from("index.mq.md"),
        "f",
        vec![EvaluatedArg::named("n", Value::int(0), Span::default())],
    )
    .unwrap_err();
    assert_eq!(error.code(), "resource.exhausted");
    assert!(error.frames.len() >= 2);
}

#[test]
fn ambiguous_ref_resource_is_an_error() {
    let source =
        "## @exec f\n\n### 公式\n\n$$a = 1$$\n\n$$b = 2$$\n\n`x = ref(\"#公式\")`\n\n`return x`\n";
    let error = run_named(source, "f", &[]).unwrap_err();
    assert_eq!(error.code(), "ref.ambiguous");
}

#[test]
fn print_without_the_bundled_stdlib_is_a_structured_error() {
    let source = "## @exec f\n\n`打印(1)`\n\n`return None`\n";
    let error = run_named(source, "f", &[]).unwrap_err();
    assert_eq!(error.code(), "kernel.unavailable");
    assert!(
        error.message.contains("bundled stdlib"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn function_ref_and_resource_members_are_readable() {
    let source = "## @exec 目标\n\n`return 1`\n\n## @exec main\n\n### 公式\n\n$$a = 1$$\n\n### 代码\n\n```text\nx\n```\n\n`f = ref(\"#目标\")`\n\n`g = ref(\"#公式\")`\n\n`c = ref(\"#代码\")`\n\n`return [f.module, f.name, g.latex, g.display, c.language, c.source]`\n";
    let value = run_named(source, "main", &[]).unwrap();
    let Value::List(items) = value else {
        panic!("expected list");
    };
    assert!(matches!(&items[0], Value::Text(module) if module.ends_with("index.mq.md")));
    assert_eq!(items[1], Value::text("目标"));
    assert_eq!(items[2], Value::text("a = 1"));
    assert_eq!(items[3], Value::Bool(true));
    assert_eq!(items[4], Value::text("text"));
    assert_eq!(items[5], Value::text("x\n"));
}

#[test]
fn memory_region_is_shared_by_handle_identity() {
    let source = "## @exec f\n\n`m = memory.new(1, 2)`\n\n`n = m`\n\n`m[3] = 7`\n\n`return [n[3], m == n, memory.size(n)]`\n";
    let value = run_named(source, "f", &[]).unwrap();
    assert_eq!(
        value,
        Value::List(vec![Value::int(7), Value::Bool(true), Value::int(1)])
    );
}

#[test]
fn memory_handle_cannot_reach_the_host_boundary() {
    let source = "## @exec f\n\n`m = memory.new(1, 1)`\n\n`fs.read(m)`\n\n`return 0`\n";
    let error = run_named(source, "f", &[]).unwrap_err();
    assert_eq!(error.code(), "runtime.type");
    assert!(
        error
            .message
            .contains("Memory values cannot cross the host boundary"),
        "unexpected message: {}",
        error.message
    );
}

#[test]
fn memory_budget_is_enforced_without_changing_the_region() {
    let program =
        build("## @exec f\n\n`m = memory.new(1, 1)`\n\n`return [m[0], memory.grow(m, 1)]`\n");
    let mut host = host();
    let error = run_with_limits(
        &program,
        &mut host,
        &ModuleId::from("index.mq.md"),
        "f",
        Vec::new(),
        MemoryLimits {
            max_total_bytes: 64 * 1024,
            max_region_bytes: 64 * 1024 * 2,
        },
    )
    .unwrap_err();
    assert_eq!(error.code(), "resource.exhausted");
}

#[test]
fn continue_obeys_fuel_and_wall_clock_budgets() {
    use std::time::{Duration, Instant};

    let source = "## @exec main\n\n`while true`\n\n`continue`\n\n`end`\n";
    let program = build(source);
    let module = ModuleId::from("index.mq.md");

    let mut fuel_host = host();
    fuel_host.budget.fuel = 20;
    let fuel = run(&program, &mut fuel_host, &module, "main", Vec::new()).unwrap_err();
    assert_eq!(fuel.code(), "resource.exhausted");
    assert!(fuel.message.contains("fuel exhausted"));

    let mut deadline_host = host();
    deadline_host.budget.fuel = u64::MAX;
    deadline_host.budget.deadline = Some(Instant::now() + Duration::from_millis(25));
    let timeout = run(&program, &mut deadline_host, &module, "main", Vec::new()).unwrap_err();
    assert_eq!(timeout.code(), "resource.exhausted");
    assert!(timeout.message.contains("wall-clock time limit"));
}

#[test]
fn malformed_ir_cannot_transfer_into_the_callers_loop() {
    let source =
        "## @exec child\n\n`return 1`\n\n## @exec main\n\n`while true`\n\n`child()`\n\n`end`\n";
    let mut program = build(source);
    let module = ModuleId::from("index.mq.md");
    let child = program
        .modules
        .get_mut(&module)
        .unwrap()
        .functions
        .iter_mut()
        .find(|function| function.id.name == "child")
        .unwrap();
    let span = child.body[0].span();
    child.body[0] = marqdo_ir::Stmt::Break { span };
    let error = run(&program, &mut host(), &module, "main", Vec::new()).unwrap_err();
    assert_eq!(error.category, marqdo_value::ErrorCategory::Internal);
    assert_eq!(error.span, Some(span));
}
