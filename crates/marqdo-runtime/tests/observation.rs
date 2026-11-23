//! Runtime observation integration tests (RFC-OBS-0001 / ADR-0059).
//!
//! The real frontend/IR/runtime path is exercised with an in-memory sink:
//! frame identity, branch/loop/return attribution, committed assignment
//! events, exact parameter selection and redaction of unselected values.

use std::cell::RefCell;
use std::rc::Rc;

use marqdo_host::{
    Capabilities, ObservationConfig, ObservationError, ObservationEvent, ObservationSink,
    ParameterSource, StdHost, ValueState,
};
use marqdo_ir::{ModuleId, Program};
use marqdo_runtime::run;
use marqdo_sandbox::SandboxMode;
use marqdo_value::Value;

#[derive(Debug, Clone)]
struct Seen {
    kind: &'static str,
    frame: Option<u64>,
    name: String,
    value_type: String,
    state: Option<ValueState>,
    wire: Option<serde_json::Value>,
    source: Option<ParameterSource>,
    decision: String,
}

struct Recorder(Rc<RefCell<Vec<Seen>>>);

impl ObservationSink for Recorder {
    fn emit(&mut self, event: ObservationEvent<'_>) -> Result<(), ObservationError> {
        let seen = match event {
            ObservationEvent::CallEnter { frame_id, .. } => Seen {
                kind: "enter",
                frame: Some(frame_id.0),
                name: String::new(),
                value_type: String::new(),
                state: None,
                wire: None,
                source: None,
                decision: String::new(),
            },
            ObservationEvent::CallExit {
                frame_id, status, ..
            } => Seen {
                kind: "exit",
                frame: Some(frame_id.0),
                name: status.to_string(),
                value_type: String::new(),
                state: None,
                wire: None,
                source: None,
                decision: String::new(),
            },
            ObservationEvent::Return {
                frame_id,
                value_type,
                ..
            } => Seen {
                kind: "return",
                frame: Some(frame_id.0),
                name: String::new(),
                value_type: value_type.to_string(),
                state: None,
                wire: None,
                source: None,
                decision: String::new(),
            },
            ObservationEvent::BranchEval {
                frame_id, result, ..
            } => Seen {
                kind: "branch",
                frame: Some(frame_id.0),
                name: result.to_string(),
                value_type: String::new(),
                state: None,
                wire: None,
                source: None,
                decision: String::new(),
            },
            ObservationEvent::LoopIteration {
                frame_id, decision, ..
            } => Seen {
                kind: "loop",
                frame: Some(frame_id.0),
                name: String::new(),
                value_type: String::new(),
                state: None,
                wire: None,
                source: None,
                decision: decision.to_string(),
            },
            ObservationEvent::LoopTransfer {
                frame_id, transfer, ..
            } => Seen {
                kind: transfer.event_kind(),
                frame: Some(frame_id.0),
                name: String::new(),
                value_type: String::new(),
                state: None,
                wire: None,
                source: None,
                decision: String::new(),
            },
            ObservationEvent::VariableAssigned {
                frame_id,
                name,
                value_type,
                value_state,
                value,
                ..
            } => Seen {
                kind: "variable",
                frame: Some(frame_id.0),
                name: name.to_string(),
                value_type: value_type.to_string(),
                state: Some(value_state),
                wire: value.map(|value| value.wire),
                source: None,
                decision: String::new(),
            },
            ObservationEvent::ParameterObserved {
                frame_id,
                name,
                source,
                value_type,
                value_state,
                value,
                ..
            } => Seen {
                kind: "parameter",
                frame: Some(frame_id.0),
                name: name.to_string(),
                value_type: value_type.to_string(),
                state: Some(value_state),
                wire: value.map(|value| value.wire),
                source: Some(source),
                decision: String::new(),
            },
            ObservationEvent::Effect { operation, .. } => Seen {
                kind: "effect",
                frame: None,
                name: operation.to_string(),
                value_type: String::new(),
                state: None,
                wire: None,
                source: None,
                decision: String::new(),
            },
            ObservationEvent::ObservationTruncated { reason, .. } => Seen {
                kind: "truncated",
                frame: None,
                name: reason.to_string(),
                value_type: String::new(),
                state: None,
                wire: None,
                source: None,
                decision: String::new(),
            },
            ObservationEvent::StdoutChunk { .. } | ObservationEvent::StderrChunk { .. } => Seen {
                kind: "chunk",
                frame: None,
                name: String::new(),
                value_type: String::new(),
                state: None,
                wire: None,
                source: None,
                decision: String::new(),
            },
        };
        self.0.borrow_mut().push(seen);
        Ok(())
    }
}

