//! Trusted C ABI provider worker (ADR-0038).
//!
//! This helper loads one trusted `c-abi-1` shared library and serves the
//! ADR-0035 framed-JSON resource protocol over stdin/stdout. It is a transport
//! boundary, not a sandbox: the library runs with the worker's privileges.

use std::ffi::{c_char, CString};
use std::path::PathBuf;

use libloading::Library;
use serde_json::{json, Value as Json};

const ABI_VERSION: u32 = 1;
const MAX_FRAME_BYTES: usize = 1024 * 1024;
const ERROR_MAX: usize = 1024;

type AbiVersionFn = unsafe extern "C" fn() -> u32;
type OpenFn = unsafe extern "C" fn(*const c_char, *const c_char, *const c_char, *mut u64) -> i32;
type ReadFn = unsafe extern "C" fn(u64, *mut u8, u32, *mut u32, *mut i32) -> i32;
type WriteFn = unsafe extern "C" fn(u64, *const u8, u32, *mut u32) -> i32;
type CloseFn = unsafe extern "C" fn(u64) -> i32;
type LastErrorFn = unsafe extern "C" fn(*mut c_char, u32) -> i32;

struct Provider {
    library: Library,
}

impl Provider {
    fn load(path: &PathBuf) -> Result<Self, String> {
        let library = unsafe { Library::new(path) }
            .map_err(|error| format!("cannot load `{}`: {error}", path.display()))?;
        let version: libloading::Symbol<AbiVersionFn> = unsafe {
            library
                .get(b"marqdo_resource_provider_abi_version\0")
                .map_err(|error| format!("missing abi_version: {error}"))?
        };
        if unsafe { version() } != ABI_VERSION {
            return Err(format!("provider ABI is not {ABI_VERSION}"));
        }
        Ok(Self { library })
    }

