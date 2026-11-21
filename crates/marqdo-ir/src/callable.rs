//! Unified callable descriptors and lookup (ADR-0009).
//!
//! The language keeps builtins closed and functions defined by Markdown
//! headings, but the *resolution* layer is shared by the frontend, the runtime
//! and plugin export registration. A descriptor carries a static signature and
//! an explicit origin; it never grants authority by itself.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};

use crate::{builtins, Function, FunctionId, Module, ModuleId, Param, Program, SignatureKind};
use marqdo_span::Span;

/// Globally unique callable identity.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CallableId(pub String);

impl fmt::Display for CallableId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Where the implementation lives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CallableOrigin {
    /// A fixed language builtin.
    Builtin,
    /// A function defined by a Markdown heading.
    Marqdo,
    /// An export declared by a plugin manifest.
    AbiPlugin {
        /// Plugin package name.
        package: String,
        /// Complete dotted export name.
        export: String,
    },
}

/// Static signature used for checking calls before execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallableSignature {
    /// Parameters in declaration order.
    pub params: Vec<Param>,
    /// Explicit signatures allow positional arguments; inferred do not.
    pub kind: SignatureKind,
}

impl CallableSignature {
    /// Validate an argument list by name/position and arity.
    pub fn validate_args(
        &self,
        args: &[marqdo_expression::Arg],
        display: &str,
    ) -> Result<(), String> {
        let mut provided = Vec::new();
        let mut positional = 0usize;
        for arg in args {
            match &arg.name {
                Some(name) => {
                    if !self.params.iter().any(|param| &param.name == name) {
                        return Err(format!("`{display}` has no parameter named `{name}`"));
                    }
                    if provided.contains(name) {
                        return Err(format!("duplicate argument `{name}`"));
                    }
                    provided.push(name.clone());
                }
                None => {
                    if self.kind != SignatureKind::Explicit {
                        return Err(format!(
                            "`{display}` has an inferred signature and only accepts named arguments"
                        ));
                    }
                    let Some(param) = self.params.get(positional) else {
                        return Err(format!(
                            "`{display}` accepts at most {} positional arguments",
                            self.params.len()
                        ));
                    };
                    if provided.contains(&param.name) {
                        return Err(format!("duplicate argument `{}`", param.name));
                    }
                    provided.push(param.name.clone());
                    positional += 1;
                }
            }
        }
        for param in &self.params {
            if param.default.is_none() && !provided.iter().any(|name| name == &param.name) {
                return Err(format!(
                    "`{display}` is missing required argument `{}`",
                    param.name
                ));
            }
        }
        Ok(())
    }
}

/// One statically visible callable.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CallableDescriptor {
    /// Globally unique id.
    pub id: CallableId,
    /// Name used from the language surface.
    pub public_name: String,
    /// Owning package instance (`core` for builtins).
    pub owner_package: String,
    /// Signature.
    pub signature: CallableSignature,
    /// Conservative effect request summary; not proof of purity.
    #[serde(default)]
    pub effects_requested: Vec<String>,
    /// Implementation origin.
    pub origin: CallableOrigin,
    /// Content digest of the implementation, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub implementation_digest: Option<String>,
    /// Declaration source location, when known.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source_span: Option<Span>,
}

impl CallableDescriptor {
    /// Whether this is a builtin.
    pub fn is_builtin(&self) -> bool {
        matches!(self.origin, CallableOrigin::Builtin)
    }
}

/// Deterministic callable index for one program or module.
#[derive(Debug, Clone, Default)]
pub struct CallableRegistry {
    builtins: BTreeMap<String, CallableDescriptor>,
    functions: BTreeMap<ModuleId, BTreeMap<String, CallableDescriptor>>,
    external: BTreeMap<String, CallableDescriptor>,
    /// Official stdlib public aliases (for example `打印`/`print`) mapped to
    /// the bundled Markdown function that implements them. Only the verified
    /// stdlib loader may populate this table; third-party packages cannot.
    official_aliases: BTreeMap<String, FunctionId>,
}

/// One official bundled-stdlib public alias.
#[derive(Debug, Clone, PartialEq)]
pub struct OfficialAlias {
    /// Public names that resolve to the same target (for example both
    /// `打印` and `print`).
    pub public_names: Vec<String>,
    /// Bundled Marqdo function implementing the alias.
    pub function: FunctionId,
    /// Effective signature; the loader must have checked it against the
    /// verified stdlib metadata before registration.
    pub signature: CallableSignature,
    /// Owner package instance (the verified stdlib instance).
    pub owner_package: String,
    /// Conservative effect request summary for Trace/Analysis.
    pub effects_requested: Vec<String>,
}

/// A deterministic static-table conflict.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CallableConflict {
    /// Colliding public name.
    pub public_name: String,
    /// Human-readable reason; never used as a machine code.
    pub reason: String,
}

impl fmt::Display for CallableConflict {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "`{}`: {}", self.public_name, self.reason)
    }
}

