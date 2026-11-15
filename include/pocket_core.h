// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
#ifndef OPENDUO_POCKET_CORE_H
#define OPENDUO_POCKET_CORE_H
#include <stddef.h>
#include <stdint.h>
#ifdef __cplusplus
extern "C" {
#endif

#define POCKET_ABI_VERSION 1
typedef struct { uint8_t *data; size_t len; } PocketBuffer;
typedef struct { int32_t status; uint64_t handle; PocketBuffer payload; } PocketResult;

/* Results contain length-delimited UTF-8 JSON, not NUL-terminated strings.
 * status == 0 is success; errors contain {"error":{"code":...,"detail":...}}.
 * Every returned result must be freed exactly once with pocket_result_free.
 * Input buffers are borrowed only until the call returns. No callbacks are retained.
 * Handles are validated IDs, thread-safe and serialized. Effects execute outside the ABI.
 * Close returns final cleanup effects: execute them before releasing platform resources.
 */
uint32_t pocket_abi_version(void);
PocketResult pocket_protocol_call(const uint8_t *data, size_t len);
/* Build voice/upload requests using a borrowed binary body. The response sets input_body=true
 * and omits the body copy; the host sends its original bytes. Other requests return body bytes. */
PocketResult pocket_request_build(const uint8_t *data, size_t len, const uint8_t *body, size_t body_len);
/* Granular owners for existing applications adopting one protocol concern at a time.
 * Do not also drive a session for the same concern. All handles share validation/thread rules. */
PocketResult pocket_component_create(const uint8_t *data, size_t len);
PocketResult pocket_component_dispatch(uint64_t handle, const uint8_t *data, size_t len);
/* kind: 0 incremental voice stream; 1 Ambient uplink; 2 Ambient downlink; 3 voice-body validation. */
PocketResult pocket_component_binary(uint64_t handle, uint64_t kind, const uint8_t *data, size_t len);
PocketResult pocket_component_close(uint64_t handle);
PocketResult pocket_session_create(const uint8_t *data, size_t len);
PocketResult pocket_session_dispatch(uint64_t handle, const uint8_t *data, size_t len);
PocketResult pocket_session_binary(uint64_t handle, uint64_t generation, const uint8_t *data, size_t len);
PocketResult pocket_session_close(uint64_t handle);
void pocket_result_free(PocketResult result);

#ifdef __cplusplus
}
#endif
#endif
