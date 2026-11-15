// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
#include <jni.h>
#include "pocket_core.h"

JNIEXPORT jint JNICALL Java_dev_openduo_pocket_protocol_NativeCore_abiVersion(JNIEnv *env, jclass cls) {
    (void)env; (void)cls;
    return (jint)pocket_abi_version();
}

JNIEXPORT jobject JNICALL Java_dev_openduo_pocket_protocol_NativeCore_call(
    JNIEnv *env, jclass cls, jint op, jlong handle, jlong generation, jbyteArray input, jbyteArray body) {
    (void)cls;
    jsize len = input ? (*env)->GetArrayLength(env, input) : 0;
    jsize body_len = body ? (*env)->GetArrayLength(env, body) : 0;
    jbyte *data = input ? (*env)->GetByteArrayElements(env, input, NULL) : NULL;
    if (input && !data) return NULL;
    jbyte *bytes = body ? (*env)->GetByteArrayElements(env, body, NULL) : NULL;
    if (body && !bytes) { if (data) (*env)->ReleaseByteArrayElements(env, input, data, JNI_ABORT); return NULL; }
    PocketResult result;
    const uint8_t *p = (const uint8_t *)data;
    switch (op) {
        case 0: result = pocket_protocol_call(p, (size_t)len); break;
        case 1: result = pocket_request_build(p, (size_t)len, (const uint8_t *)bytes, (size_t)body_len); break;
        case 2: result = pocket_session_create(p, (size_t)len); break;
        case 3: result = pocket_session_dispatch((uint64_t)handle, p, (size_t)len); break;
        case 4: result = pocket_session_binary((uint64_t)handle, (uint64_t)generation, p, (size_t)len); break;
        case 5: result = pocket_session_close((uint64_t)handle); break;
        case 6: result = pocket_component_create(p, (size_t)len); break;
        case 7: result = pocket_component_dispatch((uint64_t)handle, p, (size_t)len); break;
        case 8: result = pocket_component_binary((uint64_t)handle, (uint64_t)generation, p, (size_t)len); break;
        case 9: result = pocket_component_close((uint64_t)handle); break;
        default: result = pocket_protocol_call(NULL, 0); break;
    }
    if (bytes) (*env)->ReleaseByteArrayElements(env, body, bytes, JNI_ABORT);
    if (data) (*env)->ReleaseByteArrayElements(env, input, data, JNI_ABORT);
    jbyteArray payload = (*env)->NewByteArray(env, (jsize)result.payload.len);
    if (payload) (*env)->SetByteArrayRegion(env, payload, 0, (jsize)result.payload.len, (const jbyte *)result.payload.data);
    jint status = result.status;
    jlong id = (jlong)result.handle;
    pocket_result_free(result);
    if ((*env)->ExceptionCheck(env)) return NULL;
    jclass type = (*env)->FindClass(env, "dev/openduo/pocket/protocol/NativeCore$Result");
    if (!type) return NULL;
    jmethodID init = (*env)->GetMethodID(env, type, "<init>", "(IJ[B)V");
    if (!init) return NULL;
    return (*env)->NewObject(env, type, init, status, id, payload);
}
