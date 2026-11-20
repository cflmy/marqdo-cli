# ADR-0018: Worker capability bridge frames

Status: **Accepted**
Date: 2026-11-20
Extends: ADR-0011 (capability broker) and `spec/plugin-abi.md` §9.1

## Context

The isolated worker transport initially supported only `ready`, `invoke`,
`shutdown` and `error` frames. The plugin loader's capability handler ran in
the host process for native plugins, but a worker was started with
`LoadOptions::default()`, so every worker capability request failed even when
the user had granted it. That made the broker's execution path unreachable for
the only transport designed to contain crashes.

The worker frame set is a versioned transport contract. Adding frames is an
explicit protocol extension, not an implementation detail.

## Decision

### 1. Runtime-only capability request/reply frames

While a worker is executing one `invoke`, it may send:

```json
{"kind":"capability_request","id":7,"capability":"fs.read",
 "request":{"path":"data/allowed.txt"}}
```

The host replies with exactly one frame carrying the same `id`:

```json
{"kind":"capability_reply","id":7,"data":"file contents"}
```

or:

```json
{"kind":"capability_error","id":7,"code":"permission.denied",
 "message":"read access ... was not granted"}
```

Rules:

* Requests carry **no trusted identity**. The host binds the package and
  session from the worker it spawned; frame fields are untrusted data.
* `request` must be a JSON value; the host serializes it back to the request
  bytes used by the broker. Non-JSON requests are rejected.
* Replies are text; `capability_reply.data` is returned to the plugin as raw
  UTF-8 bytes. Non-UTF-8 replies become `capability_error`.
* A reply with an unknown/mismatched `id`, or a different kind, is a protocol
  error. No other frames are accepted while waiting for a capability reply.
* Capability handling does not extend the invocation timeout: the host keeps a
  single deadline for the whole `invoke`, including capability round-trips.
* Only the broker's executing capabilities are reachable. In this build that
  means `fs.read`; everything else returns `capability.unavailable`.
* The frames do not change the C ABI or `MqHostApi`. They are an internal IPC
  protocol between the host and its own worker binary.

### 2. Worker and host implementation

The worker routes its capability handler through the same stdin/stdout frame
channel. During plugin invocation the outer frame loop is not reading, so the
handler may synchronously send a request and wait for the reply. Reads/writes
are guarded by mutexes; clean EOF or a partial frame remains a protocol error.

The host's `WorkerPlugin` owns an optional capability handler (the broker with
the project filesystem). It serves capability frames inside the existing invoke
deadline, writes replies, and marks/kills the worker on any transport failure
so a stale stdin is never reused.

## Consequences

* The worker capability bridge is no longer a stub; granted `fs.read` works in
  both native and worker transports.
* Request/reply identity is bound to the spawn, not to plugin-supplied text.
* Worker capability requests still run as the user process. This is cooperative
  authorization, **not** an OS sandbox; H3-B strict isolation remains required
  for any untrusted-code claim.
* The frame set change requires this ADR and an updated plugin-abi spec. Frame
  kinds are additive for future ABI minors; unknown frames fail closed.

## Tests

* `worker_capability_bridge_reads_granted_file_and_denies_without_grant`
  drives a real `.mq.md` program through a C worker plugin: granted
  `fs.read` returns the file contents; without the grant the run fails with a
  permission error and no contents.
* Host unit tests cover granted/denied `fs.read` and `capability.unavailable`
  for unsupported capabilities.
