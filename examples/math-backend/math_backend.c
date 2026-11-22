/*
 * Marqdo plugin ABI v1 example plugin in C.
 *
 * Exports `math_backend.add` through `mq_plugin_entry_v1`, using the wire_version 1
 * JSON envelope. It deliberately has no external dependencies: the tiny JSON
 * scanning below is a demo aid, not a general-purpose JSON parser.
 *
 * Build:
 *   cc -std=c11 -O2 -shared -fPIC -I include examples/math-backend/math_backend.c \
 *      -o examples/math-backend/math_backend.so
 *
 * Test-only compile switches (used by the host-side conformance tests):
 *   -DMQ_SOLVER_REPORT_ABI_MAJOR=2   report a wrong ABI major on purpose
 */

#include "marqdo_plugin_abi_v1.h"

#include <limits.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#ifndef MQ_SOLVER_REPORT_ABI_MAJOR
#define MQ_SOLVER_REPORT_ABI_MAJOR MQ_ABI_MAJOR
#endif

static const char PLUGIN_METADATA[] =
    "{\"name\":\"math.backend\",\"version\":\"0.1.0\"}";

static const char SIGNATURE_ADD[] = "{\"signature\":\"($a: Int, $b: Int) -> Int\"}";

/* Single-threaded ABI: the host table is captured once at entry. */
static const MqHostApi *g_host = NULL;

/* Return the pointer just past `"key"` + optional whitespace + ':' inside
 * `json`. Returns NULL when the key is absent. This is a demo scanner. */
static const char *find_key(const char *json, const char *key) {
    size_t key_len = strlen(key);
    const char *cursor = json;
    while ((cursor = strchr(cursor, '"')) != NULL) {
        const char *name_start = cursor + 1;
        const char *name_end = strchr(name_start, '"');
        if (name_end == NULL) {
            return NULL;
        }
        if ((size_t)(name_end - name_start) == key_len &&
            memcmp(name_start, key, key_len) == 0) {
            const char *after = name_end + 1;
            while (*after == ' ' || *after == '\t' || *after == '\n' ||
                   *after == '\r') {
                after++;
            }
            if (*after != ':') {
                return NULL;
            }
            after++;
            while (*after == ' ' || *after == '\t' || *after == '\n' ||
                   *after == '\r') {
                after++;
            }
            return after;
        }
        cursor = name_end + 1;
    }
    return NULL;
}

/* Find the object that is the value of key `name` and return a pointer to its
 * opening '{'. */
static const char *find_object_value(const char *json, const char *name) {
    const char *value = find_key(json, name);
    if (value == NULL || *value != '{') {
        return NULL;
    }
    return value;
}

/* Extract a quoted JSON string value for `key`, copying at most `out_len - 1`
 * bytes into `out` (NUL terminated). Rejects escapes for simplicity. */
static int read_string_value(const char *object, const char *key, char *out,
                             size_t out_len) {
    const char *value = find_key(object, key);
    if (value == NULL || *value != '"') {
        return -1;
    }
    const char *start = value + 1;
    const char *end = strchr(start, '"');
    if (end == NULL) {
        return -1;
    }
    size_t len = (size_t)(end - start);
    if (len + 1 > out_len || memchr(start, '\\', len) != NULL) {
        return -1;
    }
    memcpy(out, start, len);
    out[len] = '\0';
    return 0;
}

/* Extract the integer argument `name` from the request's `args` object. */
static int read_int_argument(const char *json, const char *name,
                             long long *out_value) {
    const char *args = find_object_value(json, "args");
    if (args == NULL) {
        return -1;
    }
    const char *argument = find_object_value(args, name);
    if (argument == NULL) {
        return -1;
    }
    char decimal[32];
    if (read_string_value(argument, "decimal", decimal, sizeof(decimal)) != 0) {
        return -1;
    }
    char *end = NULL;
    long long value = strtoll(decimal, &end, 10);
    if (end == NULL || *end != '\0' || end == decimal) {
        return -1;
    }
    *out_value = value;
    return 0;
}

