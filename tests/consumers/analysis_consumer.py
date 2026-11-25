#!/usr/bin/env python3
"""Independent semantic consumer for `marqdo.analysis`.

This script imports only the Python standard library. It validates the public
invariants that JSON Schema cannot express: source digests, span ownership,
heading trees, mode origins, function/anchor/call ownership, reference state
consistency and resource attribution. It never links a private Rust crate and
never executes a document.

usage: analysis_consumer.py ANALYSIS.json [--require-transfers]
"""
import hashlib
import json
import re
import sys

SCHEMA = "marqdo.analysis"
SCHEMA_VERSION = 3
STATES = {
    "resolved",
    "not_loaded",
    "missing",
    "ambiguous",
    "external",
    "outside_scope",
    "invalid",
}
DIGEST64 = re.compile(r"^[0-9a-f]{64}$")
STATEMENT_KINDS = {
    "assign",
    "expression",
    "branch",
    "loop",
    "return",
    "break",
    "continue",
}
EXPRESSION_KINDS = {
    "literal",
    "variable",
    "list",
    "record",
    "unary",
    "binary",
    "member",
    "index",
    "call",
}


class InvalidAnalysis(Exception):
    """The artifact violates the current public contract."""


def fail(path, message):
    raise InvalidAnalysis(f"{path}: {message}")