impl CallableRegistry {
    /// Build a registry containing builtins plus every function in `module`.
    pub fn for_module(module: &Module) -> Self {
        let mut registry = Self::default();
        registry.add_builtins();
        registry.add_module(module);
        registry
    }

    /// Build a registry for all modules and declared external descriptors.
    ///
    /// Duplicate external names are rejected rather than silently keeping the
    /// first entry: a static table must never depend on registration order.
    pub fn try_from_program(program: &Program) -> Result<Self, CallableConflict> {
        let mut registry = Self::default();
        registry.add_builtins();
        for module in program.modules.values() {
            registry.add_module(module);
        }
        for descriptor in &program.external_callables {
            registry
                .insert_external(descriptor.clone())
                .map_err(|reason| CallableConflict {
                    public_name: descriptor.public_name.clone(),
                    reason,
                })?;
        }
        Ok(registry)
    }

    /// Build a registry for all modules and declared external descriptors.
    ///
    /// Prefer [`CallableRegistry::try_from_program`] on the CLI composition
    /// path; this compatibility wrapper keeps the first declaration for
    /// already-validated callers (tests and embedders).
    pub fn from_program(program: &Program) -> Self {
        Self::try_from_program(program).unwrap_or_else(|_| {
            let mut registry = Self::default();
            registry.add_builtins();
            for module in program.modules.values() {
                registry.add_module(module);
            }
            for descriptor in &program.external_callables {
                registry
                    .external
                    .entry(descriptor.public_name.clone())
                    .or_insert_with(|| descriptor.clone());
            }
            registry
        })
    }

    /// Look up a builtin by canonical name or alias.
    pub fn builtin(&self, name: &str) -> Option<&CallableDescriptor> {
        self.builtins.get(name)
    }

    /// Look up an ABI export by complete dotted name.
    pub fn external(&self, name: &str) -> Option<&CallableDescriptor> {
        self.external.get(name)
    }

    /// Look up a function in a module by canonical name, then unique local name.
    pub fn function(&self, module: &ModuleId, name: &str) -> Option<&CallableDescriptor> {
        let table = self.functions.get(module)?;
        table.get(name)
    }

    /// Iterate external descriptors deterministically.
    pub fn externals(&self) -> impl Iterator<Item = &CallableDescriptor> {
        self.external.values()
    }

    /// Register one official bundled-stdlib public alias.
    ///
    /// The loader calls this only after verifying the bundled stdlib digest and
    /// the metadata digest, so a third-party package can never claim `打印`,
    /// `print` or any other official public name through this path.
    pub fn insert_official_alias(&mut self, alias: OfficialAlias) -> Result<(), String> {
        if alias.public_names.is_empty() {
            return Err("official alias must declare at least one public name".to_string());
        }
        if alias.function.module.instance.is_empty() || alias.function.name.is_empty() {
            return Err("official alias target function identity must not be empty".to_string());
        }
        for name in &alias.public_names {
            if name.is_empty() {
                return Err("official alias public name must not be empty".to_string());
            }
            if self.external.contains_key(name) {
                return Err(format!(
                    "official alias `{name}` conflicts with a declared plugin export"
                ));
            }
            let descriptor = CallableDescriptor {
                id: CallableId(format!("marqdo:{}", alias.function)),
                public_name: name.clone(),
                owner_package: alias.owner_package.clone(),
                signature: alias.signature.clone(),
                effects_requested: alias.effects_requested.clone(),
                origin: CallableOrigin::Marqdo,
                implementation_digest: None,
                source_span: None,
            };
            // Official aliases intentionally replace the reserved placeholder
            // builtin (for example `打印`); no third-party path can do this.
            self.builtins.insert(name.clone(), descriptor);
            self.official_aliases
                .insert(name.clone(), alias.function.clone());
        }
        Ok(())
    }

    /// Target Marqdo function of an official alias, if `name` is one.
    pub fn alias_target(&self, name: &str) -> Option<&FunctionId> {
        self.official_aliases.get(name)
    }

    /// Iterate official aliases deterministically (public name, target).
    pub fn official_aliases(&self) -> impl Iterator<Item = (&String, &FunctionId)> {
        self.official_aliases.iter()
    }

