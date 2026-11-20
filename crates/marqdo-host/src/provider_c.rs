//! C ABI resource provider v1 (ADR-0036).
//!
//! The library is trusted native code. Loading it shares the host address space
//! and is explicitly not a sandbox; only explicitly trusted adapters may be
//! registered.

use std::collections::BTreeMap;
use std::ffi::{c_char, CString};
use std::path::{Path, PathBuf};

use libloading::Library;
use marqdo_span::Span;
use marqdo_value::{ErrorCategory, RuntimeError};

use crate::resource::{ResourceProvider, ResourceRead};

const ABI_VERSION: u32 = 1;
const ERROR_MAX: usize = 1024;

type AbiVersionFn = unsafe extern "C" fn() -> u32;
type OpenFn = unsafe extern "C" fn(*const c_char, *const c_char, *const c_char, *mut u64) -> i32;
type ReadFn = unsafe extern "C" fn(u64, *mut u8, u32, *mut u32, *mut i32) -> i32;
type WriteFn = unsafe extern "C" fn(u64, *const u8, u32, *mut u32) -> i32;
type CloseFn = unsafe extern "C" fn(u64) -> i32;
type LastErrorFn = unsafe extern "C" fn(*mut c_char, u32) -> i32;

#[derive(Debug, serde::Deserialize)]
struct ProviderPackage {
    schema: String,
    schema_version: u32,
    name: String,
    kinds: Vec<String>,
    abi: String,
    #[serde(default)]
    library: BTreeMap<String, String>,
}

/// Load a C ABI provider from its package manifest (ADR-0037).
pub(crate) fn load_package(manifest_path: &Path) -> Result<CResourceProvider, RuntimeError> {
    let bytes = std::fs::read(manifest_path).map_err(|error| {
        RuntimeError::new(
            ErrorCategory::ResourceUnsupported,
            None,
            format!(
                "cannot read provider manifest {}: {error}",
                manifest_path.display()
            ),
        )
    })?;
    let package: ProviderPackage = serde_json::from_slice(&bytes).map_err(|error| {
        RuntimeError::new(
            ErrorCategory::ResourceUnsupported,
            None,
            format!(
                "invalid provider manifest {}: {error}",
                manifest_path.display()
            ),
        )
    })?;
    if package.schema != "marqdo.provider" || package.schema_version != 1 {
        return Err(provider_error(
            &package.name,
            "manifest schema must be marqdo.provider version 1",
        ));
    }
    if package.abi != "c-abi-1" {
        return Err(provider_error(
            &package.name,
            &format!(
                "unsupported provider ABI `{}` (expected c-abi-1)",
                package.abi
            ),
        ));
    }
    let platform = if cfg!(target_os = "linux") {
        "linux"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "default"
    };
    let library_name = package
        .library
        .get(platform)
        .or_else(|| package.library.get("default"))
        .ok_or_else(|| {
            provider_error(
                &package.name,
                &format!("manifest has no `{platform}` or `default` library entry"),
            )
        })?;
    let base = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let library = PathBuf::from(library_name);
    let library = if library.is_absolute() {
        library
    } else {
        base.join(library)
    };
    CResourceProvider::open(&package.name, &package.kinds, &library)
}

pub(crate) struct CResourceProvider {
    name: String,
    kinds: Vec<String>,
    library: Library,
}

impl CResourceProvider {
    pub(crate) fn open(name: &str, kinds: &[String], path: &Path) -> Result<Self, RuntimeError> {
        let library = unsafe { Library::new(path) }.map_err(|error| {
            RuntimeError::new(
                ErrorCategory::KernelUnavailable,
                None,
                format!(
                    "cannot load C resource provider `{name}` ({}): {error}",
                    path.display()
                ),
            )
        })?;
        let version = {
            let symbol: libloading::Symbol<AbiVersionFn> = unsafe {
                library
                    .get(b"marqdo_resource_provider_abi_version\0")
                    .map_err(|error| {
                        RuntimeError::new(
                            ErrorCategory::ResourceUnsupported,
                            None,
                            format!("C resource provider `{name}` is missing abi_version: {error}"),
                        )
                    })?
            };
            unsafe { symbol() }
        };
        if version != ABI_VERSION {
            return Err(RuntimeError::new(
                ErrorCategory::ResourceUnsupported,
                None,
                format!(
                    "C resource provider `{name}` uses ABI version {version}; host requires {ABI_VERSION}"
                ),
            ));
        }
        Ok(Self {
            name: name.to_string(),
            kinds: kinds.to_vec(),
            library,
        })
    }

