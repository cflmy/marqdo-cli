/* Marqdo ResourceProvider C ABI v1 (ADR-0036).
 *
 * This ABI is for trusted native adapters, not a sandbox. The host loads the
 * shared library, checks marqdo_resource_provider_abi_version() == 1 and calls
 * these symbols for language-level resource.* operations.
 *
 * Conventions:
 *   - return 0 on success, nonzero on failure;
 *   - failure text is available from marqdo_resource_last_error();
 *   - rights is a comma-separated list (e.g. "read,write");
 *   - provider tokens are private to the adapter and must not be exposed to
 *     language Values, wire frames or trace output.
 */
#ifndef MARQDO_RESOURCE_PROVIDER_H
#define MARQDO_RESOURCE_PROVIDER_H

#include <stdint.h>

#define MARQDO_RESOURCE_PROVIDER_ABI_VERSION 1u

uint32_t marqdo_resource_provider_abi_version(void);

int32_t marqdo_resource_open(const char *kind, const char *descriptor,
                             const char *rights_csv, uint64_t *out_token);

int32_t marqdo_resource_read(uint64_t token, uint8_t *buffer,
                             uint32_t capacity, uint32_t *out_read,
                             int32_t *out_eof);

int32_t marqdo_resource_write(uint64_t token, const uint8_t *bytes,
                              uint32_t length, uint32_t *out_written);

int32_t marqdo_resource_close(uint64_t token);

int32_t marqdo_resource_last_error(char *buffer, uint32_t capacity);

#endif /* MARQDO_RESOURCE_PROVIDER_H */
