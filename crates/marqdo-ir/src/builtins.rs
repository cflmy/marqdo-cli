//! Builtin function metadata shared by the frontend (static validation) and the
//! runtime (dispatch). Adding a builtin is a language change: update
//! `spec/evaluation.md` §8 together with this table.

/// One builtin parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BuiltinParam {
    /// Parameter name used in diagnostics.
    pub name: &'static str,
    /// Whether callers must provide it.
    pub required: bool,
}

const fn req(name: &'static str) -> BuiltinParam {
    BuiltinParam {
        name,
        required: true,
    }
}

const fn opt(name: &'static str) -> BuiltinParam {
    BuiltinParam {
        name,
        required: false,
    }
}

/// A builtin function or host operation.
#[derive(Debug, Clone, Copy)]
pub struct Builtin {
    /// Canonical call name (may be namespaced, e.g. `math.eval`).
    pub name: &'static str,
    /// Alternative names (Chinese/English aliases).
    pub aliases: &'static [&'static str],
    /// Parameters in declaration order.
    pub params: &'static [BuiltinParam],
    /// Whether all arguments must be named (inferred-style API) rather than
    /// positional.
    pub named_only: bool,
    /// Whether invoking it has an effect that requires host authorization.
    pub effectful: bool,
}

impl Builtin {
    /// Whether `name` refers to this builtin.
    pub fn matches(&self, name: &str) -> bool {
        self.name == name || self.aliases.contains(&name)
    }
}

const PRINT: Builtin = Builtin {
    name: "打印",
    aliases: &["print"],
    params: &[req("值")],
    named_only: false,
    effectful: true,
};
const LEN: Builtin = Builtin {
    name: "长度",
    aliases: &["len"],
    params: &[req("值")],
    named_only: false,
    effectful: false,
};
const TYPE_OF: Builtin = Builtin {
    name: "类型",
    aliases: &["type"],
    params: &[req("值")],
    named_only: false,
    effectful: false,
};
const STR: Builtin = Builtin {
    name: "文本",
    aliases: &["str"],
    params: &[req("值")],
    named_only: false,
    effectful: false,
};
const INT: Builtin = Builtin {
    name: "整数",
    aliases: &["int"],
    params: &[req("值")],
    named_only: false,
    effectful: false,
};
const FLOAT: Builtin = Builtin {
    name: "小数",
    aliases: &["float"],
    params: &[req("值")],
    named_only: false,
    effectful: false,
};
const TABLE: Builtin = Builtin {
    name: "表",
    aliases: &["table"],
    params: &[req("表头"), req("行")],
    named_only: false,
    effectful: false,
};
const REF: Builtin = Builtin {
    name: "ref",
    aliases: &[],
    params: &[req("目标")],
    named_only: false,
    effectful: false,
};
const MATH_PARSE: Builtin = Builtin {
    name: "math.parse",
    aliases: &[],
    params: &[req("formula")],
    named_only: false,
    effectful: false,
};
const MATH_EVAL: Builtin = Builtin {
    name: "math.eval",
    aliases: &[],
    params: &[req("formula"), req("bindings"), opt("target")],
    named_only: false,
    effectful: false,
};
const MATH_SUBSTITUTE: Builtin = Builtin {
    name: "math.substitute",
    aliases: &[],
    params: &[req("formula"), req("bindings")],
    named_only: false,
    effectful: false,
};
const MATH_SIMPLIFY: Builtin = Builtin {
    name: "math.simplify",
    aliases: &[],
    params: &[req("formula")],
    named_only: false,
    effectful: false,
};
const MATH_DIFFERENTIATE: Builtin = Builtin {
    name: "math.differentiate",
    aliases: &[],
    params: &[req("formula"), req("变量")],
    named_only: false,
    effectful: false,
};
const FOREIGN_RUN: Builtin = Builtin {
    name: "foreign.run",
    aliases: &[],
    params: &[req("ref"), opt("input")],
    named_only: false,
    effectful: true,
};
const FOREIGN_CALL: Builtin = Builtin {
    name: "foreign.call",
    aliases: &[],
    params: &[req("ref"), req("input")],
    named_only: false,
    effectful: true,
};
const FS_READ: Builtin = Builtin {
    name: "fs.read",
    aliases: &[],
    params: &[req("path")],
    named_only: false,
    effectful: true,
};
const FS_WRITE: Builtin = Builtin {
    name: "fs.write",
    aliases: &[],
    params: &[req("path"), req("contents")],
    named_only: false,
    effectful: true,
};

