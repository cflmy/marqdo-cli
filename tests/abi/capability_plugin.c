/*
 * Test-only plugin: requests `fs.read` through the host capability broker.
 *
 * The plugin deliberately has no JSON parser of its own: it sends a fixed
 * request for `data/allowed.txt` and returns the broker's reply as a Text
 * value. This exercises the real ABI -> loader -> host broker -> ProjectFs
 * path, not a Rust unit shim.
 */
#include "marqdo_plugin_abi_v1.h"

#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static const MqHostApi *g_host = NULL;

static const char PLUGIN_METADATA[] =
    "{\"name\":\"demo.capability\",\"version\":\"0.1.0\"}";

static const char *PATH_REQUEST = "{\"path\":\"data/allowed.txt\"}";

static int32_t emit(const char *json, size_t len, MqOwnedBytes *out) {
    uint8_t *buffer = (uint8_t *)g_host->alloc(g_host->host_data, len);
    if (buffer == NULL) {
        return MQ_ERR_OUT_OF_MEMORY;
    }
    memcpy(buffer, json, len);
    out->data = buffer;
    out->len = len;
    return MQ_OK;
}

static int32_t capability_read(MqCallContext *ctx, MqBytes args_wire,
                               MqOwnedBytes *result_wire,
                               MqOwnedBytes *error_wire) {
    MqBytes name;
    MqBytes request;
    MqOwnedBytes reply = {NULL, 0};
    char response[512];
    int written;
    int32_t code;

    (void)args_wire;
    if (result_wire == NULL || error_wire == NULL) {
        return MQ_ERR_INVALID_ARGUMENT;
    }
    *result_wire = (MqOwnedBytes){NULL, 0};
    *error_wire = (MqOwnedBytes){NULL, 0};

    name.data = (const uint8_t *)"fs.read";
    name.len = sizeof("fs.read") - 1;
    request.data = (const uint8_t *)PATH_REQUEST;
    request.len = strlen(PATH_REQUEST);
#ifdef MQ_CAPABILITY_LOOP
    for (int i = 0; i < 100; i++) {
        MqOwnedBytes loop_reply = {NULL, 0};
        code = g_host->request_capability(ctx, name, request, &loop_reply);
        if (loop_reply.data != NULL) {
            g_host->free(g_host->host_data, loop_reply.data);
        }
        if (code != MQ_OK) {
            static const char BUDGET_JSON[] =
                "{\"wire_version\":1,\"kind\":\"response\",\"error\":"
                "{\"kind\":\"capability\",\"code\":\"capability.budget_exceeded\","
                "\"message\":\"capability request budget exceeded\"}}";
            return emit(BUDGET_JSON, sizeof(BUDGET_JSON) - 1, error_wire) == MQ_OK
                       ? code
                       : MQ_ERR_OUT_OF_MEMORY;
        }
    }
#endif
    code = g_host->request_capability(ctx, name, request, &reply);
    if (code != MQ_OK) {
        static const char ERROR_JSON[] =
            "{\"wire_version\":1,\"kind\":\"response\",\"error\":"
            "{\"kind\":\"capability\",\"code\":\"permission.denied\","
            "\"message\":\"fs.read was denied\"}}";
        return emit(ERROR_JSON, sizeof(ERROR_JSON) - 1, error_wire) == MQ_OK
                   ? code
                   : MQ_ERR_OUT_OF_MEMORY;
    }

    written = snprintf(
        response, sizeof(response),
        "{\"wire_version\":1,\"kind\":\"response\",\"value\":"
        "{\"type\":\"text\",\"value\":\"%.*s\"}}",
        (int)reply.len, (const char *)reply.data);
    if (reply.data != NULL) {
        g_host->free(g_host->host_data, reply.data);
    }
    if (written < 0 || (size_t)written >= sizeof(response)) {
        return MQ_ERR_INTERNAL;
    }
    return emit(response, (size_t)written, result_wire);
}

int32_t mq_plugin_entry_v1(const MqHostApi *host, MqPluginApi *plugin) {
    MqBytes export_name = {(const uint8_t *)"capability.read",
                           sizeof("capability.read") - 1};
    MqBytes signature = {
        (const uint8_t *)"{\"signature\":\"() -> Text\"}",
        sizeof("{\"signature\":\"() -> Text\"}") - 1};

    if (host == NULL || plugin == NULL || host->register_function == NULL ||
        host->request_capability == NULL || host->alloc == NULL ||
        host->free == NULL || host->abi_major != MQ_ABI_MAJOR) {
        return MQ_ERR_ABI_MISMATCH;
    }
    g_host = host;
    plugin->struct_size = (uint32_t)sizeof(MqPluginApi);
    plugin->abi_major = MQ_ABI_MAJOR;
    plugin->abi_minor = MQ_ABI_MINOR;
    plugin->plugin_metadata_wire.data = (const uint8_t *)PLUGIN_METADATA;
    plugin->plugin_metadata_wire.len = sizeof(PLUGIN_METADATA) - 1;
    return host->register_function(host->host_data, export_name, signature,
                                   capability_read);
}
