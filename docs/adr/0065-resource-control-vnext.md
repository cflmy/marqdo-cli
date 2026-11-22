# ADR-0065 — Declarative Resource Control

Status: Proposed. Follow-up to RFC-GPL-0004 §4 and ADR-0062.

## First milestone

Introduce a versioned provider contract for control on a live resource. Keep
Provider v2 stream transport and its schema supported. Select control-capable
providers explicitly through the manifest and lock, pinning both descriptor
content and the platform artifact. The check/inspect path reads declarations
without starting a worker. Dynamic action names and argument values also need
full validation at dispatch.

A manifest declares actions by resource kind. Each action has a unique name,
ordered argument shapes, a required right and a result kind. The result is
either a bounded Value Wire value or a child kind declared by the same provider.
Begin with positional arguments and a closed set of wire shapes. A handle always
selects the provider and kind that issued it. The caller cannot override either.
The dispatcher checks the live run-owned handle, provider grant, handle rights,
registered action, arity and shapes before sending a worker request. Both the
request and validated response are auditable without recording arguments.

Child creation returns an opaque provider token over the transport. The Host
validates the result envelope, kind, freshness and quota before minting a
run-owned Resource. Child rights are bounded by the action declaration and the
operator's grant. Validation and handle publication form one transaction. If
the Host cannot adopt a freshly allocated token, it releases the token or
tears down the faulted worker. It invalidates worker-owned handles on transport
failure and reaps them on run teardown. Raw host handles are excluded from all
wire values. Timeout, malformed frames and worker exit have bounded cleanup.

## Later milestone

Specify `resource.wait` separately after the control path works. Readiness
needs a monotonic deadline, a cancellation source and a bounded batch of
run-owned handles. Unsupported interests return a defined error. The
implementation must state how it interrupts each supported transport and what
happens to outstanding control requests on cancellation. A synchronous Worker
continues to provide process isolation from faults and needs a separate policy
for OS containment.

## Before acceptance

Use a counter for bounded state mutations, a queue for acknowledgments, and a
listener for child adoption. Test authorization before worker effects, unknown
actions, wrong shapes, untrusted or closed handles, repeated or mismatched
tokens, artifact substitution, malformed and oversized messages, worker loss
and teardown. Run the same stream-only Provider v2 suite on the new binary.
The initial implementation should register the three test kinds as external
fixtures. The host's closed `resource.request` catalogue remains governed by
ADR-0062.
