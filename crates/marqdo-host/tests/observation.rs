//! Host observation facade tests (RFC-OBS-0001 / ADR-0059).
//!
//! These pin the internal contract: stdio chunks come from the real host
//! commit point, the default path is inert, frames are paired, budgets
//! truncate only non-critical events and a broken sink aborts the run.

use std::cell::RefCell;
use std::rc::Rc;

use marqdo_host::{
    encode_value, Capabilities, ExecutionContext, FrameId, Host, ObservationConfig,
    ObservationError, ObservationEvent, ObservationSink, ParameterSource, RuntimeObservation,
    StdHost, ValueState,
};
use marqdo_sandbox::SandboxMode;
use marqdo_value::{ErrorCategory, Value};

fn context() -> ExecutionContext {
    ExecutionContext::new("index.mq.md".to_string(), None)
}

#[derive(Debug, Clone, PartialEq)]
enum Recorded {
    Stdout(Vec<u8>),
    Stderr(Vec<u8>),
    Enter {
        frame: FrameId,
        parent: Option<FrameId>,
        function: String,
    },
    Exit {
        frame: FrameId,
        status: String,
    },
    Return {
        frame: FrameId,
        value_type: String,
    },
    Branch {
        frame: FrameId,
        result: bool,
    },
    Loop {
        frame: FrameId,
        decision: String,
    },
    Variable {
        frame: FrameId,
        name: String,
        value_type: String,
        state: ValueState,
        value: Option<serde_json::Value>,
    },
    Parameter {
        frame: FrameId,
        name: String,
        source: ParameterSource,
        state: ValueState,
        value: Option<serde_json::Value>,
    },
    Effect {
        operation: String,
        result_code: String,
    },
    Truncated {
        reason: String,
        limit: u64,
        written: u64,
    },
}

impl From<ObservationEvent<'_>> for Recorded {
    fn from(event: ObservationEvent<'_>) -> Self {
        match event {
            ObservationEvent::StdoutChunk { data } => Recorded::Stdout(data.to_vec()),
            ObservationEvent::StderrChunk { data } => Recorded::Stderr(data.to_vec()),
            ObservationEvent::CallEnter {
                frame_id,
                parent_frame_id,
                function_id,
                ..
            } => Recorded::Enter {
                frame: frame_id,
                parent: parent_frame_id,
                function: function_id.to_string(),
            },
            ObservationEvent::CallExit {
                frame_id, status, ..
            } => Recorded::Exit {
                frame: frame_id,
                status: status.to_string(),
            },
            ObservationEvent::Return {
                frame_id,
                value_type,
                ..
            } => Recorded::Return {
                frame: frame_id,
                value_type: value_type.to_string(),
            },
            ObservationEvent::BranchEval {
                frame_id, result, ..
            } => Recorded::Branch {
                frame: frame_id,
                result,
            },
            ObservationEvent::LoopIteration {
                frame_id, decision, ..
            } => Recorded::Loop {
                frame: frame_id,
                decision: decision.to_string(),
            },
            ObservationEvent::Effect {
                operation,
                result_code,
                ..
            } => Recorded::Effect {
                operation: operation.to_string(),
                result_code: result_code.to_string(),
            },
            ObservationEvent::VariableAssigned {
                frame_id,
                name,
                value_type,
                value_state,
                value,
                ..
            } => Recorded::Variable {
                frame: frame_id,
                name: name.to_string(),
                value_type: value_type.to_string(),
                state: value_state,
                value: value.map(|value| value.wire),
            },
            ObservationEvent::ParameterObserved {
                frame_id,
                name,
                source,
                value_state,
                value,
                ..
            } => Recorded::Parameter {
                frame: frame_id,
                name: name.to_string(),
                source,
                state: value_state,
                value: value.map(|value| value.wire),
            },
            ObservationEvent::ObservationTruncated {
                reason,
                limit,
                written,
                ..
            } => Recorded::Truncated {
                reason: reason.to_string(),
                limit,
                written,
            },
        }
    }
}

struct Recorder {
    events: Rc<RefCell<Vec<Recorded>>>,
    fail: bool,
}

impl ObservationSink for Recorder {
    fn emit(&mut self, event: ObservationEvent<'_>) -> Result<(), ObservationError> {
        if self.fail {
            return Err(ObservationError::new("synthetic broken pipe"));
        }
        self.events.borrow_mut().push(event.into());
        Ok(())
    }
}

fn observed_host(config: ObservationConfig) -> (StdHost, Rc<RefCell<Vec<Recorded>>>) {
    let events = Rc::new(RefCell::new(Vec::new()));
    let host = StdHost::new(
        SandboxMode::Project,
        Capabilities::pure(),
        std::env::temp_dir(),
    )
    .with_observation(
        config,
        Box::new(Recorder {
            events: Rc::clone(&events),
            fail: false,
        }),
    );
    (host, events)
}