def validate(analysis):
    """Validate a parsed Analysis document; raise InvalidAnalysis on failure."""
    if not isinstance(analysis, dict):
        fail("$", "analysis must be an object")
    if analysis.get("schema") != SCHEMA:
        fail("$.schema", "unexpected schema")
    if analysis.get("schema_version") != SCHEMA_VERSION:
        fail("$.schema_version", "unsupported revision")
    producer = analysis.get("producer") or {}
    if producer.get("name") != "marqdo":
        fail("$.producer.name", "unexpected producer")
    if producer.get("markdown_profile") != "marqdo.md.v1":
        fail("$.producer.markdown_profile", "unexpected Markdown profile")

    project = analysis.get("project") or {}
    modules = analysis.get("modules")
    if not isinstance(modules, list) or not modules:
        fail("$.modules", "at least one module is required")
    by_id = {}
    for index, module in enumerate(modules):
        path = f"$.modules[{index}]"
        module_id = module.get("id")
        if not isinstance(module_id, str) or "::" not in module_id:
            fail(path + ".id", "invalid module id")
        if module_id in by_id:
            fail(path + ".id", "duplicate module id")
        by_id[module_id] = module
    if project.get("entry_module_id") not in by_id:
        fail("$.project.entry_module_id", "does not name a module")
    if project.get("identity_kind") not in ("manifest", "workspace_snapshot"):
        fail("$.project.identity_kind", "invalid identity kind")
    for field in ("lock_digest", "manifest_digest"):
        value = project.get(field)
        if not isinstance(value, str) or not (
            value == "" or re.fullmatch(r"sha256:[0-9a-f]{64}", value)
        ):
            fail(f"$.project.{field}", "invalid snapshot digest")
    if project.get("root_id") not in (None, "") and not isinstance(
        project["root_id"], str
    ):
        fail("$.project.root_id", "invalid root id")
    extensions = project.get("extensions")
    if not isinstance(extensions, list):
        fail("$.project.extensions", "must be an array")
    for index, extension in enumerate(extensions):
        path = f"$.project.extensions[{index}]"
        if extension.get("kind") not in ("plugin", "kernel", "provider"):
            fail(path + ".kind", "invalid extension kind")
        for field in ("manifest_digest", "artifact_digest"):
            if not re.fullmatch(r"sha256:[0-9a-f]{64}", extension.get(field, "")):
                fail(f"{path}.{field}", "invalid extension digest")

    function_ids = set()
    module_units = {}
    for index, module in enumerate(modules):
        path = f"$.modules[{index}]"
        unit_ids = {unit.get("id") for unit in module.get("units", [])}
        module_units[module["id"]] = unit_ids
        function_ids.update(
            function.get("id") for function in module.get("functions", [])
        )

    for index, module in enumerate(modules):
        path = f"$.modules[{index}]"
        source = module.get("source")
        if not isinstance(source, str):
            fail(path + ".source", "exact source must be a string")
        encoded = source.encode("utf-8")
        digest = module.get("source_digest")
        if digest != hashlib.sha256(encoded).hexdigest():
            fail(path + ".source_digest", "does not match source bytes")
        digest = module.get("document", {}).get("span")
        if not isinstance(module.get("document"), dict):
            fail(path + ".document", "missing document AST")
        ast_index = {}
        validate_ast(module["document"], encoded, path + ".document", ast_index)
        document_span = module["document"].get("span") or {}
        if document_span.get("start") != 0 or document_span.get("end") != len(encoded):
            fail(path + ".document.span", "root does not cover the source")

        units = module.get("units") or []
        if not units:
            fail(path + ".units", "missing root unit")
        unit_by_id = {}
        for unit in units:
            if unit.get("id") in unit_by_id:
                fail(path + ".units", "duplicate unit id")
            unit_by_id[unit["id"]] = unit
        roots = [unit for unit in units if unit.get("kind") == "root"]
        if len(roots) != 1:
            fail(path + ".units", "expected exactly one root unit")
        root = roots[0]
        if root.get("parent_id") is not None or root.get("path") != []:
            fail(path + ".units.root", "root must not have a parent or path")
        if root.get("heading_span") is not None or root.get("level") != 0:
            fail(path + ".units.root", "root must not carry a heading")
        for unit in units:
            upath = f"{path}.units[{unit['id']}]"
            check_span(unit.get("span"), encoded, upath + ".span")
            if unit.get("heading_span") is not None:
                check_span(unit.get("heading_span"), encoded, upath + ".heading_span")
            if unit is root:
                continue
            parent = unit_by_id.get(unit.get("parent_id"))
            if parent is None:
                fail(upath + ".parent_id", "missing parent unit")
            expected = list(parent.get("path") or []) + [unit.get("name")]
            if expected != unit.get("path"):
                fail(upath + ".path", "does not match parent path plus name")
            unit_span = unit.get("span") or {}
            parent_span = parent.get("span") or {}
            if not (
                parent_span.get("start", 0) <= unit_span.get("start", 0)
                and unit_span.get("end", -1) <= parent_span.get("end", -2)
            ):
                fail(upath + ".span", "escapes the parent span")
            if unit.get("mode") not in ("exec", "knowledge"):
                fail(upath + ".mode", "invalid mode")
            if unit.get("mode_origin") not in ("explicit", "inherited"):
                fail(upath + ".mode_origin", "non-root unit must be explicit or inherited")
            if not in_ancestor_chain(unit, unit.get("mode_source_unit_id"), unit_by_id):
                fail(upath + ".mode_source_unit_id", "is not an ancestor")
            function = unit.get("function")
            if function is not None:
                if unit.get("kind") not in ("function", "section"):
                    fail(upath + ".function", "only Exec H2/section units own a function")
                if function not in function_ids:
                    fail(upath + ".function", "names a missing function")
        if root.get("mode_origin") != "default":
            fail(path + ".units.root.mode_origin", "root origin must be default")

        anchors = module.get("anchors") or []
        previous = None
        for anchor in anchors:
            if previous is not None and previous >= anchor.get("anchor"):
                fail(path + ".anchors", "not strictly sorted")
            previous = anchor.get("anchor")
            if anchor.get("unit_id") not in unit_by_id:
                fail(path + ".anchors", "names a missing unit")

        seen_functions = set()
        for function in module.get("functions") or []:
            fpath = f"{path}.functions[{function.get('id')}]"
            if function.get("id") in seen_functions:
                fail(fpath, "duplicate function")
            seen_functions.add(function.get("id"))
            unit = unit_by_id.get(function.get("unit_id"))
            if unit is None or unit.get("kind") != "function":
                fail(fpath + ".unit_id", "not an Exec H2 unit")
            if unit.get("function") != function.get("id"):
                fail(fpath + ".id", "unit/function link mismatch")
            check_span(function.get("span"), encoded, fpath + ".span")
            check_span(function.get("heading_span"), encoded, fpath + ".heading_span")
            for statement in function.get("body") or []:
                validate_statement(statement, encoded, fpath)

        for import_record in module.get("imports") or []:
            check_span(import_record.get("span"), encoded, f"{path}.imports")
            resolved = import_record.get("resolved")
            if resolved is not None and resolved not in by_id:
                fail(path + ".imports", "resolved target is not a module")

        for reference in module.get("references") or []:
            validate_reference(reference, by_id, module_units, encoded, path)

        seen_call_sites = set()
        for call in module.get("calls") or []:
            cpath = f"{path}.calls"
            check_span(call.get("span"), encoded, cpath)
            key = (call.get("function"), call["span"]["start"], call["span"]["end"])
            if key in seen_call_sites:
                fail(cpath, "duplicate call site")
            seen_call_sites.add(key)
            if call.get("function") not in function_ids:
                fail(cpath, "missing caller function")
            state = call.get("state")
            if state == "function":
                if call.get("target_kind") != "function":
                    fail(cpath, "function call has wrong target kind")
                if call.get("target") not in function_ids:
                    fail(cpath, "function target does not exist")
            elif state == "builtin":
                if call.get("target_kind") != "builtin" or not call.get("target"):
                    fail(cpath, "builtin call is incomplete")
            elif state == "plugin":
                if call.get("target_kind") != "plugin" or not call.get("target_owner"):
                    fail(cpath, "plugin call lacks its owner")
            elif state == "unresolved":
                if call.get("target") is not None or call.get("target_kind") is not None:
                    fail(cpath, "unresolved call carries a target")
                if call.get("reason") not in ("dynamic", "unbound"):
                    fail(cpath, "unresolved call lacks a reason")
            else:
                fail(cpath, "invalid call state")

        seen_resources = set()
        for resource in module.get("resources") or []:
            rpath = f"{path}.resources"
            if resource.get("id") in seen_resources:
                fail(rpath, "duplicate resource id")
            seen_resources.add(resource.get("id"))
            if resource.get("module_id") != module["id"]:
                fail(rpath, "resource module mismatch")
            if resource.get("unit_id") not in unit_by_id:
                fail(rpath, "resource names a missing unit")
            check_span(resource.get("span"), encoded, rpath)
            if resource.get("kind") not in ("formula", "code"):
                fail(rpath, "unknown resource kind")
            if not DIGEST64.fullmatch(resource.get("content_digest", "")):
                fail(rpath, "invalid resource digest")
            if resource["kind"] == "formula" and not isinstance(
                resource.get("display"), bool
            ):
                fail(rpath, "formula display must be a bool")
            if resource["kind"] == "code" and resource.get("display") is not None:
                fail(rpath, "code display must be null")
            node = ast_index.get(
                (resource["span"]["start"], resource["span"]["end"])
            )
            if node is None:
                fail(rpath, "resource span does not name an AST payload node")
            expected_kinds = (
                ("math", "math_block")
                if resource["kind"] == "formula"
                else ("code_block",)
            )
            if node.get("kind") not in expected_kinds:
                fail(rpath, "resource kind does not match its AST node")
            payload = node.get("literal", "").encode("utf-8")
            if resource["content_digest"] != hashlib.sha256(payload).hexdigest():
                fail(rpath, "content_digest does not match the AST payload")

    diagnostics = analysis.get("diagnostics")
    if not isinstance(diagnostics, list):
        fail("$.diagnostics", "must be an array")
    for diagnostic in diagnostics:
        if diagnostic.get("module") not in by_id:
            fail("$.diagnostics", "names a missing module")
        if diagnostic.get("severity") == "error":
            fail("$.diagnostics", "successful Analysis contains an error")
        span = diagnostic.get("span")
        if span is not None:
            module = by_id[diagnostic["module"]]
            check_span(span, module["source"].encode("utf-8"), "$.diagnostics.span")
    return analysis