fn build(source: &str) -> Program {
    let result = marqdo_frontend::analyze_source("index.mq.md", source);
    assert!(!result.has_errors(), "{:#?}", result.diagnostics);
    let module_id = ModuleId::from("index.mq.md");
    let mut program = Program {
        main_module: Some(module_id.clone()),
        ..Program::default()
    };
    program.insert(result.module);
    let registry = program.callable_registry();
    let bound = marqdo_frontend::calls::bind_program_calls(&program, &registry);
    assert!(bound.diagnostics.is_empty(), "{:#?}", bound.diagnostics);
    program.set_call_bindings(bound.bindings);
    program
}

fn source() -> &'static str {
    "## @exec f\n\n`params(n: Int)`\n\n`if n <= 0`\n\n`return 0`\n\n`end`\n\n`x = n`\n\n`return x + f(n=n-1)`\n\n## @exec main\n\n`i = 0`\n\n`while i < 2`\n\n`i = i + 1`\n\n`end`\n\n`visible = 42`\n\n`secret = \"DO_NOT_LEAK_SENTINEL_1\"`\n\n`return f(n=2)`\n"
}

fn run_observed(config: ObservationConfig) -> (Value, Vec<Seen>) {
    let program = build(source());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::pure(),
        std::env::temp_dir(),
    )
    .with_observation(config, Box::new(Recorder(Rc::clone(&seen))));
    let outcome = run(
        &program,
        &mut host,
        &ModuleId::from("index.mq.md"),
        "main",
        Vec::new(),
    );
    let value = outcome.map(|outcome| outcome.value).unwrap();
    let seen = seen.borrow().clone();
    (value, seen)
}

#[test]
fn recursive_calls_get_unique_frames_and_correct_parents() {
    let (value, seen) = run_observed(ObservationConfig::default());
    assert_eq!(value, Value::int(3));

    let enters: Vec<_> = seen.iter().filter(|event| event.kind == "enter").collect();
    assert_eq!(enters.len(), 4, "main + f(2) + f(1) + f(0)");
    let mut ids: Vec<u64> = enters.iter().filter_map(|event| event.frame).collect();
    ids.sort_unstable();
    ids.dedup();
    assert_eq!(ids.len(), 4, "frame ids must be run-unique");
    assert_eq!(
        enters[0].frame,
        Some(1),
        "the entry frame is the first frame"
    );
    // Nested calls are announced in call order; each later frame has a parent.
    for pair in enters.windows(2).skip(1).take(2) {
        assert!(pair[1].frame.unwrap() > pair[0].frame.unwrap());
    }
    let exits = seen.iter().filter(|event| event.kind == "exit").count();
    assert_eq!(exits, 4, "every entered frame closes, including recursion");
    let returns = seen.iter().filter(|event| event.kind == "return").count();
    assert_eq!(returns, 4);
    assert!(
        seen.iter().all(|event| event.wire.is_none()),
        "default observation publishes no value payloads"
    );
}