#[test]
fn stdout_and_stderr_chunks_are_emitted_at_the_commit_point() {
    let (mut host, events) = observed_host(ObservationConfig::default());
    let ctx = context();
    let stdout = match host
        .resource_open("stdio", "stdout", &["write".to_string()], &ctx)
        .unwrap()
    {
        Value::Resource(handle) => handle,
        other => panic!("expected a resource handle, got {other:?}"),
    };
    let stderr = match host
        .resource_open("stdio", "stderr", &["write".to_string()], &ctx)
        .unwrap()
    {
        Value::Resource(handle) => handle,
        other => panic!("expected a resource handle, got {other:?}"),
    };
    host.resource_write(stdout, b"START\n", &ctx).unwrap();
    host.resource_write(stderr, &[0xff, 0x00, 0xfe], &ctx)
        .unwrap();
    host.resource_write(stdout, b"DONE\n", &ctx).unwrap();
    let events = events.borrow();
    assert_eq!(
        events
            .iter()
            .filter_map(|event| match event {
                Recorded::Stdout(bytes) => Some(bytes.clone()),
                _ => None,
            })
            .collect::<Vec<_>>(),
        vec![b"START\n".to_vec(), b"DONE\n".to_vec()],
        "each committed stdout write must produce its own chunk event"
    );
    assert!(events.contains(&Recorded::Stderr(vec![0xff, 0x00, 0xfe])));
    assert_eq!(host.take_output(), b"START\nDONE\n");
}

#[test]
fn rejected_output_write_emits_no_chunk() {
    let (mut host, events) = observed_host(ObservationConfig::default());
    host.budget.max_output_bytes = 3;
    let ctx = context();
    let stdout = match host
        .resource_open("stdio", "stdout", &["write".to_string()], &ctx)
        .unwrap()
    {
        Value::Resource(handle) => handle,
        other => panic!("expected a resource handle, got {other:?}"),
    };
    let error = host.resource_write(stdout, b"hello", &ctx).unwrap_err();
    assert_eq!(error.category, ErrorCategory::ResourceExhausted);
    assert!(
        events
            .borrow()
            .iter()
            .all(|event| !matches!(event, Recorded::Stdout(_))),
        "an output write rejected by the host budget must not be observed"
    );
    assert!(host.take_output().is_empty());
}

#[test]
fn large_writes_are_split_into_bounded_chunks() {
    let (mut host, events) = observed_host(ObservationConfig::default());
    let ctx = context();
    let stdout = match host
        .resource_open("stdio", "stdout", &["write".to_string()], &ctx)
        .unwrap()
    {
        Value::Resource(handle) => handle,
        other => panic!("expected a resource handle, got {other:?}"),
    };
    let data = vec![b'x'; 150_000];
    host.resource_write(stdout, &data, &ctx).unwrap();
    let chunks: Vec<Vec<u8>> = events
        .borrow()
        .iter()
        .filter_map(|event| match event {
            Recorded::Stdout(bytes) => Some(bytes.clone()),
            _ => None,
        })
        .collect();
    assert_eq!(chunks.len(), 3);
    assert!(chunks.iter().all(|chunk| chunk.len() <= 64 * 1024));
    assert_eq!(chunks.concat(), data);
}

#[test]
fn frames_are_unique_paired_and_parented() {
    let (mut host, events) = observed_host(ObservationConfig::default());
    let main = host
        .observation_begin_frame("root::main", "root::index.mq.md", None)
        .unwrap()
        .expect("observation enabled must allocate a frame");
    let child = host
        .observation_begin_frame("root::f", "root::index.mq.md", None)
        .unwrap()
        .unwrap();
    let grandchild = host
        .observation_begin_frame("root::g", "root::index.mq.md", None)
        .unwrap()
        .unwrap();
    host.observe_runtime(RuntimeObservation::Return {
        frame_id: Some(grandchild),
        value_type: "Int",
        module_id: "root::index.mq.md",
        span: None,
    })
    .unwrap();
    host.observation_end_frame(Some(grandchild), "return", "root::index.mq.md", None)
        .unwrap();
    host.observation_end_frame(Some(child), "ok", "root::index.mq.md", None)
        .unwrap();
    host.observation_end_frame(Some(main), "ok", "root::index.mq.md", None)
        .unwrap();

    let events = events.borrow();
    let enters: Vec<_> = events
        .iter()
        .filter_map(|event| match event {
            Recorded::Enter { frame, parent, .. } => Some((*frame, *parent)),
            _ => None,
        })
        .collect();
    assert_eq!(enters.len(), 3);
    assert_eq!(enters[0].1, None);
    assert_eq!(enters[1].1, Some(main));
    assert_eq!(enters[2].1, Some(child));
    let exits = events
        .iter()
        .filter(|event| matches!(event, Recorded::Exit { .. }))
        .count();
    assert_eq!(exits, 3);
}

