//! End-to-end SDK tests: run the macro-generated entry point against a tiny
//! in-process host table and invoke the registered export.

use std::ffi::c_void;
use std::ptr;

use marqdo_plugin_abi::abi::{
    result, MqBytes, MqCallContext, MqHostApi, MqOwnedBytes, MqPluginApi, MqPluginInvoke,
    ABI_MAJOR, ABI_MINOR,
};

use crate::wire;
use crate::{CallContext, PluginError, Value, WireRequest, WireResponse};

/// ABI v1 is single-threaded: the generated plugin stores the host table in a
/// process-global. Serialize tests that call the entry point so they do not
/// race (the SDK's own contract, not a test-only limitation).
static ABI_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn abi_test_lock() -> std::sync::MutexGuard<'static, ()> {
    ABI_TEST_LOCK
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
}

#[derive(Default)]
struct TestHostState {
    registrations: Vec<(String, String, MqPluginInvoke)>,
}

unsafe extern "C" fn test_alloc(_host_data: *mut c_void, size: usize) -> *mut c_void {
    const HEADER: usize = 16;
    if size == 0 {
        return ptr::null_mut();
    }
    let Ok(layout) = std::alloc::Layout::from_size_align(size + HEADER, 16) else {
        return ptr::null_mut();
    };
    // SAFETY: non-zero layout.
    let base = unsafe { std::alloc::alloc(layout) };
    if base.is_null() {
        return ptr::null_mut();
    }
    // SAFETY: allocation has room for the header.
    unsafe { (base as *mut usize).write(size) };
    // SAFETY: offset within the allocation.
    unsafe { base.add(HEADER) as *mut c_void }
}

unsafe extern "C" fn test_free(_host_data: *mut c_void, pointer: *mut c_void) {
    const HEADER: usize = 16;
    if pointer.is_null() {
        return;
    }
    // SAFETY: only pointers from test_alloc are freed.
    unsafe {
        let base = (pointer as *mut u8).sub(HEADER);
        let size = (base as *const usize).read();
        std::alloc::dealloc(
            base,
            std::alloc::Layout::from_size_align_unchecked(size + HEADER, 16),
        );
    }
}