/// All builtins known to the language. Host availability is decided at call
/// time by the capability layer, not here.
pub const BUILTINS: &[Builtin] = &[
    PRINT,
    LEN,
    TYPE_OF,
    STR,
    INT,
    FLOAT,
    TABLE,
    REF,
    MATH_PARSE,
    MATH_EVAL,
    MATH_SUBSTITUTE,
    MATH_SIMPLIFY,
    MATH_DIFFERENTIATE,
    FOREIGN_RUN,
    FOREIGN_CALL,
    FS_READ,
    FS_WRITE,
];

/// Find a builtin by canonical name or alias.
pub fn find(name: &str) -> Option<&'static Builtin> {
    BUILTINS.iter().find(|builtin| builtin.matches(name))
}

/// Validate an argument list against a builtin parameter list.
///
/// Returns `Ok(())` when every required parameter is supplied and no unknown
/// name is used; positional arguments are matched by declaration order.
pub fn validate_args(builtin: &Builtin, args: &[marqdo_expression::Arg]) -> Result<(), String> {
    if builtin.named_only && args.iter().any(|arg| arg.name.is_none()) {
        return Err(format!("`{}` only accepts named arguments", builtin.name));
    }
    let mut provided = Vec::new();
    let mut positional = 0usize;
    for arg in args {
        match &arg.name {
            Some(name) => {
                if !builtin.params.iter().any(|param| param.name == name) {
                    return Err(format!(
                        "`{}` has no parameter named `{name}`",
                        builtin.name
                    ));
                }
                if provided.contains(name) {
                    return Err(format!("duplicate argument `{name}`"));
                }
                provided.push(name.clone());
            }
            None => {
                let Some(param) = builtin.params.get(positional) else {
                    return Err(format!(
                        "`{}` accepts at most {} positional arguments",
                        builtin.name,
                        builtin.params.len()
                    ));
                };
                if provided.contains(&param.name.to_string()) {
                    return Err(format!("duplicate argument `{}`", param.name));
                }
                provided.push(param.name.to_string());
                positional += 1;
            }
        }
    }
    for param in builtin.params {
        if param.required && !provided.iter().any(|name| name == param.name) {
            return Err(format!(
                "`{}` is missing required argument `{}`",
                builtin.name, param.name
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lookup_aliases() {
        assert_eq!(find("print").unwrap().name, "打印");
        assert_eq!(find("打印").unwrap().name, "打印");
        assert!(!find("math.eval").unwrap().named_only);
        assert!(find("nope").is_none());
    }

    #[test]
    fn arity_validation() {
        use marqdo_expression::parse_arguments;
        let print = find("打印").unwrap();
        assert!(validate_args(print, &parse_arguments("(1)").unwrap()).is_ok());
        assert!(validate_args(print, &parse_arguments("()").unwrap()).is_err());
        let eval = find("math.eval").unwrap();
        assert!(validate_args(
            eval,
            &parse_arguments("(formula=ref(\"#x\"), bindings={}, target=\"A\")").unwrap()
        )
        .is_ok());
        // math.eval accepts positional arguments followed by named ones.
        assert!(validate_args(eval, &parse_arguments("(ref(\"#x\"), {})").unwrap()).is_ok());
        assert!(validate_args(eval, &parse_arguments("(formula=1, nope=2)").unwrap()).is_err());
        assert!(validate_args(eval, &parse_arguments("(1, 2, 3, 4)").unwrap()).is_err());
    }
}