#[test]
fn structural_budget_truncates_with_one_marker_and_keeps_critical_events() {
    let config = ObservationConfig {
        max_observation_events: 2,
        ..ObservationConfig::default()
    };
    let (mut host, events) = observed_host(config);
    let ctx = context();
    let stdout = match host
        .resource_open("stdio", "stdout", &["write".to_string()], &ctx)
        .unwrap()
    {
        Value::Resource(handle) => handle,
        other => panic!("expected a resource handle, got {other:?}"),
    };
    let first = host
        .observation_begin_frame("f1", "m", None)
        .unwrap()
        .unwrap();
    host.observation_end_frame(Some(first), "ok", "m", None)
        .unwrap();
    // Third structural event exceeds the two-event budget.
    host.observation_begin_frame("f2", "m", None).unwrap();
    host.resource_write(stdout, b"still-visible", &ctx).unwrap();

    let events = events.borrow();
    let markers = events
        .iter()
        .filter(|event| matches!(event, Recorded::Truncated { .. }))
        .count();
    assert_eq!(markers, 1, "exactly one truncation marker");
    assert!(
        events.contains(&Recorded::Stdout(b"still-visible".to_vec())),
        "stdout chunks are critical and survive the structural budget"
    );
    assert!(events.iter().any(|event| matches!(
        event,
        Recorded::Truncated { reason, limit, written }
            if reason == "event_budget" && *limit == 2 && *written == 2
    )));
}

#[test]
fn broken_sink_aborts_and_is_saved_for_the_next_boundary() {
    let events = Rc::new(RefCell::new(Vec::new()));
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::pure(),
        std::env::temp_dir(),
    )
    .with_observation(
        ObservationConfig::default(),
        Box::new(Recorder {
            events: Rc::clone(&events),
            fail: true,
        }),
    );
    let error = host
        .observation_begin_frame("f", "m", None)
        .expect_err("a broken sink must propagate");
    assert_eq!(error.category, ErrorCategory::Observation);
    assert_eq!(error.code(), "observe.transport_failed");
    assert_eq!(host.observation_failure(), Some("synthetic broken pipe"));
    let boundary = host.consume_fuel(1, None).expect_err("next boundary fails");
    assert_eq!(boundary.category, ErrorCategory::Observation);
}

#[test]
fn default_host_has_no_observation_state() {
    let mut host = StdHost::new(
        SandboxMode::Project,
        Capabilities::pure(),
        std::env::temp_dir(),
    );
    assert!(!host.observation_enabled());
    assert!(host.observation_config().is_none());
    assert_eq!(host.observation_begin_frame("f", "m", None).unwrap(), None);
    host.observation_end_frame(None, "ok", "m", None).unwrap();
}

#[test]
fn value_encoding_is_selected_bounded_and_handle_safe() {
    let mut config = ObservationConfig {
        max_value_bytes: 8,
        ..ObservationConfig::default()
    };
    assert_eq!(
        encode_value(&config, false, &Value::int(42)),
        (ValueState::NotRequested, None)
    );
    let (state, value) = encode_value(&config, true, &Value::int(42));
    assert_eq!(state, ValueState::Observed);
    assert_eq!(value.unwrap().wire["decimal"], "42");

    const SENTINEL: &str = "DO_NOT_LEAK_SENTINEL_1";
    let (state, value) = encode_value(&config, true, &Value::text(SENTINEL));
    assert_eq!(state, ValueState::Redacted);
    assert!(value.is_none(), "text requires explicit consent");

    config.allow_text_values = true;
    let (state, value) = encode_value(&config, true, &Value::text(SENTINEL));
    assert_eq!(state, ValueState::Truncated);
    let wire = value.unwrap().wire;
    assert_eq!(wire["truncated"], true);
    assert!(wire["value"].as_str().unwrap().len() <= 8);
    assert!(!wire.to_string().contains(SENTINEL));

    config.allow_text_values = true;
    let text = "开始文本\n";
    let (state, value) = encode_value(&config, true, &Value::text(text));
    assert_eq!(state, ValueState::Truncated);
    let wire = value.unwrap().wire;
    assert!(
        text.starts_with(wire["value"].as_str().unwrap()),
        "truncation must keep a UTF-8 scalar prefix"
    );

    let handle_value = Value::None;
    let (state, _) = encode_value(&config, true, &handle_value);
    assert_eq!(state, ValueState::Unsupported);
    let (state, value) = encode_value(&config, true, &Value::List(vec![Value::int(1)]));
    assert_eq!(state, ValueState::Unsupported);
    assert!(value.is_none());
}
