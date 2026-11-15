// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
package dev.openduo.pocket.protocol;

/** Minimal JNI transport for ABI v1. No Android API, network, audio or application state. */
public final class NativeCore {
    public static final int PROTOCOL = 0, REQUEST = 1, CREATE = 2, DISPATCH = 3, BINARY = 4,
            CLOSE = 5, COMPONENT_CREATE = 6, COMPONENT_DISPATCH = 7,
            COMPONENT_BINARY = 8, COMPONENT_CLOSE = 9;

    static { System.loadLibrary("pocket_jni"); }

    private NativeCore() {}

    public static final class Result {
        public final int status;
        public final long handle;
        public final byte[] payload;

        public Result(int status, long handle, byte[] payload) {
            this.status = status;
            this.handle = handle;
            this.payload = payload;
        }
    }

    /** Input arrays are borrowed during the call; result payload is an independent Java copy. */
    public static native Result call(int operation, long handle, long generation, byte[] input, byte[] body);
    public static native int abiVersion();
}
