/*
 * Conformance resource provider (C ABI v1) for the unknown kind `demo.sensor`.
 *
 * The host's released `marqdo-provider-worker` helper loads this library and
 * serves the framed-JSON resource protocol; the CLI never links this code.
 * The provider ABI is re-declared here from the public C ABI specification
 * (no standalone header is published yet).
 *
 * Behaviour used by the fixture:
 *   open("demo.sensor", "unit-1", ["read"]) -> token
 *   first read(max_bytes>=5)                -> "hello", eof=true
 *   write(bytes)                            -> accepted, discarded
 *   close(token)                            -> closes exactly once
 */
#include <stdint.h>
#include <stdio.h>
#include <string.h>

#define ABI_VERSION 1
#define MQ_OK 0
#define MQ_ERR 1

static char g_last_error[256];
static int closed = 0;

uint32_t marqdo_resource_provider_abi_version(void) { return ABI_VERSION; }

static int fail(const char *message) {
    snprintf(g_last_error, sizeof(g_last_error), "%s", message);
    return MQ_ERR;
}

int32_t marqdo_resource_open(
    const char *kind,
    const char *descriptor,
    const char *rights,
    uint64_t *token
) {
    (void)descriptor;
    if (kind == NULL || strcmp(kind, "demo.sensor") != 0) {
        return fail("unsupported kind");
    }
    if (rights == NULL || strstr(rights, "read") == NULL) {
        return fail("read right is required");
    }
    if (token == NULL) {
        return fail("token pointer is NULL");
    }
    *token = 1;
    closed = 0;
    snprintf(g_last_error, sizeof(g_last_error), "%s", "ok");
    return MQ_OK;
}

int32_t marqdo_resource_read(
    uint64_t token,
    uint8_t *buffer,
    uint32_t capacity,
    uint32_t *read,
    int32_t *eof
) {
    static const char payload[] = "hello";
    if (token != 1 || buffer == NULL || read == NULL || eof == NULL) {
        return fail("invalid read arguments");
    }
    if (closed) {
        return fail("resource is closed");
    }
    if (capacity < sizeof(payload) - 1) {
        return fail("buffer is too small");
    }
    memcpy(buffer, payload, sizeof(payload) - 1);
    *read = (uint32_t)(sizeof(payload) - 1);
    *eof = 1;
    return MQ_OK;
}

int32_t marqdo_resource_write(
    uint64_t token,
    const uint8_t *bytes,
    uint32_t length,
    uint32_t *written
) {
    if (token != 1 || written == NULL || (bytes == NULL && length != 0)) {
        return fail("invalid write arguments");
    }
    if (closed) {
        return fail("resource is closed");
    }
    *written = length;
    return MQ_OK;
}

int32_t marqdo_resource_close(uint64_t token) {
    if (token != 1) {
        return fail("unknown token");
    }
    if (closed) {
        return fail("resource is already closed");
    }
    closed = 1;
    return MQ_OK;
}

int32_t marqdo_resource_last_error(char *buffer, uint32_t capacity) {
    if (buffer == NULL || capacity == 0) {
        return MQ_ERR;
    }
    snprintf(buffer, capacity, "%s", g_last_error);
    return MQ_OK;
}