#[test]
fn branch_loop_and_assignments_are_attributed_and_default_hides_values() {
    let (_value, seen) = run_observed(ObservationConfig::default());
    let branches: Vec<_> = seen.iter().filter(|event| event.kind == "branch").collect();
    assert_eq!(
        branches.len(),
        3,
        "one condition per call: f(2), f(1), f(0)"
    );
    assert_eq!(
        branches.iter().filter(|event| event.name == "true").count(),
        1,
        "only the n <= 0 branch is true"
    );
    let loops: Vec<_> = seen.iter().filter(|event| event.kind == "loop").collect();
    assert_eq!(
        loops
            .iter()
            .map(|event| event.decision.as_str())
            .collect::<Vec<_>>(),
        vec!["continue", "continue", "exit"]
    );

    let mut variables: Vec<(&str, &str)> = seen
        .iter()
        .filter(|event| event.kind == "variable")
        .map(|event| (event.name.as_str(), event.value_type.as_str()))
        .collect();
    variables.sort();
    assert!(variables.contains(&("i", "Int")));
    assert!(variables.contains(&("visible", "Int")));
    assert!(variables.contains(&("secret", "Text")));
    assert!(variables.contains(&("x", "Int")));
    for event in seen.iter().filter(|event| event.kind == "variable") {
        assert_eq!(event.state, Some(ValueState::NotRequested));
        assert!(
            event.wire.is_none(),
            "default event mode publishes no values"
        );
    }
    assert!(!serde_json::to_string(
        &seen
            .iter()
            .filter_map(|event| event.wire.clone())
            .collect::<Vec<_>>()
    )
    .unwrap()
    .contains("DO_NOT_LEAK_SENTINEL_1"));
}

#[test]
fn exact_selection_publishes_only_selected_scalars_and_redacts_text() {
    let mut config = ObservationConfig::default();
    config.selected_variables.insert("visible".to_string());
    config.selected_variables.insert("secret".to_string());
    config.selected_parameters.insert("n".to_string());
    let (_value, seen) = run_observed(config);

    let visible = seen
        .iter()
        .find(|event| event.kind == "variable" && event.name == "visible")
        .expect("visible assignment event");
    assert_eq!(visible.state, Some(ValueState::Observed));
    assert_eq!(
        visible.wire.as_ref().unwrap()["decimal"],
        serde_json::Value::from("42")
    );

    let secret = seen
        .iter()
        .filter(|event| event.kind == "variable" && event.name == "secret")
        .collect::<Vec<_>>();
    assert!(!secret.is_empty());
    for event in secret {
        assert_eq!(event.state, Some(ValueState::Redacted));
        assert!(event.wire.is_none(), "text needs a second explicit consent");
    }

    let unselected: Vec<_> = seen
        .iter()
        .filter(|event| event.kind == "variable" && (event.name == "x" || event.name == "i"))
        .collect();
    assert!(!unselected.is_empty());
    for event in unselected {
        assert_eq!(event.state, Some(ValueState::NotRequested));
        assert!(event.wire.is_none(), "unselected names never carry values");
    }

    let parameters: Vec<_> = seen
        .iter()
        .filter(|event| event.kind == "parameter" && event.name == "n")
        .collect();
    assert_eq!(parameters.len(), 3, "f(2), f(1), f(0)");
    assert_eq!(
        parameters
            .iter()
            .map(|event| event.wire.as_ref().unwrap()["decimal"].as_str().unwrap())
            .collect::<Vec<_>>(),
        vec!["2", "1", "0"]
    );
    assert!(parameters
        .iter()
        .all(|event| event.source == Some(ParameterSource::Provided)));
}

#[test]
fn argument_binding_failure_creates_no_frame() {
    let program = build(source());
    let seen = Rc::new(RefCell::new(Vec::new()));
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::pure(),
        std::env::temp_dir(),
    )
    .with_observation(
        ObservationConfig::default(),
        Box::new(Recorder(Rc::clone(&seen))),
    );
    let result = run(
        &program,
        &mut host,
        &ModuleId::from("index.mq.md"),
        "f",
        vec![],
    );
    assert!(result.is_err(), "f requires n");
    assert!(
        seen.borrow().iter().all(|event| event.kind != "enter"),
        "a frame is announced only after successful binding"
    );
}