/* Allocate an output buffer through the host allocator and copy `len` bytes.
 * The host frees it after copying; the plugin never frees output buffers. */
static int32_t emit(const char *json, size_t len, MqOwnedBytes *out) {
    uint8_t *buffer;
    if (g_host == NULL || out == NULL) {
        return MQ_ERR_INVALID_ARGUMENT;
    }
    buffer = (uint8_t *)g_host->alloc(g_host->host_data, len);
    if (buffer == NULL) {
        return MQ_ERR_OUT_OF_MEMORY;
    }
    memcpy(buffer, json, len);
    out->data = buffer;
    out->len = len;
    return MQ_OK;
}

static int32_t emit_error(const char *code, const char *message,
                          MqOwnedBytes *out, int32_t failure_code) {
    char json[512];
    int written = snprintf(
        json, sizeof(json),
        "{\"wire_version\":1,\"kind\":\"response\",\"error\":"
        "{\"kind\":\"invoke\",\"code\":\"%s\",\"message\":\"%s\"}}",
        code, message);
    if (written < 0 || (size_t)written >= sizeof(json)) {
        return MQ_ERR_INTERNAL;
    }
    if (emit(json, (size_t)written, out) != MQ_OK) {
        return MQ_ERR_OUT_OF_MEMORY;
    }
    /* Non-zero: the invocation failed even though an error envelope was
     * written. The host reads the envelope for the structured error. */
    return failure_code;
}

/* Optional hot-path ABI backend for pure math algorithms. */
static int32_t math_backend_add(MqCallContext *ctx, MqBytes args_wire,
                          MqOwnedBytes *result_wire, MqOwnedBytes *error_wire) {
    char *json;
    long long a = 0;
    long long b = 0;
    long long sum;
    char response[160];
    int written;

    (void)ctx;
    if (result_wire == NULL || error_wire == NULL) {
        return MQ_ERR_INVALID_ARGUMENT;
    }
    *result_wire = (MqOwnedBytes){NULL, 0};
    *error_wire = (MqOwnedBytes){NULL, 0};
    if (args_wire.data == NULL || args_wire.len == 0) {
        return emit_error("bad_arguments", "request wire is empty", error_wire,
                          MQ_ERR_INVALID_ARGUMENT);
    }
    if (args_wire.len > MQ_MAX_REQUEST_WIRE_BYTES) {
        return emit_error("request_too_large", "request wire is too large",
                          error_wire, MQ_ERR_INVALID_ARGUMENT);
    }
    json = (char *)malloc(args_wire.len + 1);
    if (json == NULL) {
        return emit_error("out_of_memory", "plugin could not copy request",
                          error_wire, MQ_ERR_OUT_OF_MEMORY);
    }
    memcpy(json, args_wire.data, args_wire.len);
    json[args_wire.len] = '\0';

    if (read_int_argument(json, "a", &a) != 0 ||
        read_int_argument(json, "b", &b) != 0) {
        free(json);
        return emit_error("bad_arguments",
                          "math_backend.add requires Int arguments `a` and `b`",
                          error_wire, MQ_ERR_INVALID_ARGUMENT);
    }
    free(json);

#ifdef MQ_SOLVER_CRASH
    /* Test-only crash injection for isolated-worker crash tests. */
    abort();
#endif

    if (__builtin_add_overflow(a, b, &sum)) {
        return emit_error("integer_overflow", "math_backend.add overflowed i64",
                          error_wire, MQ_ERR_INTERNAL);
    }
    written = snprintf(
        response, sizeof(response),
        "{\"wire_version\":1,\"kind\":\"response\",\"value\":"
        "{\"type\":\"int\",\"decimal\":\"%lld\"}}",
        sum);
    if (written < 0 || (size_t)written >= sizeof(response)) {
        return MQ_ERR_INTERNAL;
    }
    return emit(response, (size_t)written, result_wire);
}

#ifdef MQ_SOLVER_MULTI_EXPORT
/* Test-only second export: `math_backend.neg`, used to prove complete-name routing
 * (ADR-0010). It is not part of the default demo manifest. */
