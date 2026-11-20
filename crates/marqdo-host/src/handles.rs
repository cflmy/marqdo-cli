//! Host-issued plugin handle table (ADR-0014 / M0.6 audit E15).
//!
//! A `PluginHandle` decoded from wire JSON is only a structural claim. It is
//! never accepted as authority: the host mints handles with an owner package,
//! run id and generation, and validates exact matches before use. A plugin
//! returning a handle value to the host is rejected because plugin output must
//! not mint host authority.

use std::collections::BTreeMap;

use marqdo_value::{ErrorCategory, PluginHandle, RuntimeError};

#[derive(Debug, Clone, PartialEq, Eq)]
struct HandleEntry {
    plugin: String,
    export: String,
    owner_package: String,
    run_id: u64,
    generation: u64,
}

/// Host-owned handle table.
#[derive(Debug, Clone, Default)]
pub struct PluginHandleTable {
    next_id: u64,
    generation: u64,
    live: BTreeMap<u64, HandleEntry>,
}

impl PluginHandleTable {
    /// Mint a handle for one export and owner package in one run.
    pub fn mint(
        &mut self,
        plugin: impl Into<String>,
        export: impl Into<String>,
        owner_package: impl Into<String>,
        run_id: u64,
    ) -> PluginHandle {
        self.next_id += 1;
        self.generation += 1;
        let id = self.next_id;
        self.live.insert(
            id,
            HandleEntry {
                plugin: plugin.into(),
                export: export.into(),
                owner_package: owner_package.into(),
                run_id,
                generation: self.generation,
            },
        );
        PluginHandle {
            plugin: self.live[&id].plugin.clone(),
            export: self.live[&id].export.clone(),
            id,
        }
    }

    /// Validate a handle against its claimed owner and current run.
    pub fn validate(
        &self,
        handle: &PluginHandle,
        owner_package: &str,
        run_id: u64,
    ) -> Result<(), RuntimeError> {
        let Some(entry) = self.live.get(&handle.id) else {
            return Err(rejected("unknown or expired plugin handle"));
        };
        if entry.plugin != handle.plugin || entry.export != handle.export {
            return Err(rejected("plugin handle identity does not match the table"));
        }
        if entry.owner_package != owner_package || entry.run_id != run_id {
            return Err(rejected(
                "plugin handle belongs to another package or run session",
            ));
        }
        if entry.generation == 0 {
            return Err(rejected("plugin handle generation is no longer valid"));
        }
        Ok(())
    }

    /// Invalidate all handles from a run (unload/shutdown).
    pub fn clear_run(&mut self, run_id: u64) {
        self.live.retain(|_, entry| entry.run_id != run_id);
        self.generation += 1;
    }

    /// Number of live handles (testing/diagnostics).
    pub fn len(&self) -> usize {
        self.live.len()
    }

    /// Whether the table is empty.
    pub fn is_empty(&self) -> bool {
        self.live.is_empty()
    }
}

fn rejected(message: &str) -> RuntimeError {
    RuntimeError::new(
        ErrorCategory::PermissionDenied,
        None,
        format!("plugin_handle.untrusted: {message}"),
    )
}

/// Recursively detect a plugin handle in a value returned by an untrusted
/// extension (plugin or foreign kernel).
pub fn contains_plugin_handle(value: &marqdo_value::Value) -> bool {
    match value {
        marqdo_value::Value::PluginHandle(_) => true,
        marqdo_value::Value::List(items) => items.iter().any(contains_plugin_handle),
        marqdo_value::Value::Map(map) => map.values().any(contains_plugin_handle),
        marqdo_value::Value::Table(table) => table
            .rows
            .iter()
            .flat_map(|row| row.iter())
            .any(contains_plugin_handle),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn forged_cross_owner_and_expired_handles_are_rejected() {
        let mut table = PluginHandleTable::default();
        let handle = table.mint("demo.solver", "solver.resource", "demo.solver", 7);
        table
            .validate(&handle, "demo.solver", 7)
            .expect("owner handle");
        assert!(table.validate(&handle, "other.solver", 7).is_err());
        assert!(table.validate(&handle, "demo.solver", 8).is_err());

        let mut forged = handle.clone();
        forged.id = 999;
        assert!(table.validate(&forged, "demo.solver", 7).is_err());
        let mut wrong_export = handle.clone();
        wrong_export.export = "solver.other".into();
        assert!(table.validate(&wrong_export, "demo.solver", 7).is_err());

        table.clear_run(7);
        assert!(table.validate(&handle, "demo.solver", 7).is_err());
        assert_eq!(table.len(), 0);
    }

    #[test]
    fn handles_inside_untrusted_values_are_detected() {
        let mut table = PluginHandleTable::default();
        let handle = table.mint("demo", "demo.resource", "demo", 1);
        let value = marqdo_value::Value::List(vec![marqdo_value::Value::Map(
            [("h".to_string(), marqdo_value::Value::PluginHandle(handle))]
                .into_iter()
                .collect(),
        )]);
        assert!(contains_plugin_handle(&value));
        assert!(!contains_plugin_handle(&marqdo_value::Value::Int(1.into())));
    }
}
