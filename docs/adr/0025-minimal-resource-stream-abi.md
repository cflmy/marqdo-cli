# ADR-0025: Minimal Resource/Stream ABI and byte transfer through `Memory` (RFC-CM CM-A06)

Status: **Accepted**
Date: 2026-11-20
Relates to: `docs/MARQDO_COMPUTE_MACHINE_RFC_0001.md`,
`docs/MARQDO_COMPUTE_CORE_STDLIB_MIGRATION_RFC_0002.md` (CM-A06, CM-A09,
CM-A11, P3), ADR-0022, ADR-0024
Specs: `spec/evaluation.md`, `spec/security.md`, `spec/value-wire.md`
Evidence: `docs/evidence/2026-11-20-resource-stream-p3.md`

## Context

P1/P2 give `.mq.md` managed values, addressable `Memory`, structure primitives,
a recoverable `Result` convention and a substantial set of in-language
algorithms. Nothing yet lets a standard-library function perform a real byte
transfer to the outside world without a dedicated `HostOp` per high-level
function.

CM-A06 asks for a stable, authorized, versioned resource boundary: obtain a
granted resource, call the operations it promises, release it. CM-A09 says to
extend the existing host/ABI rather than build a second plugin system. CM-A11
says strict must stay fail-closed until a provider is actually verified.

This ADR freezes a **first, deliberately small slice**:

* one opaque host-issued `ResourceHandle` value;
* `resource.open/read/write/close` as pure language builtins, not new `HostOp`
  variants;
* byte transfer through `Memory`, never through host pointers;
* stdio `stdout` write and project-file **read** only;
* `Result` maps for EOF/closed/write counts, fatal errors for invalid handle
  or capability denial.

File write, directories, network, processes, TLS, asynchronous I/O and a
generic provider registry remain open.

## Decision

### 1. Opaque handle

`Value::ResourceHandle(ResourceHandle)` is a host-issued opaque identity with
private `(run_id, resource_id, generation)` fields and a hidden constructor.
It is not `Serialize`/`Deserialize`, is rejected by the wire, is never
synthesized from a number or a JSON value, and renders as `Resource` in
`display`/`repr` without exposing the id.

It is a **separate domain** from `Memory`: a handle is not indexable, not
byte-addressable and not a capability token the user can manufacture.

### 2. First resource kinds

| Kind | Descriptor | Rights | Availability |
|---|---|---|---|
| `stdio` | `stdout` | `write` | `pure` and `project` when the console grant is present; `strict` rejects until verified |
| `file` | project-relative path | `read` | `project` with an `fs_read` grant |

`file` streams are opened by reading a bounded regular file (existing
`ProjectFs::MAX_READ_BYTES = 8 MiB` limit) into host-owned bytes; `read`
returns successive chunks and `eof` when the position reaches the end. This is
honest about being a first provider: it does not expose a host file descriptor,
does not support seeking, writing, directories, network, processes, devices or
paths outside the project root.

Any other kind/descriptor/right combination is an explicit
`resource.unsupported`/`permission.denied` diagnostic, never a silent fallback.

### 3. Builtins

```text
resource.open(kind: Text, descriptor: Text, rights: List) -> Resource
resource.read(handle: Resource, memory: Memory, offset: Int, max_bytes: Int)
    -> {ok:true, value:{count:Int, eof:Bool}} | {ok:false, code, message}
resource.write(handle: Resource, memory: Memory, offset: Int, length: Int)
    -> {ok:true, value:{count:Int}} | {ok:false, code, message}
resource.close(handle: Resource)
    -> {ok:true, value:None} | {ok:false, code, message}
```

* `resource.open` is effectful and capability-checked at execution time.
* `resource.read` validates `offset` and the remaining buffer **before** asking
  the provider, so a failed range check cannot consume stream data. It caps the
  request to the remaining region, writes the returned bytes into `Memory` and
  returns the actual count and EOF flag. EOF is a **success** with `eof:true`.
* `resource.write` copies bytes out of `Memory` only for the duration of the
  call. A short write is reported as a count; the caller loops. It never
  stores a host pointer into a region that could later grow.