def check_span(span, encoded, path):
    if not isinstance(span, dict):
        fail(path, "missing span")
    start, end = span.get("start"), span.get("end")
    if not isinstance(start, int) or not isinstance(end, int):
        fail(path, "span endpoints must be integers")
    if start < 0 or start > end or end > len(encoded):
        fail(path, "span is outside the source")
    for offset in (start, end):
        try:
            encoded[offset:].decode("utf-8")
        except UnicodeDecodeError:
            fail(path, "span is not on a UTF-8 boundary")


def in_ancestor_chain(unit, target, unit_by_id):
    current = unit
    while current is not None:
        if current.get("id") == target:
            return True
        parent = current.get("parent_id")
        current = unit_by_id.get(parent) if parent else None
    return False


def validate_ast(node, encoded, path, ast_index=None):
    if not isinstance(node, dict):
        fail(path, "AST node must be an object")
    kind = node.get("kind")
    if not isinstance(kind, str):
        fail(path + ".kind", "missing node kind")
    if kind.startswith("footnote"):
        fail(path + ".kind", "disabled profile node")
    check_span(node.get("span"), encoded, path + ".span")
    children = node.get("children")
    if not isinstance(children, list):
        fail(path + ".children", "must be an array")
    if kind in {
        "text",
        "code",
        "code_block",
        "math",
        "math_block",
        "html_block",
        "html_inline",
        "front_matter",
        "thematic_break",
        "soft_break",
        "line_break",
    } and children:
        fail(path + ".children", "leaf node has children")
    if ast_index is not None and kind in ("code_block", "math", "math_block"):
        span = node.get("span") or {}
        ast_index[(span.get("start"), span.get("end"))] = node
    for index, child in enumerate(children):
        validate_ast(child, encoded, f"{path}.children[{index}]", ast_index)


