// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
#include "pocket_core.h"
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static unsigned checks;
#define CHECK(expr) do { ++checks; if (!(expr)) { fprintf(stderr, "FAIL: %s\n", #expr); exit(1); } } while (0)

static uint64_t expect(PocketResult result, int status, const char *contains) {
    CHECK(result.status == status);
    CHECK(result.payload.data != NULL && result.payload.len > 0);
    char *text = malloc(result.payload.len + 1);
    CHECK(text != NULL);
    memcpy(text, result.payload.data, result.payload.len);
    text[result.payload.len] = '\0';
    CHECK(strstr(text, contains) != NULL);
    uint64_t handle = result.handle;
    free(text);
    pocket_result_free(result);
    return handle;
}

static void dispatch(uint64_t handle, const char *command, const char *contains) {
    expect(pocket_session_dispatch(handle, (const uint8_t *)command, strlen(command)), 0, contains);
}

int main(void) {
    CHECK(pocket_abi_version() == POCKET_ABI_VERSION);
    const char *pair = "{\"op\":\"parse_pairing\",\"text\":\"duoduopocket://pair?host=example.test&code=prepared\"}";
    expect(pocket_protocol_call((const uint8_t *)pair, strlen(pair)), 0, "example.test");
    expect(pocket_protocol_call(NULL, 10), 1, "invalid_input");
    const char *voice = "{\"op\":\"frame_voice\",\"packets\":[[1,2],[3]]}";
    expect(pocket_protocol_call((const uint8_t *)voice, strlen(voice)), 0, "[2,0,1,2,1,0,3]");
    const char *config = "{\"endpoint\":{\"host\":\"example.test\",\"port\":443,\"tls\":true},\"room\":\"prepared\",\"token\":\"fixture-token\",\"scope\":\"room\"}";
    uint64_t handle = expect(pocket_session_create((const uint8_t *)config, strlen(config)), 0, "\"abi\":1");
    CHECK(handle != 0);
    dispatch(handle, "{\"type\":\"connect\"}", "open_live");
    dispatch(handle, "{\"type\":\"live_text\",\"generation\":1,\"frame\":{\"type\":\"ready\",\"api\":2,\"seq\":0,\"messages\":[]}}", "\"online\":true");
    dispatch(handle, "{\"type\":\"live_text\",\"generation\":1,\"frame\":{\"type\":\"speech\",\"speech_id\":\"c-prepared\"}}", "reset_decoder");
    const uint8_t packet[] = {0xb8, 0xff, 0xfe};
    expect(pocket_session_binary(handle, 1, packet, sizeof packet), 0, "decode");
    dispatch(handle, "{\"type\":\"send_text\",\"id\":\"prepared-message\",\"text\":\"Prepared text\",\"now_ms\":0}", "persist");
    dispatch(handle, "{\"type\":\"persisted\",\"id\":\"prepared-message\",\"generation\":1,\"ok\":false,\"now_ms\":0}", "storage_failed");
    expect(pocket_session_close(handle), 0, "close_live");
    expect(pocket_session_close(handle), 1, "invalid_handle");
    expect(pocket_session_binary(handle, 1, packet, sizeof packet), 1, "invalid_handle");
    printf("PASS %u C ABI assertions; linked installed shared library; no network or audio device\n", checks);
    return 0;
}
