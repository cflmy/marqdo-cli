# RFC-CM CM-A06 evidence: minimal Resource/Stream ABI

Date: 2026-11-20 (Asia/Shanghai)
Repository: `marqdo-cli`
Baseline: `a113d89` plus the uncommitted RFC-CM P0–P3 changes.
Decision record: [ADR-0025](../adr/0025-minimal-resource-stream-abi.md)
Related: [P1](2026-11-20-memory-p1.md),
[P2 result/allocator](2026-11-20-result-and-free-list-p2.md),
[STATUS §5](../STATUS.md).
Scope: **first CM-A06 slice only**. Not the full P3 migration.

## Environment and gate

Ubuntu 26.04 development container, Linux x86_64, Rust 1.100.0.

```text
cargo fmt --all -- --check                            PASS
cargo clippy --workspace --all-targets -- -D warnings PASS
cargo test --workspace --locked                       PASS
  402 passed, 0 failed, 2 ignored (existing plugin-sdk doc-tests)
```

No GitHub CI is available from this checkout; this is local evidence.

## Implemented slice

### Value and wire

* `Value::Resource(ResourceHandle)` is an opaque host-issued handle; fields are
  private, the constructor is host-only, and `display`/`repr` render only
  `Resource`.
* `to_wire(Resource)` emits only `{"type":"resource"}`; `from_wire` rejects the
  tag. A `Resource` inside a list/map/table is equally rejected on decode.
* The generic `Host::invoke` boundary rejects resources before dispatch, like
  `Memory`. Plugins/foreign kernels never receive either handle kind.

### Builtins

```text
resource.open(kind, descriptor, rights) -> Resource
resource.read(handle, memory, offset, max_bytes) -> Result {count, eof}
resource.write(handle, memory, offset, length) -> Result {count}
resource.close(handle) -> Result
```

* `read` validates the destination region **before** reading from the provider
  and caps the request to the remaining bytes.
* EOF is `{ok:true, value:{count:0, eof:true}}`, not an error.
* Closed handles return `{ok:false, code:"resource.closed"}`; forged/cross-run/
  stale handles are fatal `runtime.resource`; unsupported kind/rights are
  `resource.unsupported`; missing grant is `permission.denied`.
* Bytes move only as copied `Memory` ranges for the duration of the call. No
  host pointer is stored in a region.

### Providers in this slice

| Kind | Descriptor | Rights | Mode | Behavior |
|---|---|---|---|---|
| `stdio` | `stdout` | `write` | pure/project | appends to the program output buffer after the output-budget check |
| `file` | project-relative path | `read` | project | bounded regular file loaded at open; chunked reads with EOF |
| anything else | — | — | — | explicit `resource.unsupported` |

* `strict` refuses every `resource.open` with `permission.denied` (CM-A11
  fail-closed); no provider is silently enabled.
* `pure` allows only stdio stdout (console grant), exactly the existing
  console exception; file resources are refused by an explicit mode check,
  even if a caller hand-constructs a matching `fs_read` grant.
* File paths still go through `ProjectFs::resolve` (absolute paths, `..`,
  symlink escapes rejected) and require an `fs_read` glob grant.
* File size is bounded by the existing 8 MiB read limit.
* `resource.write` to stdout charges the existing output budget; a budget
  failure produces no output. There is no separate I/O-byte budget yet.

## Tests

### Host integration (`crates/marqdo-host/tests/resource_streams.rs`, 7)

```text
pure_mode_writes_stdout_through_a_resource
stdout_write_obeys_the_output_budget_atomically
strict_mode_refuses_every_resource_open
resource_handles_cannot_cross_the_generic_host_boundary
pure_mode_refuses_file_resources_even_with_a_grant
project_file_reads_are_chunked_and_require_a_grant
traversal_and_unsupported_rights_are_refused
```

### Host table unit tests (`crates/marqdo-host/src/resource.rs`, 3)

```text
file_reads_are_chunked_with_eof
forged_and_closed_handles_are_rejected
stdout_is_not_readable_and_file_is_not_writable
```

### CLI integration (`crates/cli/tests/resource_streams.rs`, 2)

```text
project_file_stream_reads_chunks_into_memory
  data.txt = "abcdef"
  run index.mq.md --allow-read data.txt --json
  results: counts 2/4/0, eof false/true/true, bytes 97..102
  resource.close => ok:true

file_stream_requires_a_read_grant
  same document without --allow-read
  exit 1, error.code = permission.denied, no bytes read
```

### Semantic fixture 66

`tests/conformance/semantics/66-resource-stdout-write.mq.md`:

* `.mq.md` UTF-8 encoder writes `"你好，资源！\n"` into `Memory`;
* `resource.open("stdio","stdout",["write"])`;
* `resource.write` reports count 19;
* `resource.close` succeeds;
* a second `resource.read` on the closed handle returns
  `{ok:false,code:"resource.closed"}`;
* captured program stdout is exactly `你好，资源！\n`.

## What this does not prove

* No file write, directory listing, rename, metadata, network, DNS, TLS,
  process, clock, random or device resources.
* No generic third-party provider registry (CM-A09) and no Worker/ABI byte
  transfer.
* No stdlib `print` migration (CM-A07); `打印` still uses `HostOp::Print`.
* No strict-mode stream support. Strict L1 is unchanged and refuses opens.
* No async/nonblocking/seek/zero-copy and no per-stream deadline.
* No Windows/macOS native lane and no remote CI URL.
* Host-in-process I/O is not an OS sandbox; project mode remains cooperative.

## Reproduce

```bash
export PATH=/root/.cargo/bin:$PATH
export CARGO_HOME=/root/work/marqdo-cli/.cache/cargo
export CARGO_TARGET_DIR=/root/work/marqdo-cli/.cache/target
cargo test -p marqdo-host
cargo test -p marqdo-cli --test resource_streams
cargo test -p marqdo-cli --test conformance_semantics
cargo test --workspace --locked
```
