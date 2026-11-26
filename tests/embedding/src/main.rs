//! Independent Engine consumer for issue #13 / ADR-0071.
//!
//! This binary is a real Cargo package outside the CLI workspace. It executes
//! one Markdown program through the real Frontend, static call binding and
//! Runtime and reports the result through the neutral Host contract. It never
//! depends on the Native Host, the CLI, the sandbox or a plugin loader.
//!
//! Positive cases are compared against a real CLI invocation by
//! `tools/ci/check_engine_boundary.py`. Negative cases use a bounded test host
//! and assert the concrete error category; a no-op limit is not accepted.

use std::time::{Duration, Instant};

use marqdo_frontend::analyze_source;
use marqdo_host_contract::{ExecutionContext, Host, HostOp};
use marqdo_ir::{ModuleId, Program};
use marqdo_runtime::{run_with_limits, EvaluatedArg, MemoryLimits};
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError, Value};

const MODULE_ID: &str = "program.mq.md";

/// A small bounded host. It implements real budgets and refuses every external
/// effect; it is deliberately not a published product host or a sandbox.
struct TestHost {
    fuel: u64,
    stack_limit: usize,
    deadline: Option<Instant>,
    output: Vec<u8>,
}

impl TestHost {
    fn bounded() -> Self {
        Self {
            fuel: 5_000_000,
            stack_limit: 128,
            deadline: Some(Instant::now() + Duration::from_secs(30)),
            output: Vec::new(),
        }
    }

    fn with_fuel(mut self, fuel: u64) -> Self {
        self.fuel = fuel;
        self
    }

    fn with_stack_limit(mut self, limit: usize) -> Self {
        self.stack_limit = limit;
        self
    }

    fn with_expired_deadline(mut self) -> Self {
        self.deadline = Some(Instant::now() - Duration::from_secs(1));
        self
    }
}

impl Host for TestHost {
    fn invoke(
        &mut self,
        op: HostOp,
        _args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        Err(RuntimeError::new(
            ErrorCategory::PermissionDenied,
            ctx.span,
            format!(
                "embedding consumer grants no external effect for `{}`",
                op.name()
            ),
        ))
    }

    fn resource_open(
        &mut self,
        kind: &str,
        _descriptor: &str,
        _rights: &[String],
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        Err(RuntimeError::new(
            ErrorCategory::PermissionDenied,
            ctx.span,
            format!("embedding consumer grants no `{kind}` resource"),
        ))
    }

    fn consume_fuel(&mut self, amount: u64, span: Option<Span>) -> Result<(), RuntimeError> {
        if amount > self.fuel {
            self.fuel = 0;
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                "embedding consumer evaluation fuel exhausted",
            ));
        }
        self.fuel -= amount;
        Ok(())
    }

    fn check_stack(&self, depth: usize, span: Option<Span>) -> Result<(), RuntimeError> {
        if depth > self.stack_limit {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                format!(
                    "embedding consumer stack limit ({}) exceeded",
                    self.stack_limit
                ),
            ));
        }
        Ok(())
    }

    fn check_deadline(&self, span: Option<Span>) -> Result<(), RuntimeError> {
        if let Some(deadline) = self.deadline {
            if Instant::now() >= deadline {
                return Err(RuntimeError::new(
                    ErrorCategory::ResourceExhausted,
                    span,
                    "embedding consumer wall-clock deadline exceeded",
                ));
            }
        }
        Ok(())
    }

    fn take_output(&mut self) -> Vec<u8> {
        std::mem::take(&mut self.output)
    }
}

/// A host with no resource implementation at all: it inherits the trait
/// defaults, which must fail explicitly with `ResourceUnsupported`.
struct InertHost {
    fuel: u64,
}

impl Host for InertHost {
    fn invoke(
        &mut self,
        op: HostOp,
        _args: Vec<Value>,
        ctx: &ExecutionContext,
    ) -> Result<Value, RuntimeError> {
        Err(RuntimeError::new(
            ErrorCategory::ResourceUnsupported,
            ctx.span,
            format!("inert host does not implement `{}`", op.name()),
        ))
    }

    fn consume_fuel(&mut self, amount: u64, span: Option<Span>) -> Result<(), RuntimeError> {
        if amount > self.fuel {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceExhausted,
                span,
                "inert host fuel exhausted",
            ));
        }
        self.fuel -= amount;
        Ok(())
    }

    fn check_stack(&self, _depth: usize, _span: Option<Span>) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn check_deadline(&self, _span: Option<Span>) -> Result<(), RuntimeError> {
        Ok(())
    }

    fn take_output(&mut self) -> Vec<u8> {
        Vec::new()
    }
}

fn build_program(source: &str) -> Program {
    let analysis = analyze_source(MODULE_ID, source);
    assert!(
        !analysis.has_errors(),
        "frontend rejected the embedding program: {:#?}",
        analysis.diagnostics
    );
    let module_id = ModuleId::from(MODULE_ID);
    let mut program = Program {
        main_module: Some(module_id),
        ..Program::default()
    };
    program.insert(analysis.module);
    let registry = program.callable_registry();
    let bound = marqdo_frontend::calls::bind_program_calls(&program, &registry);
    assert!(
        bound.diagnostics.is_empty(),
        "static call binding rejected the embedding program: {:#?}",
        bound.diagnostics
    );
    program.set_call_bindings(bound.bindings);
    program
}

