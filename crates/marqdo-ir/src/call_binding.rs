//! Resolved static call bindings (RFC-EXT-0001 §4.2, ADR-0054).
//!
//! The frontend resolves every statically visible call site exactly once and
//! records the decision here. The runtime only dispatches by this table: it
//! never re-resolves a qualified name and never guesses a plugin from a string
//! prefix. The table is internal IR (not part of the Analysis/Observation wire
//! contracts) and is deliberately skipped when a [`crate::Program`] is
//! serialized.
//!
//! Identity rules:
//!
//! * a [`CallSiteKey`] is `(module, function, byte span)` and is therefore
//!   independent of disk paths, registration order and hash-map iteration;
//! * a bound target names the *resolved implementation*, including the plugin
//!   package, complete export name and verified artifact digest for ABI calls;
//! * [`BoundTarget::DynamicValue`] is the only non-static path and may only
//!   call language values (for example a `FunctionRef`); it is never a back
//!   door to an unknown ABI export.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{FunctionId, ModuleId};

/// Stable identity of one static call site.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CallSiteKey {
    /// Module containing the call.
    pub module: ModuleId,
    /// Function containing the call.
    pub function: FunctionId,
    /// First byte offset of the call expression.
    pub span_start: usize,
    /// End byte offset (exclusive) of the call expression.
    pub span_end: usize,
}

impl CallSiteKey {
    /// Construct a key from a function identity and an expression span.
    pub fn new(function: FunctionId, span: marqdo_span::Span) -> Self {
        Self {
            module: function.module.clone(),
            function,
            span_start: span.start,
            span_end: span.end,
        }
    }
}

impl fmt::Display for CallSiteKey {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}@{}..{}",
            self.function, self.span_start, self.span_end
        )
    }
}

/// Resolved implementation target for a call site.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum BoundTarget {
    /// A fixed language builtin, by canonical builtin name.
    Builtin {
        /// Canonical name (for example `foreign.run`), not a user alias.
        canonical_name: String,
    },
    /// A function defined by a Markdown heading.
    Marqdo {
        /// Exact function identity.
        function: FunctionId,
    },
    /// A statically declared ABI plugin export.
    AbiPlugin {
        /// Plugin package that owns the export.
        package: String,
        /// Complete dotted export name.
        export: String,
        /// Verified artifact digest from the selected manifest artifact.
        digest: String,
        /// Declared parameter names in manifest order; used to normalise
        /// positional arguments deterministically for the ABI v1 map wire.
        param_names: Vec<String>,
    },
    /// The callee is a language value resolved at runtime (local variable or
    /// an expression result). Only language callables such as `FunctionRef`
    /// are valid; this path cannot invoke an ABI plugin.
    DynamicValue,
}

impl BoundTarget {
    /// Whether this binding names a statically known implementation.
    pub fn is_static(&self) -> bool {
        !matches!(self, BoundTarget::DynamicValue)
    }
}

/// One deterministic binding table for a prepared program.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CallBindingTable {
    sites: BTreeMap<CallSiteKey, BoundTarget>,
}

impl CallBindingTable {
    /// Insert a binding. Returns `false` when the site was already bound to a
    /// different target; re-binding the same target is idempotent.
    pub fn insert(&mut self, key: CallSiteKey, target: BoundTarget) -> bool {
        match self.sites.get(&key) {
            Some(existing) => existing == &target,
            None => {
                self.sites.insert(key, target);
                true
            }
        }
    }

    /// Look up the bound target for a call site.
    pub fn get(&self, key: &CallSiteKey) -> Option<&BoundTarget> {
        self.sites.get(key)
    }

    /// Number of bound call sites.
    pub fn len(&self) -> usize {
        self.sites.len()
    }

    /// Whether no call site is bound.
    pub fn is_empty(&self) -> bool {
        self.sites.is_empty()
    }

    /// Iterate bindings deterministically (call site order).
    pub fn iter(&self) -> impl Iterator<Item = (&CallSiteKey, &BoundTarget)> {
        self.sites.iter()
    }

    /// Number of statically bound (non-dynamic) call sites.
    pub fn static_count(&self) -> usize {
        self.sites
            .values()
            .filter(|target| target.is_static())
            .count()
    }

    /// Number of dynamically resolved call sites.
    pub fn dynamic_count(&self) -> usize {
        self.sites.len() - self.static_count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use marqdo_span::Span;

    fn function(name: &str) -> FunctionId {
        FunctionId {
            module: ModuleId::root_default("index.mq.md"),
            name: name.to_string(),
        }
    }

    fn key(name: &str, span: Span) -> CallSiteKey {
        CallSiteKey::new(function(name), span)
    }

    #[test]
    fn keys_are_deterministic_and_span_based() {
        let left = key("f", Span::new(10, 20));
        let right = key("f", Span::new(10, 20));
        let other = key("f", Span::new(21, 30));
        assert_eq!(left, right);
        assert_ne!(left, other);
        assert_eq!(left.to_string(), "root:default::index.mq.md::f@10..20");
    }

    #[test]
    fn insert_is_idempotent_but_rejects_conflicting_targets() {
        let mut table = CallBindingTable::default();
        let site = key("f", Span::new(1, 2));
        assert!(table.insert(
            site.clone(),
            BoundTarget::Builtin {
                canonical_name: "长度".into()
            }
        ));
        assert!(table.insert(
            site.clone(),
            BoundTarget::Builtin {
                canonical_name: "长度".into()
            }
        ));
        assert!(!table.insert(site, BoundTarget::DynamicValue));
        assert_eq!(table.len(), 1);
        assert_eq!(table.static_count(), 1);
        assert_eq!(table.dynamic_count(), 0);
    }
}