def validate_statement(statement, encoded, path):
    if not isinstance(statement, dict):
        fail(path, "statement must be an object")
    kind = statement.get("kind")
    if kind not in STATEMENT_KINDS:
        fail(path + ".kind", "unknown statement kind")
    check_span(statement.get("span"), encoded, path + ".span")
    if kind == "assign":
        validate_place(statement.get("target"), encoded, path + ".target")
        validate_expression(statement.get("value"), encoded, path + ".value")
    elif kind == "expression":
        validate_expression(statement.get("value"), encoded, path + ".value")
    elif kind == "return":
        validate_expression(statement.get("value"), encoded, path + ".value")
    elif kind == "branch":
        arms = statement.get("arms")
        if not isinstance(arms, list) or not arms:
            fail(path + ".arms", "branch needs at least one arm")
        for arm in arms:
            validate_expression(arm.get("condition"), encoded, path + ".condition")
            check_span(arm.get("condition_span"), encoded, path + ".condition_span")
            for nested in arm.get("body") or []:
                validate_statement(nested, encoded, path + ".arm")
    elif kind == "loop":
        validate_expression(statement.get("condition"), encoded, path + ".condition")
        check_span(statement.get("condition_span"), encoded, path + ".condition_span")
        for nested in statement.get("body") or []:
            validate_statement(nested, encoded, path + ".body")
    elif kind in ("break", "continue"):
        pass


def validate_place(place, encoded, path):
    if isinstance(place, str):
        if not place:
            fail(path, "empty local target")
        return
    if not isinstance(place, dict) or place.get("kind") != "index":
        fail(path, "invalid assignment target")
    validate_expression(place.get("base"), encoded, path + ".base")
    validate_expression(place.get("index"), encoded, path + ".index")
    check_span(place.get("span"), encoded, path + ".span")