* `resource.close` closes the host slot. Reading/writing/closing an already
  closed handle returns `{ok:false, code:"resource.closed", ...}` (recoverable).
  A forged, cross-run or stale handle is `runtime.resource` (fatal).
* All four are pure interpreter operations over host trait methods; they add no
  `HostOp` variant. Invalid capacity/bounds remains `runtime.index`; wrong
  operand types remain `runtime.type`.

### 4. Authorization and modes

* Authorization is the intersection of the requested operation, the effective
  `Capabilities` grant and the mode:
  * `stdio/stdout/write` requires `console` (the same grant `打印` uses);
  * `file/.../read` requires an `fs_read` pattern match and project-root path
    validation (`..`, absolute paths and symlink escapes rejected);
* `pure`: stdio stdout is allowed because console is already a pure-mode
  console grant; **file resources are refused by the mode check**, even if a
  caller constructs a `Capabilities` set with a matching `fs_read` pattern.
  Network/process resources do not exist in this slice.
* `project`: stdio stdout and explicitly granted project-file reads.
* `strict`: only the existing **stdio/stdout** console channel is permitted
  (`resource.open("stdio","stdout",["write"])`, console grant); every file,
  network, process or device resource open is refused with
  `permission.denied`. This is the existing console capability reached through
  the resource ABI, not a new OS capability (ADR-0030). Strict L1 is otherwise
  unchanged.
* Manifest capability requests are not grants; only the operator-provided
  effective set is consulted.

### 5. Errors, budgets and tracing

New stable categories:

| Code | Meaning |
|---|---|
| `runtime.resource` | forged/cross-run/stale handle, invalid resource state |
| `resource.closed` | read/write/close on an already closed handle (recoverable) |
| `resource.unsupported` | kind/descriptor/rights not implemented in this slice |

`resource.write` to stdout charges the existing output budget and appends to
the host output buffer exactly like `打印`; capability denial is
`permission.denied`. Trace records `resource.open`, `resource.read`,
`resource.write` and `resource.close` with kind/count/eof metadata, never raw
file bytes or handles.

### 6. Wire and boundary

* `to_wire(Resource)` emits `{"type":"resource"}` only; decoding it is a hard
  error (same rule as `Memory`).
* The generic `Host::invoke` boundary rejects arguments containing a resource
  handle, so plugins/foreign kernels/file adapters never receive one.
* Plugin output cannot mint a handle: the constructor is host-only and the
  value is not decodable from plugin wire.

### 7. Non-goals

* No `print` migration in this ADR; that is CM-A07 and a later increment.
* File **write** is added by [ADR-0026](0026-buffered-project-file-write-streams.md),
  not this decision; this ADR remains read-only for files.
* No file write, directory listing, rename, metadata, network, DNS, TLS,
  process, clock, random or device operations.
* No generic third-party provider registration; CM-A09 remains open.
* No zero-copy, async, deadlines per stream, seek or shared memory.
* No strict-mode file/network/process resource support and no claim that
  host-in-process I/O is a sandbox; strict stdio is the existing console.

## Compatibility

* Additive value variant; old programs never produce `Resource`.
* New builtins only; existing `fs.read/write`, `打印`, plugin and foreign
  behavior is unchanged.
* No wire version bump: the new tag is rejected, not accepted.
* New error categories are additive; existing categories/codes are unchanged.

## Consequences

* `.mq.md` can implement `write_text`-style output by encoding text into
  `Memory` (fixture 59) and calling `resource.write`, and can implement
  chunked file readers with `resource.read`/`Result`/EOF.
* The remaining high-level I/O surface can grow in the standard library without
  a new `HostOp` per function.
* The resource table is host state per run; closed slots are not reused in this
  slice, so generation checks are straightforward.
* Strict remains conservative; `.mq.md` resource code runs in `pure` (stdout)
  or `project` (file read) only.

## Verification

`docs/evidence/2026-11-20-resource-stream-p3.md` records unit tests for grants,
mode refusal, traversal rejection, forged/closed handles, output budget,
file chunking/EOF, wire rejection and a `.mq.md` stdout fixture.