    /// Whether a dotted builtin namespace starts with `prefix.`.
    pub fn has_builtin_dotted_prefix(&self, prefix: &str) -> bool {
        self.builtins.keys().any(|name| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.'))
        })
    }

    /// Whether a declared plugin export starts with `prefix.`.
    pub fn has_external_dotted_prefix(&self, prefix: &str) -> bool {
        self.external.keys().any(|name| {
            name.strip_prefix(prefix)
                .is_some_and(|rest| rest.starts_with('.'))
        })
    }

    /// Resolve a module function to its exact identity.
    pub fn function_id(&self, module: &ModuleId, name: &str) -> Option<FunctionId> {
        let descriptor = self.function(module, name)?;
        if matches!(descriptor.origin, CallableOrigin::Marqdo) {
            Some(FunctionId {
                module: module.clone(),
                name: descriptor.public_name.clone(),
            })
        } else {
            None
        }
    }

    /// Insert an external descriptor, rejecting duplicate public names.
    pub fn insert_external(&mut self, descriptor: CallableDescriptor) -> Result<(), String> {
        if self.builtins.contains_key(&descriptor.public_name) {
            return Err(format!(
                "external callable `{}` conflicts with a builtin",
                descriptor.public_name
            ));
        }
        if self.external.contains_key(&descriptor.public_name) {
            return Err(format!(
                "external callable `{}` is declared more than once",
                descriptor.public_name
            ));
        }
        self.external
            .insert(descriptor.public_name.clone(), descriptor);
        Ok(())
    }

    fn add_builtins(&mut self) {
        for builtin in builtins::BUILTINS {
            let descriptor = builtin_descriptor(builtin);
            self.builtins
                .insert(builtin.name.to_string(), descriptor.clone());
            for alias in builtin.aliases {
                self.builtins
                    .insert((*alias).to_string(), descriptor.clone());
            }
        }
    }

    fn add_module(&mut self, module: &Module) {
        let mut table = BTreeMap::new();
        for function in &module.functions {
            let descriptor = function_descriptor(module, function);
            table.insert(function.id.name.clone(), descriptor.clone());
        }
        // A local name is only a stable key when it is unique in the module.
        for function in &module.functions {
            let local_matches = module
                .functions
                .iter()
                .filter(|candidate| candidate.local_name == function.local_name)
                .count();
            if local_matches == 1 {
                if let Some(descriptor) = table.get(&function.id.name).cloned() {
                    table
                        .entry(function.local_name.clone())
                        .or_insert(descriptor);
                }
            }
        }
        self.functions.insert(module.id.clone(), table);
    }
}

fn builtin_descriptor(builtin: &builtins::Builtin) -> CallableDescriptor {
    CallableDescriptor {
        id: CallableId(format!("builtin:{}", builtin.name)),
        public_name: builtin.name.to_string(),
        owner_package: "core".to_string(),
        signature: CallableSignature {
            params: builtin
                .params
                .iter()
                .map(|param| Param {
                    name: param.name.to_string(),
                    span: Span::default(),
                    type_annotation: None,
                    default: None,
                })
                .collect(),
            kind: SignatureKind::Explicit,
        },
        effects_requested: if builtin.effectful {
            vec!["host".to_string()]
        } else {
            Vec::new()
        },
        origin: CallableOrigin::Builtin,
        implementation_digest: None,
        source_span: None,
    }
}

fn function_descriptor(module: &Module, function: &Function) -> CallableDescriptor {
    CallableDescriptor {
        id: CallableId(format!("marqdo:{}", function.id)),
        public_name: function.id.name.clone(),
        owner_package: module.id.instance.clone(),
        signature: CallableSignature {
            params: function.params.clone(),
            kind: function.signature,
        },
        effects_requested: vec!["unknown".to_string()],
        origin: CallableOrigin::Marqdo,
        implementation_digest: None,
        source_span: Some(function.heading_span),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Module;

    fn empty_module(id: &str) -> Module {
        Module {
            id: ModuleId::from(id),
            units: Vec::new(),
            functions: Vec::new(),
            imports: Vec::new(),
            resources: Vec::new(),
            anchors: BTreeMap::new(),
        }
    }

    #[test]
    fn builtins_are_registered_with_aliases() {
        let registry = CallableRegistry::for_module(&empty_module("index.mq.md"));
        assert_eq!(registry.builtin("print").unwrap().id.0, "builtin:打印");
        assert_eq!(registry.builtin("打印").unwrap().id.0, "builtin:打印");
        assert!(registry.builtin("nope").is_none());
        assert!(registry
            .builtin("print")
            .unwrap()
            .effects_requested
            .contains(&"host".to_string()));
    }

    #[test]
    fn external_names_must_be_unique_and_not_shadow_builtins() {
        let mut registry = CallableRegistry::for_module(&empty_module("index.mq.md"));
        let descriptor = CallableDescriptor {
            id: CallableId("plugin:demo:solver.add".into()),
            public_name: "solver.add".into(),
            owner_package: "demo".into(),
            signature: CallableSignature {
                params: Vec::new(),
                kind: SignatureKind::Explicit,
            },
            effects_requested: Vec::new(),
            origin: CallableOrigin::AbiPlugin {
                package: "demo".into(),
                export: "solver.add".into(),
            },
            implementation_digest: None,
            source_span: None,
        };
        registry.insert_external(descriptor.clone()).unwrap();
        assert!(registry.insert_external(descriptor).is_err());
        let mut shadow = CallableDescriptor {
            id: CallableId("plugin:demo:print".into()),
            public_name: "print".into(),
            owner_package: "demo".into(),
            signature: CallableSignature {
                params: Vec::new(),
                kind: SignatureKind::Explicit,
            },
            effects_requested: Vec::new(),
            origin: CallableOrigin::AbiPlugin {
                package: "demo".into(),
                export: "print".into(),
            },
            implementation_digest: None,
            source_span: None,
        };
        shadow.public_name = "print".into();
        assert!(registry.insert_external(shadow).is_err());
    }
}