def validate_expression(expression, encoded, path):
    if not isinstance(expression, dict):
        fail(path, "expression must be an object")
    kind = expression.get("kind")
    if kind not in EXPRESSION_KINDS:
        fail(path + ".kind", "unknown expression kind")
    check_span(expression.get("span"), encoded, path + ".span")
    if kind == "literal":
        literal_type = expression.get("type")
        if literal_type not in ("int", "float", "text", "bool", "none"):
            fail(path + ".type", "unknown literal type")
        if literal_type == "int":
            if not re.fullmatch(r"-?[0-9]+", expression.get("value", "")):
                fail(path + ".value", "integer literal must be a decimal string")
        elif literal_type == "none":
            if "value" in expression:
                fail(path + ".value", "none literal carries a value")
    elif kind == "variable":
        if not expression.get("name"):
            fail(path + ".name", "empty variable name")
    elif kind == "list":
        for item in expression.get("items") or []:
            validate_expression(item, encoded, path + ".items")
    elif kind == "record":
        for entry in expression.get("entries") or []:
            validate_expression(entry.get("value"), encoded, path + ".entry")
    elif kind == "unary":
        validate_expression(expression.get("operand"), encoded, path + ".operand")
    elif kind in ("binary",):
        validate_expression(expression.get("left"), encoded, path + ".left")
        validate_expression(expression.get("right"), encoded, path + ".right")
    elif kind == "member":
        validate_expression(expression.get("base"), encoded, path + ".base")
    elif kind == "index":
        validate_expression(expression.get("base"), encoded, path + ".base")
        validate_expression(expression.get("index"), encoded, path + ".index")
    elif kind == "call":
        validate_expression(expression.get("callee"), encoded, path + ".callee")
        for argument in expression.get("args") or []:
            validate_expression(argument.get("value"), encoded, path + ".arg")


def validate_reference(reference, by_id, module_units, encoded, path):
    rpath = f"{path}.references"
    check_span(reference.get("span"), encoded, rpath + ".span")
    if reference.get("source_unit") not in module_units.get(reference.get("source"), set()):
        fail(rpath + ".source_unit", "not a unit of the source module")
    state = reference.get("state")
    if state not in STATES:
        fail(rpath + ".state", "unknown state")
    target_module = reference.get("target_module")
    target_unit = reference.get("target_unit")
    target_kind = reference.get("target_kind")
    if state == "resolved":
        if target_kind == "module":
            if target_module not in by_id:
                fail(rpath + ".target_module", "does not name a module")
            if target_unit is not None and target_unit not in module_units.get(
                target_module, set()
            ):
                fail(rpath + ".target_unit", "does not name a target unit")
        elif target_kind == "file":
            if target_module is not None or target_unit is not None:
                fail(rpath, "file target must not name a module/unit")
        else:
            fail(rpath + ".target_kind", "resolved reference has no target kind")
    elif state == "missing":
        if target_unit is not None:
            fail(rpath + ".target_unit", "missing reference carries a unit")
    elif state == "ambiguous":
        if target_unit is not None or len(reference.get("candidates") or []) < 2:
            fail(rpath, "ambiguous reference needs several candidates")
    else:
        if target_module is not None or target_unit is not None or target_kind is not None:
            fail(rpath, "non-resolved reference carries a target")


def visit_transfers(body, loop_span=None):
    for statement in body:
        kind = statement["kind"]
        if kind == "loop":
            yield from visit_transfers(statement["body"], statement["span"])
        elif kind == "branch":
            for arm in statement["arms"]:
                yield from visit_transfers(arm["body"], loop_span)
        elif kind in ("break", "continue"):
            if loop_span is None:
                fail("$", f"{kind} outside a projected loop")
            span = statement["span"]
            if not 0 <= loop_span["start"] < span["start"] < span["end"]:
                fail("$", f"invalid {kind} carrier span")
            yield kind, loop_span["start"]


def main(argv):
    if len(argv) not in (2, 3) or (len(argv) == 3 and argv[2] != "--require-transfers"):
        raise SystemExit(
            "usage: analysis_consumer.py ANALYSIS.json [--require-transfers]"
        )
    with open(argv[1], encoding="utf-8") as handle:
        analysis = json.load(handle)
    validate(analysis)
    transfers = []
    functions = 0
    for module in analysis["modules"]:
        for function in module["functions"]:
            functions += 1
            transfers.extend(visit_transfers(function["body"]))
    if len(argv) == 3 and not transfers:
        raise SystemExit("missing loop transfers")
    print(f"functions={functions}")
    print(f"transfers={len(transfers)}")


if __name__ == "__main__":
    main(sys.argv)
