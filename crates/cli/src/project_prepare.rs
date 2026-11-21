//! The single `prepare` composition-root step (RFC-EXT-0001 §4.4, §18.2).
//!
//! Order of operations (T1–T5 of the RFC):
//!
//! 1. load the Markdown/package graph (read only, no execution);
//! 2. read static extension descriptors (never `dlopen`/spawn);
//! 3. build the unique [`CallableRegistry`] and reject static conflicts;
//! 4. run the single program-wide call binding pass;
//! 5. produce a [`PreparedProgram`] consumed by `check`, `run`, `call`,
//!    `inspect` and Analysis.
//!
//! `run` may only assemble the Host and start workers after this returns.

use std::path::PathBuf;

use marqdo_ir::callable::CallableRegistry;
use marqdo_package::{ExtensionKind, LockedExtension};

use crate::extension_descriptors::{read_plugin_descriptors, PrepareError, StaticPluginSet};
use crate::extension_selection;
use crate::loader::{self, FileDiagnostic, LoadedProject};

/// Options that influence static preparation.
#[derive(Debug, Clone, Default)]
pub struct PrepareOptions {
    /// Explicit `--plugin` manifests (repeatable, no implicit discovery).
    pub plugins: Vec<PathBuf>,
}

/// A statically prepared program: everything `check`/`run`/`inspect` share.
pub struct PreparedProgram {
    /// Loaded project graph with authoritative diagnostics.
    pub loaded: LoadedProject,
    /// Static plugin descriptors (identity only; no binary loaded).
    pub plugins: StaticPluginSet,
    /// Unique callable registry used by the binding pass.
    ///
    /// Kept on the prepared value so later consumers (`inspect --extensions`,
    /// evidence) can report the same static table without rebuilding it.
    #[allow(dead_code)]
    pub registry: CallableRegistry,
    /// Static identities of the declared extensions, already compared with the
    /// lock when one is present (never grants). Consumed by
    /// `inspect --extensions` and evidence.
    #[allow(dead_code)]
    pub extension_identities: Vec<LockedExtension>,
}

/// Prepare one document without executing any project or extension code.
pub fn prepare(
    file: Option<&std::path::Path>,
    options: &PrepareOptions,
) -> Result<PreparedProgram, PrepareError> {
    // T1: static module/package loading. No call diagnostics are produced by
    // the loader; the single binding pass below owns them.
    let mut loaded = loader::load(file.unwrap_or_else(|| std::path::Path::new("index.mq.md")))
        .map_err(|message| default_load_error(&message))?;

    // T2: extension descriptors are read from manifests only. Explicit CLI
    // `--plugin` paths are merged with the project's declared and locked
    // activation sources; CLI paths remain a development form and grant
    // nothing by themselves.
    let mut plugin_paths = options.plugins.clone();
    for extension in &loaded.extension_sources {
        if extension.kind == ExtensionKind::Plugin {
            plugin_paths.push(extension.path.clone());
        }
    }
    plugin_paths.sort();
    plugin_paths.dedup();
    let plugins = read_plugin_descriptors(&plugin_paths)?;

    // T3a: declared extensions must have a locked identity (lock v4). The
    // declaration is activation, not authorization; a mismatched/missing lock
    // is fail-closed rather than silently "latest".
    let extension_identities =
        extension_selection::extension_identities(&loaded.extension_sources)?;
    if !loaded.extension_sources.is_empty() {
        match &loaded.lock {
            Some(lock) => {
                lock.validate_extension_declarations(
                    loaded
                        .manifest
                        .as_ref()
                        .expect("extension sources require a project manifest"),
                )
                .map_err(|message| PrepareError::load("ext.extension_lock_mismatch", message))?;
                extension_selection::validate_against_lock(lock, &extension_identities)?;
            }
            None => {
                return Err(PrepareError::load(
                    "ext.extension_lock_mismatch",
                    format!(
                        "the project declares {} extension(s) but marqdo.lock is missing; run `marqdo lock` first",
                        loaded.extension_sources.len()
                    ),
                ));
            }
        }
    }

    // T3: unique static table. Duplicate exports/builtin shadowing are rejected
    // before any call site can be bound.
    for descriptor in plugins.descriptors() {
        if loaded
            .program
            .external_callable(&descriptor.public_name)
            .is_some()
        {
            return Err(PrepareError::option(
                "ext.duplicate_callable",
                format!(
                    "plugin export `{}` is declared more than once",
                    descriptor.public_name
                ),
            ));
        }
        if marqdo_ir::builtins::find(&descriptor.public_name).is_some() {
            return Err(PrepareError::option(
                "ext.duplicate_callable",
                format!(
                    "plugin export `{}` conflicts with a builtin; plugins may not shadow builtins",
                    descriptor.public_name
                ),
            ));
        }
        loaded.program.external_callables.push(descriptor);
    }
    let mut registry = CallableRegistry::try_from_program(&loaded.program).map_err(|conflict| {
        PrepareError::option(
            "ext.duplicate_callable",
            format!("static callable table conflict: {conflict}"),
        )
    })?;

    // Official stdlib public aliases (ADR-0058) are ordinary Marqdo bindings.
    for alias in &loaded.official_aliases {
        registry
            .insert_official_alias(alias.clone())
            .map_err(|error| {
                PrepareError::option(
                    "ext.duplicate_callable",
                    format!("stdlib public alias: {error}"),
                )
            })?;
    }

    // T4: exactly one call binding pass for the whole program.
    let bound = marqdo_frontend::calls::bind_program_calls(&loaded.program, &registry);
    for (module_id, diagnostic) in bound.diagnostics {
        let Some(file) = loaded.files.get(&module_id) else {
            continue;
        };
        let (path, source) = (file.path.clone(), file.source.clone());
        let duplicate = loaded.diagnostics.iter().any(|existing| {
            existing.module_id == module_id
                && existing.diagnostic.code == diagnostic.code
                && existing.diagnostic.span == diagnostic.span
                && existing.diagnostic.message == diagnostic.message
        });
        if !duplicate {
            loaded.diagnostics.push(FileDiagnostic {
                module_id,
                path,
                source,
                diagnostic,
            });
        }
    }
    loaded.program.set_call_bindings(bound.bindings);

    Ok(PreparedProgram {
        loaded,
        plugins,
        registry,
        extension_identities,
    })
}

fn default_load_error(message: &str) -> PrepareError {
    if message.contains("marqdo.lock") || message.contains("lockfile") {
        PrepareError::load("project.lock", message)
    } else {
        PrepareError::option("project.load", message)
    }
}