static int32_t math_backend_neg(MqCallContext *ctx, MqBytes args_wire,
                          MqOwnedBytes *result_wire, MqOwnedBytes *error_wire) {
    char *json;
    long long a = 0;
    char response[160];
    int written;

    (void)ctx;
    if (result_wire == NULL || error_wire == NULL) {
        return MQ_ERR_INVALID_ARGUMENT;
    }
    *result_wire = (MqOwnedBytes){NULL, 0};
    *error_wire = (MqOwnedBytes){NULL, 0};
    if (args_wire.data == NULL || args_wire.len == 0) {
        return emit_error("bad_arguments", "request wire is empty", error_wire,
                          MQ_ERR_INVALID_ARGUMENT);
    }
    json = (char *)malloc(args_wire.len + 1);
    if (json == NULL) {
        return emit_error("out_of_memory", "plugin could not copy request",
                          error_wire, MQ_ERR_OUT_OF_MEMORY);
    }
    memcpy(json, args_wire.data, args_wire.len);
    json[args_wire.len] = '\0';
    if (read_int_argument(json, "a", &a) != 0) {
        free(json);
        return emit_error("bad_arguments", "math_backend.neg requires Int `a`",
                          error_wire, MQ_ERR_INVALID_ARGUMENT);
    }
    free(json);
    written = snprintf(
        response, sizeof(response),
        "{\"wire_version\":1,\"kind\":\"response\",\"value\":"
        "{\"type\":\"int\",\"decimal\":\"%lld\"}}",
        -a);
    if (written < 0 || (size_t)written >= sizeof(response)) {
        return MQ_ERR_INTERNAL;
    }
    return emit(response, (size_t)written, result_wire);
}
#endif

/* Required ABI v1 entry point. */
int32_t mq_plugin_entry_v1(const MqHostApi *host, MqPluginApi *plugin) {
    MqBytes export_name;
    MqBytes signature_wire;
    int32_t code;

    if (host == NULL || plugin == NULL) {
        return MQ_ERR_INVALID_ARGUMENT;
    }
    if (host->struct_size < sizeof(MqHostApi)) {
        return MQ_ERR_ABI_MISMATCH;
    }
    if (host->abi_major != MQ_ABI_MAJOR) {
        return MQ_ERR_ABI_MISMATCH;
    }
    if (host->alloc == NULL || host->free == NULL ||
        host->register_function == NULL || host->request_capability == NULL) {
        return MQ_ERR_ABI_MISMATCH;
    }

    g_host = host;

    /* The host pre-fills struct_size; keep it and report our ABI version. */
    plugin->struct_size = (uint32_t)sizeof(MqPluginApi);
    plugin->abi_major = MQ_SOLVER_REPORT_ABI_MAJOR;
    plugin->abi_minor = MQ_ABI_MINOR;
    plugin->plugin_metadata_wire.data = (const uint8_t *)PLUGIN_METADATA;
    plugin->plugin_metadata_wire.len = sizeof(PLUGIN_METADATA) - 1;

    export_name.data = (const uint8_t *)"math_backend.add";
    export_name.len = sizeof("math_backend.add") - 1;
    signature_wire.data = (const uint8_t *)SIGNATURE_ADD;
    signature_wire.len = sizeof(SIGNATURE_ADD) - 1;

    code = host->register_function(host->host_data, export_name, signature_wire,
                                   math_backend_add);
    if (code != MQ_OK) {
        return code;
    }

#ifdef MQ_SOLVER_MULTI_EXPORT
    export_name.data = (const uint8_t *)"math_backend.neg";
    export_name.len = sizeof("math_backend.neg") - 1;
    signature_wire.data = (const uint8_t *)"{\"signature\":\"($a: Int) -> Int\"}";
    signature_wire.len = sizeof("{\"signature\":\"($a: Int) -> Int\"}") - 1;
    code = host->register_function(host->host_data, export_name, signature_wire,
                                   math_backend_neg);
    if (code != MQ_OK) {
        return code;
    }
#endif
    return MQ_OK;
}