unsafe extern "C" fn test_register(
    host_data: *mut c_void,
    export_name: MqBytes,
    signature_wire: MqBytes,
    invoke: MqPluginInvoke,
) -> i32 {
    if host_data.is_null() {
        return result::INVALID_ARGUMENT;
    }
    // SAFETY: host_data points at TestHostState for the test.
    let state = unsafe { &mut *(host_data as *mut TestHostState) };
    // SAFETY: buffers are valid for this callback.
    let name = match unsafe { export_name.as_slice() } {
        Ok(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        Err(_) => return result::INVALID_ARGUMENT,
    };
    // SAFETY: buffers are valid for this callback.
    let signature = match unsafe { signature_wire.as_slice() } {
        Ok(bytes) => String::from_utf8_lossy(bytes).into_owned(),
        Err(_) => return result::INVALID_ARGUMENT,
    };
    state.registrations.push((name, signature, invoke));
    result::OK
}

unsafe extern "C" fn test_capability(
    _ctx: *mut MqCallContext,
    _name: MqBytes,
    _request: MqBytes,
    reply: *mut MqOwnedBytes,
) -> i32 {
    if !reply.is_null() {
        // SAFETY: output pointer is non-NULL.
        unsafe { *reply = MqOwnedBytes::EMPTY };
    }
    result::UNSUPPORTED
}

fn add(_ctx: &mut CallContext, args: &[u8]) -> Result<Vec<u8>, PluginError> {
    let request = wire::decode_request(args)?;
    let values = request.typed_args()?;
    let a = wire::int_arg(&values, "a")?;
    let b = wire::int_arg(&values, "b")?;
    wire::success(Some(Value::Int((a + b).into())))
}

crate::mq_plugin_entry! {
    name: "sdk.test",
    version: "0.1.0",
    exports: {
        "test.add" => "($a: Int, $b: Int) -> Int" => add,
    }
}

struct TestHost {
    _state: Box<TestHostState>,
    api: MqHostApi,
}

fn test_host() -> TestHost {
    let mut state = Box::new(TestHostState::default());
    let api = MqHostApi {
        struct_size: std::mem::size_of::<MqHostApi>() as u32,
        abi_major: ABI_MAJOR,
        abi_minor: ABI_MINOR,
        host_data: (&mut *state) as *mut TestHostState as *mut c_void,
        alloc: test_alloc,
        free: test_free,
        register_function: test_register,
        request_capability: test_capability,
    };
    TestHost { _state: state, api }
}

#[test]
fn macro_exports_and_registers() {
    let _guard = abi_test_lock();
    let host = test_host();
    let mut plugin_api = MqPluginApi::host_initialized();
    // SAFETY: test host table follows the ABI contract.
    let code = unsafe { mq_plugin_entry_v1(&host.api as *const MqHostApi, &mut plugin_api) };
    assert_eq!(code, result::OK);
    assert_eq!(plugin_api.abi_major, ABI_MAJOR);
    assert_eq!(plugin_api.abi_minor, ABI_MINOR);
    // SAFETY: metadata is plugin static data valid during the entry call.
    let metadata = unsafe { plugin_api.plugin_metadata_wire.as_slice() }.expect("metadata");
    let metadata = String::from_utf8_lossy(metadata);
    assert!(metadata.contains("\"name\":\"sdk.test\""));
    assert_eq!(host._state.registrations.len(), 1);
    assert_eq!(host._state.registrations[0].0, "test.add");
    assert!(host._state.registrations[0]
        .1
        .contains("($a: Int, $b: Int) -> Int"));
}

#[test]
fn registered_export_roundtrips_wire_request() {
    let _guard = abi_test_lock();
    let host = test_host();
    let mut plugin_api = MqPluginApi::host_initialized();
    // SAFETY: test host table follows the ABI contract.
    let code = unsafe { mq_plugin_entry_v1(&host.api as *const MqHostApi, &mut plugin_api) };
    assert_eq!(code, result::OK);

    let request = WireRequest::new("test.add")
        .with_arg("a", &Value::Int(20_i64.into()))
        .with_arg("b", &Value::Int(22_i64.into()));
    let request_bytes = request.to_json_bytes().expect("request");
    let (_, _, invoke) = &host._state.registrations[0];
    let mut result_buffer = MqOwnedBytes::EMPTY;
    let mut error_buffer = MqOwnedBytes::EMPTY;
    // SAFETY: invoke is the SDK callback; null ctx is fine because the handler
    // does not request a capability. Buffers are valid out-parameters.
    let code = unsafe {
        invoke(
            ptr::null_mut(),
            MqBytes::from_slice(&request_bytes),
            &mut result_buffer,
            &mut error_buffer,
        )
    };
    assert_eq!(code, result::OK);
    assert!(error_buffer.is_empty());
    // SAFETY: result buffer was allocated by test_alloc for us.
    let response_bytes = unsafe { result_buffer.to_vec() }.expect("result");
    // SAFETY: allocated by test_alloc; release exactly once.
    unsafe { test_free(ptr::null_mut(), result_buffer.data as *mut c_void) };

    let response = WireResponse::from_json_bytes(&response_bytes).expect("response");
    assert_eq!(
        response.typed_value().expect("typed"),
        Some(Value::Int(42_i64.into()))
    );
}

#[test]
fn handler_error_is_written_as_error_envelope() {
    let _guard = abi_test_lock();
    let host = test_host();
    let mut plugin_api = MqPluginApi::host_initialized();
    // SAFETY: test host table follows the ABI contract.
    let code = unsafe { mq_plugin_entry_v1(&host.api as *const MqHostApi, &mut plugin_api) };
    assert_eq!(code, result::OK);

    let request = WireRequest::new("test.add"); // missing arguments
    let request_bytes = request.to_json_bytes().expect("request");
    let (_, _, invoke) = &host._state.registrations[0];
    let mut result_buffer = MqOwnedBytes::EMPTY;
    let mut error_buffer = MqOwnedBytes::EMPTY;
    // SAFETY: invoke is the SDK callback with valid out-parameters.
    let code = unsafe {
        invoke(
            ptr::null_mut(),
            MqBytes::from_slice(&request_bytes),
            &mut result_buffer,
            &mut error_buffer,
        )
    };
    assert_ne!(code, result::OK);
    assert!(result_buffer.is_empty());
    // SAFETY: error buffer was allocated by test_alloc for us.
    let error_bytes = unsafe { error_buffer.to_vec() }.expect("error");
    // SAFETY: allocated by test_alloc; release exactly once.
    unsafe { test_free(ptr::null_mut(), error_buffer.data as *mut c_void) };
    let response = WireResponse::from_error_json_bytes(&error_bytes).expect("decode");
    let error = response.into_result().expect_err("error");
    assert_eq!(error.code, "missing_argument");
    assert_eq!(error.kind, marqdo_plugin_abi::ErrorKind::Invoke);
}

#[test]
fn initialization_rejects_bad_host_major() {
    let _guard = abi_test_lock();
    let mut host = test_host();
    host.api.abi_major = 2;
    let mut plugin_api = MqPluginApi::host_initialized();
    // SAFETY: intentionally inconsistent host table; entry must reject it.
    let code = unsafe { mq_plugin_entry_v1(&host.api as *const MqHostApi, &mut plugin_api) };
    assert_eq!(code, result::ABI_MISMATCH);
}