    fn last_error(&self) -> String {
        let Ok(symbol) = (unsafe {
            self.library
                .get::<LastErrorFn>(b"marqdo_resource_last_error\0")
        }) else {
            return "C provider failed".to_string();
        };
        let mut buffer = vec![0i8; ERROR_MAX];
        if unsafe { symbol(buffer.as_mut_ptr(), ERROR_MAX as u32) } != 0 {
            return "C provider failed".to_string();
        }
        let bytes = buffer
            .iter()
            .map(|value| *value as u8)
            .take_while(|value| *value != 0)
            .collect::<Vec<_>>();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn open(&self, kind: &str, descriptor: &str, rights: &[String]) -> Result<u64, String> {
        let kind = CString::new(kind).map_err(|_| "kind has NUL".to_string())?;
        let descriptor = CString::new(descriptor).map_err(|_| "descriptor has NUL".to_string())?;
        let rights = CString::new(rights.join(",")).map_err(|_| "rights have NUL".to_string())?;
        let symbol: libloading::Symbol<OpenFn> = unsafe {
            self.library
                .get(b"marqdo_resource_open\0")
                .map_err(|error| format!("missing open: {error}"))?
        };
        let mut token = 0u64;
        if unsafe {
            symbol(
                kind.as_ptr(),
                descriptor.as_ptr(),
                rights.as_ptr(),
                &mut token,
            )
        } != 0
        {
            return Err(self.last_error());
        }
        Ok(token)
    }

    fn read(&self, token: u64, max_bytes: usize) -> Result<(Vec<u8>, bool), String> {
        let capacity = u32::try_from(max_bytes).unwrap_or(u32::MAX);
        let mut buffer = vec![0u8; capacity as usize];
        let mut read = 0u32;
        let mut eof = 0i32;
        let symbol: libloading::Symbol<ReadFn> = unsafe {
            self.library
                .get(b"marqdo_resource_read\0")
                .map_err(|error| format!("missing read: {error}"))?
        };
        if unsafe { symbol(token, buffer.as_mut_ptr(), capacity, &mut read, &mut eof) } != 0 {
            return Err(self.last_error());
        }
        buffer.truncate(read as usize);
        Ok((buffer, eof != 0))
    }

    fn write(&self, token: u64, bytes: &[u8]) -> Result<usize, String> {
        let length = u32::try_from(bytes.len()).map_err(|_| "write too large".to_string())?;
        let mut written = 0u32;
        let symbol: libloading::Symbol<WriteFn> = unsafe {
            self.library
                .get(b"marqdo_resource_write\0")
                .map_err(|error| format!("missing write: {error}"))?
        };
        if unsafe { symbol(token, bytes.as_ptr(), length, &mut written) } != 0 {
            return Err(self.last_error());
        }
        Ok(written as usize)
    }

    fn close(&self, token: u64) -> Result<(), String> {
        let symbol: libloading::Symbol<CloseFn> = unsafe {
            self.library
                .get(b"marqdo_resource_close\0")
                .map_err(|error| format!("missing close: {error}"))?
        };
        if unsafe { symbol(token) } != 0 {
            return Err(self.last_error());
        }
        Ok(())
    }
}

fn read_frame() -> Result<Option<Json>, String> {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    marqdo_worker_ipc::read_frame(&mut input, MAX_FRAME_BYTES).map_err(|error| error.to_string())
}

fn write_frame(value: &Json) -> Result<(), String> {
    let stdout = std::io::stdout();
    let mut output = stdout.lock();
    marqdo_worker_ipc::write_frame(&mut output, value, MAX_FRAME_BYTES)
        .map_err(|error| error.to_string())
}

fn fail(message: &str) -> Json {
    json!({"ok": false, "code": "host.error", "message": message})
}

fn main() {
    let Some(path) = std::env::args_os().nth(1) else {
        eprintln!("usage: marqdo-provider-worker <c-abi-library>");
        std::process::exit(2);
    };
    let provider = match Provider::load(&PathBuf::from(path)) {
        Ok(provider) => provider,
        Err(error) => {
            eprintln!("provider worker: {error}");
            std::process::exit(2);
        }
    };
    loop {
        let request = match read_frame() {
            Ok(Some(request)) => request,
            Ok(None) => break,
            Err(error) => {
                eprintln!("provider worker: {error}");
                std::process::exit(1);
            }
        };
        let op = request.get("op").and_then(Json::as_str).unwrap_or("");
        let response = match op {
            "ping" => json!({"ok": true}),
            "open" => {
                let kind = request.get("kind").and_then(Json::as_str).unwrap_or("");
                let descriptor = request
                    .get("descriptor")
                    .and_then(Json::as_str)
                    .unwrap_or("");
                let rights = request
                    .get("rights")
                    .and_then(Json::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(Json::as_str)
                            .map(str::to_string)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                match provider.open(kind, descriptor, &rights) {
                    Ok(token) => json!({"ok": true, "token": token}),
                    Err(error) => fail(&error),
                }
            }
            "read" => {
                let token = request.get("token").and_then(Json::as_u64).unwrap_or(0);
                let max_bytes =
                    request.get("max_bytes").and_then(Json::as_u64).unwrap_or(0) as usize;
                match provider.read(token, max_bytes) {
                    Ok((bytes, eof)) => json!({"ok": true, "bytes": bytes, "eof": eof}),
                    Err(error) => fail(&error),
                }
            }
            "write" => {
                let token = request.get("token").and_then(Json::as_u64).unwrap_or(0);
                let bytes = request
                    .get("bytes")
                    .and_then(Json::as_array)
                    .map(|values| {
                        values
                            .iter()
                            .filter_map(Json::as_u64)
                            .filter_map(|value| u8::try_from(value).ok())
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default();
                match provider.write(token, &bytes) {
                    Ok(count) => json!({"ok": true, "count": count}),
                    Err(error) => fail(&error),
                }
            }
            "close" => {
                let token = request.get("token").and_then(Json::as_u64).unwrap_or(0);
                match provider.close(token) {
                    Ok(()) => json!({"ok": true}),
                    Err(error) => fail(&error),
                }
            }
            "shutdown" => break,
            _ => fail("unsupported worker operation"),
        };
        if let Err(error) = write_frame(&response) {
            eprintln!("provider worker: {error}");
            std::process::exit(1);
        }
    }
}
