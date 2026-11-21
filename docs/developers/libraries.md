# Publishing and consuming a pure `.mq.md` library

A pure library is the preferred extension mechanism: it needs no core change,
no C ABI and no rebuild of `marqdo`. It is ordinary Markdown plus executable
sections.

## 1. Repository layout

```text
marqdo-quantum/
  README.md
  marqdo.yaml
  index.mq.md
  tests/
  examples/
```

`marqdo.yaml`:

```yaml
project:
  name: quantum
  version: 0.1.0
  marqdo: ">=3.0.0-beta.1,<4.0.0"   # enforced against the running core
dependencies: {}
```

`index.mq.md`:

```markdown
# @knowledge 量子数学工具

此包在不修改 Marqdo 内核的情况下向外公开领域函数。

## @exec 加倍

`params(x: Int)`

`return x * 2`
```

Every exported function is an Exec H2. Its `params(...)` declaration is the
public contract; keep prose documentation in `@knowledge` sections or
paragraphs, which never execute.

## 2. How a consumer imports it

Pin a **full 40/64-character commit**; branches and tags are not reproducible:

```yaml
# consumer/marqdo.yaml
project:
  name: quantum-consumer
  version: 0.1.0
  marqdo: ">=3.0.0-beta.1,<4.0.0"
dependencies:
  quantum:
    git: https://github.com/your-org/marqdo-quantum.git
    rev: 0123456789abcdef0123456789abcdef01234567
```

```markdown
---
import:
  quantum: "@quantum"
---

## @exec main

`return quantum.加倍(x=21)`
```

An `@alias` import binds the dependency's **entry package** (its
`index.mq.md`); qualify calls with the alias as above. A project-relative
dependency uses `path: ./deps/x` instead of `git`/`rev`. `marqdo add <alias>
--git <url> --rev <sha>` writes the dependency for you.

## 3. Lock and run

From the consumer project root:

```bash
marqdo lock                 # writes marqdo.lock (lock_version: 4)
marqdo sync --locked        # verify cached content and materialize .marqdo/env; no fetch
marqdo check index.mq.md --refs
marqdo call index.mq.md main    # prints 42
```

Rules that matter for reproducibility:

- With declared dependencies, `check`/`run`/`call` require `marqdo.lock` and
  never fetch or rewrite it.
- Plain `marqdo sync` may fetch a pinned Git revision named by the lock;
  `--locked` forbids fetching and fails if the verified cache is unavailable.
  Both modes verify content digests and never silently update the lock.
- A dependency's content digest, manifest digest, instance id
  (`dep:quantum@0.1.0#<digest>`) and alias edges are all recorded. Drift fails
  with a "run `marqdo lock`" diagnostic.
- `project.marqdo` is enforced; a stale `>=0.1,<1` range is a hard
  `package.marqdo_incompatible` error, not a warning.

## 4. Testing your library

Test it with the released binary only:

```bash
MARQDO_BIN=/opt/marqdo/marqdo
$MARQDO_BIN check index.mq.md
$MARQDO_BIN call index.mq.md 加倍 --arg x=21
```

Add a consumer fixture that pins a full SHA and assert the `--json` values and
diagnostics. For negative cases expect hard errors, never prose fallback:

- wrong arity/type → `call.arity` / a type diagnostic;
- unknown function → `symbol.unknown`;
- missing lock with dependencies → a `run marqdo lock` diagnostic;
- edited dependency content with a stale lock → lock/content-drift error.

## 5. Publishing checklist

- README with a minimal runnable example and the exact CLI product range.
- `marqdo.yaml` with an enforced `project.marqdo` range; functions with
  `params(...)`; all examples migrated to RFC-LANG-0002.
- Tests pinned to a full commit SHA; no floating branches.
- State that the package is pure `.mq.md` and needs no plugin/provider/kernel.
- Version the package independently of the CLI (`0.1.0` here is the package
  version, not the product version).