fn named_args(args: &[(&str, Value)]) -> Vec<EvaluatedArg> {
    args.iter()
        .map(|(name, value)| EvaluatedArg::named(*name, value.clone(), Span::default()))
        .collect()
}

fn run_case(
    program: &Program,
    host: &mut dyn Host,
    function: &str,
    args: &[(&str, Value)],
    limits: MemoryLimits,
) -> Result<Value, RuntimeError> {
    run_with_limits(
        program,
        host,
        &ModuleId::from(MODULE_ID),
        function,
        named_args(args),
        limits,
    )
    .map(|outcome| outcome.value)
}

fn report_value(name: &str, value: &Value) -> Result<(), String> {
    let wire = marqdo_value::to_wire(value);
    let encoded = serde_json::to_string(&wire).map_err(|error| error.to_string())?;
    println!("result {name} {encoded}");
    Ok(())
}

fn expect_error(name: &str, result: Result<Value, RuntimeError>, expected: ErrorCategory) {
    match result {
        Ok(value) => fail(&format!(
            "{name}: expected error {:?}, ran successfully with {value:?}",
            expected
        )),
        Err(error) => {
            if error.category != expected {
                fail(&format!(
                    "{name}: expected error {:?}, got {:?} ({})",
                    expected, error.category, error.message
                ));
            }
            println!("error {name} {}", error.code());
        }
    }
}

fn fail(message: &str) -> ! {
    eprintln!("FAIL: {message}");
    std::process::exit(1);
}

fn main() {
    if let Err(message) = execute() {
        fail(&message);
    }
    println!("PASS embedding-consumer");
}

fn execute() -> Result<(), String> {
    let source = include_str!("../program.mq.md");
    let program = build_program(source);

    // Positive: Int loop result.
    let mut host = TestHost::bounded();
    let sum = run_case(
        &program,
        &mut host,
        "sum_to",
        &[("n", Value::int(10))],
        MemoryLimits::default(),
    )
    .map_err(|error| format!("sum_to failed: {}", error.message))?;
    if sum != Value::int(55) {
        return Err(format!("sum_to(10) = {sum:?}, expected Int(55)"));
    }
    report_value("sum_to", &sum)?;

    // Positive: structural values use the single Value Wire encoder.
    let mut host = TestHost::bounded();
    let record = run_case(
        &program,
        &mut host,
        "types",
        &[("seed", Value::int(7))],
        MemoryLimits::default(),
    )
    .map_err(|error| format!("types failed: {}", error.message))?;
    report_value("types", &record)?;

    // Negative: real fuel exhaustion.
    let mut host = TestHost::bounded().with_fuel(4);
    expect_error(
        "fuel",
        run_case(
            &program,
            &mut host,
            "sum_to",
            &[("n", Value::int(10))],
            MemoryLimits::default(),
        ),
        ErrorCategory::ResourceExhausted,
    );

    // Negative: real stack-depth limit.
    let mut host = TestHost::bounded().with_stack_limit(2);
    expect_error(
        "stack",
        run_case(
            &program,
            &mut host,
            "recurse",
            &[("n", Value::int(10))],
            MemoryLimits::default(),
        ),
        ErrorCategory::ResourceExhausted,
    );

    // Negative: real wall-clock deadline.
    let mut host = TestHost::bounded().with_expired_deadline();
    expect_error(
        "deadline",
        run_case(
            &program,
            &mut host,
            "sum_to",
            &[("n", Value::int(10))],
            MemoryLimits::default(),
        ),
        ErrorCategory::ResourceExhausted,
    );

    // Negative: addressable-memory bounds are enforced by the runtime.
    let mut host = TestHost::bounded();
    match run_case(&program, &mut host, "oob", &[], MemoryLimits::default()) {
        Ok(value) => return Err(format!("oob: expected a memory error, got {value:?}")),
        Err(error) => {
            if !matches!(
                error.category,
                ErrorCategory::IndexOutOfBounds | ErrorCategory::MemoryFault
            ) {
                return Err(format!(
                    "oob: expected a memory bound error, got {:?} ({})",
                    error.category, error.message
                ));
            }
            println!("error oob {}", error.code());
        }
    }

    // Negative: a host that grants no external effect refuses explicitly.
    let mut host = TestHost::bounded();
    expect_error(
        "file_denied",
        run_case(
            &program,
            &mut host,
            "read_file",
            &[],
            MemoryLimits::default(),
        ),
        ErrorCategory::PermissionDenied,
    );

    // Negative: a host with no resource implementation reports Unsupported,
    // never a prose fallback or a silent success.
    let mut host = InertHost { fuel: 1_000_000 };
    expect_error(
        "file_unsupported",
        run_case(
            &program,
            &mut host,
            "read_file",
            &[],
            MemoryLimits::default(),
        ),
        ErrorCategory::ResourceUnsupported,
    );

    // Sanity: budget fields are not a no-op and remain run-scoped.
    let mut host = TestHost::bounded();
    let fuel_before = host.fuel;
    let _ = run_case(
        &program,
        &mut host,
        "sum_to",
        &[("n", Value::int(3))],
        MemoryLimits::default(),
    )
    .map_err(|error| format!("budget sanity run failed: {}", error.message))?;
    if host.fuel >= fuel_before {
        return Err("fuel budget did not decrease during execution".to_string());
    }

    Ok(())
}
