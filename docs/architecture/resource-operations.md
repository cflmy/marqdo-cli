# Core resource-operation ownership

Status: **reviewed and enforced** (ADR-0062, RFC-EXT-0003 H-03/H-04).
This page is the ownership record for every operation reachable through
`Host::resource_request`. Adding an entry here is a product decision, not an
implementation detail; the architecture gate
`crates/cli/tests/architecture_boundaries.rs::resource_request_operation_allowlist_is_reviewed`
extracts the literals from the host source and fails on any unreviewed change.

## Dispatch rules

- Every operation requires `SandboxMode::Project`; `pure` and `strict` refuse all
  resource operations with `PermissionDenied`.
- `Memory`/resource handles never cross this API; path arguments are `Text`.
- Operation-level grants are checked before any host action (`check_process`,
  `check_read`, `check_write`, `check_entropy`, `check_env`, network grants).
- The compatibility dispatcher matches exact names in
  `crates/marqdo-host/src/host/resource_requests.rs`. The `fs.` prefix only
  distinguishes unknown-operation diagnostics.

## Reviewed operation table

| Operation | Class | Owner / implementation | Authorization | Evidence |
|---|---|---|---|---|
| `process.run` | Core OS primitive | `crate::process::run_process` | exact program grant (`--allow-program`) | ADR-0044; `crates/marqdo-host/tests/resource_streams.rs` |
| `env.get` | Core OS primitive | host environment | exact-name env grant | ADR-0043 |
| `clock.now` | Core OS primitive | `SystemTime` | project mode | ADR-0042 |
| `random.bytes` | Core OS primitive | OS entropy | explicit entropy grant (`--allow-random`) | ADR-0046 |
| `fs.read_at` | Core OS primitive | `ProjectFs::request("read_at")` | read grant | ADR-0045 |
| `fs.append` | Core OS primitive | `ProjectFs::request("append")` | write grant | ADR-0045 |
| `fs.walk` | Core OS primitive | `ProjectFs::request("walk")` | read grant | ADR-0045 |
| `fs.exists` | Core OS primitive | `ProjectFs::request("exists")` | read grant | ADR-0041 |
| `fs.metadata` | Core OS primitive | `ProjectFs::request("metadata")` | read grant | ADR-0041 |
| `fs.list` | Core OS primitive | `ProjectFs::request("list")` | read grant | ADR-0041 |
| `fs.mkdir` | Core OS primitive | `ProjectFs::request("mkdir")` | write grant | ADR-0041 |
| `fs.remove` | Core OS primitive | `ProjectFs::request("remove")` | write grant | ADR-0041 |
| `fs.rename` | Core OS primitive | `ProjectFs::request("rename")` | write grant on both paths | ADR-0041 |
| `regex.match` | Host-held built-in service | host regex library | project mode | ADR-0050 |
| `regex.captures` | Host-held built-in service | host regex library | project mode | ADR-0050 |
| `compress.gzip` | Host-held built-in service | host gzip library | project mode | ADR-0049 |
| `compress.gunzip` | Host-held built-in service | host gzip library | project mode | ADR-0049 |

`regex.*` and `compress.*` are existing host-held convenience services. They
do not create a general extension mechanism: no new domain operation may follow
them into the core.

## Forbidden additions

- No domain/business operation (math, text, graphics, networking protocol,
  database, …) may be added to `resource_request`. Ship it as a pure `.mq.md`
  package, a Callable Plugin ABI export, or a Resource Provider.
- No new language keyword, builtin name or CLI flag is added by this table.
- No provider may use or declare a core kind: the single list lives in
  `marqdo_host::CORE_RESOURCE_KINDS` and is enforced by both the CLI validator
  and trusted host registration.
- The removed `math.*` builtins (ADR-0061) are not coming back through this API.
