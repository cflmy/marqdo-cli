/*
 * Marqdo plugin C ABI, generation v1.
 * =====================================================================
 *
 * This header is the canonical, frozen description of the Marqdo native
 * plugin ABI v1. It is an independent ABI generation: it is *not* the old
 * `marqdo_abi.h` v2 interface and is neither source- nor binary-compatible
 * with it. Only an explicit, separately tested adapter may ever bridge the
 * two.
 *
 * This file defines stable-layout C types only:
 *
 *   - fixed-width integers (`int32_t`, `uint32_t`) and `size_t`,
 *   - byte buffers represented as length + pointer (`MqBytes` /
 *     `MqOwnedBytes`),
 *   - opaque, host-owned handles (`MqCallContext`),
 *   - `extern "C"` function pointers.
 *
 * The following must NEVER cross this boundary:
 *
 *   - Rust `String` / `Vec` / `Result` / trait objects / panics,
 *   - C++ exceptions, Go runtime objects or garbage-collected pointers,
 *   - raw pointers to host-internal structures (other than the opaque
 *     `MqCallContext *` minted for one invocation),
 *   - real filesystem handles, sockets, keys or OS resources; capabilities
 *     are requested symbolically through `MqHostApi::request_capability`.
 *
 * A C ABI IS NOT A SANDBOX
 * ------------------------
 * Native plugins are loaded into the host process. They are not isolated:
 * a plugin can read host memory, call arbitrary syscalls, crash the process
 * or corrupt the host. In-process native loading therefore requires an
 * explicit user trust decision. Untrusted third-party plugins must run in an
 * isolated worker process (same logical plugin API, length-prefixed IPC
 * transport) or a Wasm component. Do not present this ABI as a security
 * boundary.
 *
 * VERSIONS
 * --------
 * Three independent versions exist and must never be conflated:
 *
 *   1. Marqdo language / syntax / semantics version,
 *   2. plugin ABI major.minor (this header, `MQ_ABI_MAJOR`/`MQ_ABI_MINOR`),
 *   3. plugin package SemVer (declared in `plugin.yaml`).
 *
 * This header is ABI major 1, minor 0.
 *
 * TARGET AND WORD SIZE
 * --------------------
 * The layout below uses `size_t`, so it depends on the target triple and the
 * pointer width. A plugin artifact is only loadable by a host built for the
 * same target triple and the same pointer width (32-bit and 64-bit objects
 * are never interchangeable). All struct fields use native endianness;
 * host and plugin must share the target byte order. The JSON wire payloads
 * are UTF-8 and endian-neutral.
 *
 * LAYOUT EXPECTATIONS (little/big-endian, size_t == 8 bytes)
 * ---------------------------------------------------------
 *   sizeof(MqBytes)           == 16, alignof == 8
 *   sizeof(MqOwnedBytes)      == 16, alignof == 8
 *   sizeof(MqPluginApi)       == 32, alignof == 8
 *   sizeof(MqHostApi)         == 56, alignof == 8
 *   sizeof(MqPluginInvoke)    ==  8
 * The same expectations are asserted in `crates/marqdo-plugin-abi`.
 *
 * BUFFER OWNERSHIP AND LIFETIMES
 * ------------------------------
 * There are three distinct kinds of byte buffer:
 *
 * (a) Host-owned input buffers (`args_wire`, capability `request_wire`,
 *     `export_name`, `signature_wire`, `plugin_metadata_wire` as watched by
 *     the host). The producer of the pointer keeps it valid only for the
 *     duration of the call in which it is passed. The consumer MUST NOT
 *     retain the pointer, MUST NOT free it and MUST copy any data it needs
 *     to keep. In particular the host copies plugin metadata, export names
 *     and signatures during the entry/registration call.
 *
 * (b) Host-allocated output buffers. The host owns `alloc`/`free`. A plugin
 *     that produces `MqOwnedBytes` output (`result_wire`, `error_wire`,
 *     capability `reply_wire`) MUST allocate the memory with
 *     `host->alloc(host->host_data, size)` and write the payload there. The
 *     consuming side (host for invoke results/errors, plugin for capability
 *     replies) copies what it needs and then releases the buffer with
 *     `host->free(host->host_data, data)`. The producer MUST NOT free the
 *     buffer itself and MUST NOT use `malloc`/`new`/GC memory for it.
 *
 * (c) Plugin-owned static metadata. During `mq_plugin_entry_v1` the plugin
 *     passes `plugin_metadata_wire` pointing at storage it owns (typically a
 *     static string). It only has to remain valid for the duration of the
 *     entry call; the host copies it.
 *
 * Zero-length semantics: a buffer with `len == 0` must have `data == NULL`
 * (hosts and plugins must treat `{NULL, 0}` as the empty buffer). A buffer
 * with `len > 0` must have a non-NULL `data`. Violations are protocol errors
 * (`MQ_ERR_INVALID_ARGUMENT` / `MQ_ERR_BAD_WIRE`), never silent.
 *
 * SIZE LIMITS (enforced by the host; plugins should fail fast)
 * -----------------------------------------------------------
 *   MQ_MAX_EXPORT_NAME_BYTES         256
 *   MQ_MAX_SIGNATURE_WIRE_BYTES      4096
 *   MQ_MAX_METADATA_WIRE_BYTES       65536
 *   MQ_MAX_REQUEST_WIRE_BYTES        8384512   (8 MiB - 4096)
 *   MQ_MAX_RESULT_WIRE_BYTES         8384512   (8 MiB - 4096)
 *   MQ_MAX_ERROR_WIRE_BYTES          1048576   (1 MiB)
 *   MQ_MAX_CAPABILITY_REPLY_BYTES    8384512   (8 MiB - 4096)
 * JSON depth is additionally limited to 64 levels. Requests/results that
 * exceed these limits are rejected with a structured error; they must not
 * be truncated and reparsed.
 *
 * ERROR FIELD BEHAVIOUR
 * ---------------------
 * `mq_plugin_entry_v1` returns `MQ_OK` (0) on success, a non-zero
 * `MQ_ERR_*` code on failure. On a non-zero return the host MUST NOT read
 * `plugin_metadata_wire`, `plugin_api.abi_*` or any registered callback.
 *
 * `MqPluginInvoke` returns `MQ_OK` (0) on success and a non-zero code on
 * failure:
 *   - on success, `*result_wire` holds exactly one UTF-8 JSON response
 *     envelope (see `spec/value-wire.md` / wire_version 1) and
 *     `*error_wire` MUST be `{NULL, 0}`;
 *   - on failure, `*error_wire` holds a UTF-8 JSON error envelope
 *     (`{ "wire_version": 1, "kind": "response", "error": {...} }`) and
 *     `*result_wire` MUST be `{NULL, 0}`. The host treats a missing error
 *     envelope as an unstructured plugin failure and still fails the call.
 * Both output pointers MUST be non-NULL as provided by the host; writing
 * through a NULL output pointer is a caller/plugin contract violation.
 *
 * OOM / NULL ALLOCATION: `host->alloc` returns NULL on failure (and for
 * `size == 0`). A plugin that cannot allocate must return
 * `MQ_ERR_OUT_OF_MEMORY` and must not write partial output buffers.
 *
 * THREAD MODEL
 * ------------
 * v1 is deliberately single-threaded and synchronous:
 *   - `mq_plugin_entry_v1` is called exactly once, by the loading thread;
 *   - every `MqPluginInvoke` and every host callback for a given loaded
 *     plugin instance runs on that same thread;
 *   - at most one invocation is in flight per instance; the host serializes
 *     calls. A plugin must not start its own threads that call back into
 *     `MqHostApi`, and must not invoke the host from a signal handler;
 *   - there is no async/parallel callback contract in v1. `MqCallContext`
 *     is valid only for the duration of the invocation it was minted for,
 *     must not be cached, and must not be used after the call returns.
 *
 * UNLOADING / SHUTDOWN
 * --------------------
 * v1 does not support hot-unload or reload of a loaded native library.
 * A plugin lives for the lifetime of the process (or of the trusted loading
 * scope); there is no `shutdown` callback in v1. Hosts must not unload a
 * library while a call is in flight, and must keep all registered function
 * pointers reachable until shutdown. Plugins must tolerate never being
 * unloaded.
 *
 * VERSION NEGOTIATION
 * -------------------
 * The host fills `MqPluginApi::struct_size` with sizeof(MqPluginApi) before
 * the entry call and zeroes the remaining fields; the plugin fills in
 * `abi_major`, `abi_minor` and `plugin_metadata_wire`. The host then:
 *   - rejects the load if `plugin->abi_major != MQ_ABI_MAJOR` (major is
 *     never negotiated);
 *   - accepts a plugin minor greater than its own, but clamps to the host's
 *     supported minor and only uses features defined at that minor.
 * The plugin mirrors the check against the host table:
 *   - reject if `host == NULL`, `plugin == NULL`,
 *     `host->struct_size < sizeof(MqHostApi)` (the exact size of the v1.0
 *     table it was built against) or `host->abi_major != MQ_ABI_MAJOR`.
 * `struct_size` is the extension mechanism: a consumer must only read a
 * field when `struct_size >= offsetof(field) + sizeof(field)`; a producer
 * sets `struct_size = sizeof(its struct)` and may append new fields only in
 * a later ABI minor while never reordering or resizing earlier fields.
 * Fields appended in a later minor are ABSENT (not usable) when the
 * negotiated minor is lower.
 *
 * EXCEPTIONS, PANICS, UNDEFINED BEHAVIOUR
 * ---------------------------------------
 * No panic, exception, longjmp or unwinding of any kind may cross the ABI
 * boundary. Rust plugins should call `extern "C"` entry points from
 * panic-aborting builds or catch panics inside the SDK; C++ plugins must
 * catch all exceptions inside their invocation function. The host treats a
 * process crash as a host process crash: this ABI cannot contain it.
 *
 * WIRE FORMAT
 * -----------
 * All payloads are UTF-8 JSON, `wire_version == 1`. See the canonical
 * `spec/value-wire.md` (type-tagged values: every value is an object with a
 * `type` tag; 64-bit-and-beyond integers use decimal strings; NaN/Infinity
 * use explicit tags). Names:
 *   - request  : {"wire_version":1,"kind":"request","function":...,
 *                 "args":{...},"context":{...}}
 *   - response : {"wire_version":1,"kind":"response","value":{...}}
 *   - error    : {"wire_version":1,"kind":"response","error":
 *                 {"kind":"...","code":"...","message":"...",...}}
 * `export_name` and capability `name` are UTF-8 byte strings (not JSON).
 * `signature_wire` is the JSON object {"signature":"<signature string>"}.
 * `plugin_metadata_wire` is the JSON object
 * {"name":"<plugin name>","version":"<semver>"} (extra fields ignored).
 */