    fn open_fn(&self) -> Result<libloading::Symbol<'_, OpenFn>, RuntimeError> {
        unsafe { self.library.get(b"marqdo_resource_open\0") }
            .map_err(|error| provider_error(&self.name, &format!("missing open: {error}")))
    }
    fn read_fn(&self) -> Result<libloading::Symbol<'_, ReadFn>, RuntimeError> {
        unsafe { self.library.get(b"marqdo_resource_read\0") }
            .map_err(|error| provider_error(&self.name, &format!("missing read: {error}")))
    }
    fn write_fn(&self) -> Result<libloading::Symbol<'_, WriteFn>, RuntimeError> {
        unsafe { self.library.get(b"marqdo_resource_write\0") }
            .map_err(|error| provider_error(&self.name, &format!("missing write: {error}")))
    }
    fn close_fn(&self) -> Result<libloading::Symbol<'_, CloseFn>, RuntimeError> {
        unsafe { self.library.get(b"marqdo_resource_close\0") }
            .map_err(|error| provider_error(&self.name, &format!("missing close: {error}")))
    }

    fn last_error(&self) -> String {
        let Ok(symbol) = (unsafe {
            self.library
                .get::<LastErrorFn>(b"marqdo_resource_last_error\0")
        }) else {
            return format!("C resource provider `{}` failed", self.name);
        };
        let mut buffer = vec![0i8; ERROR_MAX];
        let code = unsafe { symbol(buffer.as_mut_ptr(), ERROR_MAX as u32) };
        if code != 0 {
            return format!("C resource provider `{}` failed", self.name);
        }
        let bytes = buffer
            .iter()
            .map(|value| *value as u8)
            .take_while(|value| *value != 0)
            .collect::<Vec<_>>();
        String::from_utf8_lossy(&bytes).into_owned()
    }

    fn failure(&self, span: Option<Span>) -> RuntimeError {
        let message = self.last_error();
        RuntimeError::new(ErrorCategory::HostError, span, message)
    }
}

impl ResourceProvider for CResourceProvider {
    fn name(&self) -> &str {
        &self.name
    }
    fn kinds(&self) -> &[&str] {
        &[]
    }
    fn kind_strings(&self) -> Vec<String> {
        self.kinds.clone()
    }
    fn open(
        &mut self,
        kind: &str,
        descriptor: &str,
        rights: &[String],
        span: Option<Span>,
    ) -> Result<u64, RuntimeError> {
        let kind = CString::new(kind).map_err(|_| provider_error(&self.name, "kind has NUL"))?;
        let descriptor = CString::new(descriptor)
            .map_err(|_| provider_error(&self.name, "descriptor has NUL"))?;
        let rights = CString::new(rights.join(","))
            .map_err(|_| provider_error(&self.name, "rights have NUL"))?;
        let mut token = 0u64;
        let code = unsafe {
            (self.open_fn()?)(
                kind.as_ptr(),
                descriptor.as_ptr(),
                rights.as_ptr(),
                &mut token,
            )
        };
        if code != 0 {
            return Err(self.failure(span));
        }
        Ok(token)
    }
    fn read(
        &mut self,
        token: u64,
        max_bytes: usize,
        span: Option<Span>,
    ) -> Result<ResourceRead, RuntimeError> {
        let capacity = u32::try_from(max_bytes).unwrap_or(u32::MAX);
        let mut buffer = vec![0u8; capacity as usize];
        let mut read = 0u32;
        let mut eof = 0i32;
        let code =
            unsafe { (self.read_fn()?)(token, buffer.as_mut_ptr(), capacity, &mut read, &mut eof) };
        if code != 0 {
            return Err(self.failure(span));
        }
        buffer.truncate(read as usize);
        Ok(ResourceRead {
            bytes: buffer,
            eof: eof != 0,
        })
    }
    fn write(
        &mut self,
        token: u64,
        bytes: &[u8],
        span: Option<Span>,
    ) -> Result<usize, RuntimeError> {
        let length = u32::try_from(bytes.len())
            .map_err(|_| provider_error(&self.name, "write size exceeds the C ABI u32 limit"))?;
        let mut written = 0u32;
        let code = unsafe { (self.write_fn()?)(token, bytes.as_ptr(), length, &mut written) };
        if code != 0 {
            return Err(self.failure(span));
        }
        Ok(written as usize)
    }
    fn close(&mut self, token: u64, span: Option<Span>) -> Result<(), RuntimeError> {
        let code = unsafe { (self.close_fn()?)(token) };
        if code != 0 {
            return Err(self.failure(span));
        }
        Ok(())
    }
}

impl std::fmt::Debug for CResourceProvider {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CResourceProvider")
            .field("name", &self.name)
            .field("kinds", &self.kinds)
            .finish()
    }
}

fn provider_error(name: &str, message: &str) -> RuntimeError {
    RuntimeError::new(
        ErrorCategory::HostError,
        None,
        format!("C resource provider `{name}`: {message}"),
    )
}
