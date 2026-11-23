# Quickstart: install and run

Product: `3.0.0-rc.2` (Linux x86_64 candidate). Install a **release archive**, not
a source checkout, when you only consume Marqdo.

## 1. Install the archive

```bash
sha256sum -c marqdo-3.0.0-rc.2-x86_64-unknown-linux-gnu.tar.gz.sha256
tar -xzf marqdo-3.0.0-rc.2-x86_64-unknown-linux-gnu.tar.gz
cd marqdo-3.0.0-rc.2-x86_64-unknown-linux-gnu
./marqdo --version            # marqdo 3.0.0-rc.2
./marqdo --help
```

Keep `marqdo` and `marqdo-provider-worker` in the **same directory**. The
provider helper is resolved as a sibling; there is no `PATH` fallback, and a
missing helper fails closed with `ext.worker_unavailable`. You can move the whole
directory or add it to `PATH`:

```bash
export PATH="$PWD:$PATH"
marqdo --version
```

## 2. First project

```bash
marqdo init demo
cd demo
marqdo check index.mq.md
marqdo run index.mq.md
marqdo call index.mq.md main
```

`init` writes `marqdo.yaml` (with the enforced
`marqdo: ">=3.0.0-beta.1,<4.0.0"` range), an RFC-LANG-0002 `index.mq.md` and a
`.gitignore`; it does not create a lock, env or git repository.

```markdown
## @exec main

`问候 = "你好，Marqdo"`

`打印(问候)`

`return 问候`
```

`run` executes the entry function (`main` by default) and prints program output;
`call` prints the returned value. Use `--json` for machine-readable
results (`{"command":"run","success":true,...}`), including embedded program
stdout and diagnostics.

## 3. Language in one screen

- An H2 marked `@exec` is a callable function; `@knowledge` sections are inert
  documentation. Unmarked headings inherit the nearest ancestor mode.
- `**strong**`, `*emph*` and inline `` `code` `` are interchangeable carriers of
  one statement each.
- A branch has an `if`, any number of `else if` arms and an optional
  final `else`, followed by one `end`. Each condition is evaluated in order.
- A `while` loop ends at `end`. Inside its body, `break` exits the nearest
  loop and `continue` skips to its next condition check. `return` exits
  the function. See the [runnable example](../../examples/control-flow.mq.md).
- Lists, block quotes and paragraphs organize the source view. For review or
  linting, put each branch head at the start of an ordered list item and
  place the loop head at the start of a bullet item. Nest the body visually
  and close each group with its own `end` carrier.
- The normative contract is
  [`../architecture/RFC-LANG-0002-unified-semantics.md`](../architecture/RFC-LANG-0002-unified-semantics.md),
  [`../../spec/mapping.md`](../../spec/mapping.md) and
  [`../../spec/expression.ebnf`](../../spec/expression.ebnf).

## 4. Machine-readable output

```bash
marqdo check index.mq.md --json
marqdo run index.mq.md --json
marqdo call index.mq.md main --json
```

## 5. Where next

- Publish/consume a pure library: [libraries.md](libraries.md).
- Build an external tool: [tools.md](tools.md).
- Plugins, kernels, providers: [extensions.md](extensions.md).
- Analysis/Trace consumers: [analysis-trace.md](analysis-trace.md).
- Version axes and platform support: [compatibility.md](compatibility.md).
- Beta limitations: [release-limits.md](release-limits.md).