#ifndef MARQDO_PLUGIN_ABI_V1_H
#define MARQDO_PLUGIN_ABI_V1_H

#include <stddef.h>
#include <stdint.h>

#ifdef __cplusplus
extern "C" {
#endif

/* ------------------------------------------------------------------ */
/* Version and result codes                                           */
/* ------------------------------------------------------------------ */

/** Plugin ABI major version. Major mismatches always fail the load. */
#define MQ_ABI_MAJOR 1u
/** Plugin ABI minor version implemented by this header. */
#define MQ_ABI_MINOR 0u
/** Human-readable ABI version of this header. */
#define MQ_ABI_VERSION_STRING "1.0"
/** Required exported symbol name. */
#define MQ_PLUGIN_ENTRY_SYMBOL "mq_plugin_entry_v1"

/** Success. */
#define MQ_OK 0
/** Malformed argument (NULL pointer, bad pointer/length pair, ...). */
#define MQ_ERR_INVALID_ARGUMENT 1
/** ABI major mismatch or incompatible struct_size. */
#define MQ_ERR_ABI_MISMATCH 2
/** A required export could not be registered. */
#define MQ_ERR_REGISTRATION 3
/** Operation/type/capability not supported by this version. */
#define MQ_ERR_UNSUPPORTED 4
/** Host allocation failed. */
#define MQ_ERR_OUT_OF_MEMORY 5
/** Payload is not valid wire_version 1 JSON. */
#define MQ_ERR_BAD_WIRE 6
/** Unexpected internal plugin failure (including caught panics). */
#define MQ_ERR_INTERNAL 7
/** Capability request failed or was denied by the host. */
#define MQ_ERR_CAPABILITY 8

/* ------------------------------------------------------------------ */
/* Size limits                                                        */
/* ------------------------------------------------------------------ */

#define MQ_MAX_EXPORT_NAME_BYTES 256u
#define MQ_MAX_SIGNATURE_WIRE_BYTES 4096u
#define MQ_MAX_METADATA_WIRE_BYTES 65536u
#define MQ_MAX_REQUEST_WIRE_BYTES ((8u * 1024u * 1024u) - 4096u)
#define MQ_MAX_RESULT_WIRE_BYTES ((8u * 1024u * 1024u) - 4096u)
#define MQ_MAX_ERROR_WIRE_BYTES (1024u * 1024u)
#define MQ_MAX_CAPABILITY_REPLY_BYTES ((8u * 1024u * 1024u) - 4096u)

/* ------------------------------------------------------------------ */
/* Stable-layout buffer types                                         */
/* ------------------------------------------------------------------ */

/**
 * Borrowed, immutable byte view.
 *
 * Ownership stays with the producer; the pointer is valid only for the
 * duration of the call that received it. `{NULL, 0}` is the empty buffer.
 * A non-NULL `data` with `len == 0` is tolerated by neither side.
 */
typedef struct {
    const uint8_t *data;
    size_t len;
} MqBytes;

/**
 * Mutable byte buffer ownership transfer.
 *
 * The producer allocates with `MqHostApi::alloc` and passes ownership to the
 * consumer, which copies the bytes and then calls `MqHostApi::free` on
 * `data`. `{NULL, 0}` is the empty buffer. A buffer left non-empty when the
 * call returns is owned by the documented consumer; it is never freed by
 * the producer.
 */
typedef struct {
    uint8_t *data;
    size_t len;
} MqOwnedBytes;

/**
 * Opaque, host-owned invocation context.
 *
 * The plugin only ever receives a pointer to this type and must pass the
 * same pointer back to `request_capability`. Its layout is private to the
 * host and may change between ABI minors without notice. It is valid only
 * during the invocation it was minted for and must not be cached, copied or
 * inspected. Never dereference this pointer from a plugin.
 */
typedef struct MqCallContext MqCallContext;

/* ------------------------------------------------------------------ */
/* Function pointer types                                             */
/* ------------------------------------------------------------------ */

/**
 * Plugin invocation callback, registered once per export.
 *
 * Contract: see the ERROR FIELD BEHAVIOUR section above. The callback runs
 * on the host invocation thread, with at most one call in flight per plugin
 * instance. It must be reentrant-safe with respect to its own state because
 * repeated invocations may occur sequentially. It must not panic/throw.
 *
 * @param ctx          Opaque host context for this call (may be NULL only
 *                     if the host could not mint one; then the plugin should
 *                     fail with MQ_ERR_INVALID_ARGUMENT).
 * @param args_wire    Borrowed UTF-8 JSON request envelope.
 * @param result_wire  Host-provided out-parameter; on success must be
 *                     filled with a host-allocated UTF-8 JSON response
 *                     envelope. Must not be NULL.
 * @param error_wire   Host-provided out-parameter; on failure must be filled
 *                     with a host-allocated UTF-8 JSON error envelope. Must
 *                     not be NULL.
 * @return MQ_OK on success, a non-zero MQ_ERR_* code otherwise.
 */
typedef int32_t (*MqPluginInvoke)(
    MqCallContext *ctx,
    MqBytes args_wire,
    MqOwnedBytes *result_wire,
    MqOwnedBytes *error_wire
);

/* ------------------------------------------------------------------ */
/* Host API table                                                     */
/* ------------------------------------------------------------------ */

/**
 * Host-provided function table, valid for the whole life of the plugin
 * instance. All function pointers are non-NULL in ABI v1.0.
 *
 * `host_data` is an opaque host-owned pointer that must be passed back to
 * every function that takes it. Plugins must not interpret it.
 */
typedef struct {
    /**
     * sizeof(MqHostApi) as seen by the host. Consumers must check this
     * against their own required size before reading any field (see VERSION
     * NEGOTIATION). Producers never shrink the table; fields are only
     * appended and never reordered.
     */
    uint32_t struct_size;
    /** Host-supported ABI major (must be MQ_ABI_MAJOR). */
    uint32_t abi_major;
    /** Host-supported ABI minor (features above this are unavailable). */
    uint32_t abi_minor;
    /** Opaque host state; pass back to every callback that accepts it. */
    void *host_data;

    /**
     * Host allocator. Returns NULL on failure and for `size == 0`.
     * The returned memory is at least `size` bytes and suitably aligned for
     * byte buffers. The matching `free` must be used; never free with
     * malloc/new/free() from the plugin runtime.
     */
    void *(*alloc)(void *host_data, size_t size);

    /**
     * Host deallocator; must be called exactly once for a pointer returned by
     * `alloc` (including capability replies allocated by the host). Passing
     * NULL is a no-op.
     */
    void (*free)(void *host_data, void *ptr);

    /**
     * Register one plugin export during `mq_plugin_entry_v1`.
     *
     * `export_name` is a UTF-8 dotted name, already namespaced by the
     * manifest validation (e.g. "solver.add"). `signature_wire` is UTF-8
     * JSON: {"signature":"(...)"}. Name and signature are copied by the
     * host during this call. Returns MQ_OK or a non-zero MQ_ERR_* code; on
     * failure the export must be considered not registered.
     *
     * Export names must not shadow Marqdo built-ins and must match the
     * manifest exports exactly.
     */
    int32_t (*register_function)(
        void *host_data,
        MqBytes export_name,
        MqBytes signature_wire,
        MqPluginInvoke invoke
    );

    /**
     * Request a whitelisted capability from the host for the current call.
     *
     * `name` is a UTF-8 capability name (e.g. "math.compute", "fs.read",
     * "clock"). `request_wire` is a UTF-8 JSON capability request whose shape
     * is defined per capability. On success the host allocates
     * `reply_wire` (with `alloc`) and the plugin must copy and then free it
     * with `free`. On failure `reply_wire` is `{NULL, 0}` and the call
     * returns a non-zero code.
     *
     * Capabilities are granted structurally by the host's authorization
     * layer before the plugin is loaded; this function only consumes a grant
     * and can never escalate privileges. `ctx` must be the context of the
     * current invocation only.
     */
    int32_t (*request_capability)(
        MqCallContext *ctx,
        MqBytes name,
        MqBytes request_wire,
        MqOwnedBytes *reply_wire
    );
} MqHostApi;

/* ------------------------------------------------------------------ */
/* Plugin API table (host-filled, plugin-filled)                      */
/* ------------------------------------------------------------------ */

/**
 * Plugin-side version/metadata table.
 *
 * The host initializes `struct_size` to sizeof(MqPluginApi) and zeroes the
 * rest before calling the entry point. The plugin must set `abi_major`,
 * `abi_minor` and `plugin_metadata_wire` and must not change
 * `struct_size` (it may only be larger in a future ABI minor; consumers
 * check it).
 */
typedef struct {
    /** sizeof(MqPluginApi) as initialized by the host. */
    uint32_t struct_size;
    /** Plugin ABI major; must equal MQ_ABI_MAJOR for the load to proceed. */
    uint32_t abi_major;
    /** Plugin ABI minor; the host clamps and negotiates down as needed. */
    uint32_t abi_minor;
    /**
     * Plugin metadata JSON, plugin-owned static storage, valid only for the
     * duration of the entry call. Format:
     *   {"name":"<plugin name>","version":"<semver>"}
     * The host copies it during the call. Must be a non-empty, valid UTF-8
     * JSON object.
     */
    MqBytes plugin_metadata_wire;
} MqPluginApi;

/**
 * Required exported entry point. Exactly one per plugin artifact.
 *
 * @param host   Host API table, host-owned, valid for the plugin lifetime.
 *               Must be non-NULL.
 * @param plugin Plugin API table, host-provided. The plugin must fill it in
 *               and may not retain it. Must be non-NULL.
 * @return MQ_OK on success, a non-zero MQ_ERR_* code on failure.
 *
 * The host calls this exactly once on the loading thread. The plugin
 * registers every export through `host->register_function` during this call
 * and must return MQ_OK only after all exports are registered successfully.
 */
typedef int32_t (*MqPluginEntryV1)(const MqHostApi *host, MqPluginApi *plugin);

#ifdef __cplusplus
} /* extern "C" */
#endif

/*
 * Compile-time layout checks for the canonical 64-bit target. On other word
 * sizes the expected sizes are smaller (every pointer/size_t shrinks by 4
 * bytes); the Rust side asserts the corresponding architecture-specific
 * values.
 */
#if defined(__STDC_VERSION__) && __STDC_VERSION__ >= 201112L
#if defined(__LP64__) || defined(_LP64)
_Static_assert(sizeof(MqBytes) == 16, "MqBytes layout must stay 16 bytes on 64-bit");
_Static_assert(sizeof(MqOwnedBytes) == 16, "MqOwnedBytes layout must stay 16 bytes on 64-bit");
_Static_assert(sizeof(MqPluginApi) == 32, "MqPluginApi layout must stay 32 bytes on 64-bit");
_Static_assert(sizeof(MqHostApi) == 56, "MqHostApi layout must stay 56 bytes on 64-bit");
#endif
#endif

#endif /* MARQDO_PLUGIN_ABI_V1_H */
