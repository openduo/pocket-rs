// Copyright 2026 OpenDuo contributors
// SPDX-License-Identifier: FSL-1.1-Apache-2.0
import dev.openduo.pocket.protocol.NativeCore;
import java.nio.charset.StandardCharsets;

/** Independent Java consumer, also runnable on Android ART without an application dependency. */
public final class Consumer {
    private static byte[] bytes(String text) { return text.getBytes(StandardCharsets.UTF_8); }
    private static String text(NativeCore.Result result) {
        if (result.status != 0) throw new AssertionError(new String(result.payload, StandardCharsets.UTF_8));
        return new String(result.payload, StandardCharsets.UTF_8);
    }
    public static void main(String[] args) {
        if (NativeCore.abiVersion() != 1) throw new AssertionError("ABI version");
        NativeCore.Result created = NativeCore.call(NativeCore.COMPONENT_CREATE, 0, 0, bytes("{\"type\":\"packet_stream\"}"), null);
        text(created);
        NativeCore.Result packets = NativeCore.call(NativeCore.COMPONENT_BINARY, created.handle, 0, new byte[]{3,0,1,2,3}, null);
        if (!text(packets).contains("[[1,2,3]]")) throw new AssertionError("Native binary packet framing");
        text(NativeCore.call(NativeCore.COMPONENT_DISPATCH, created.handle, 0, bytes("{\"type\":\"finish\"}"), null));
        text(NativeCore.call(NativeCore.COMPONENT_CLOSE, created.handle, 0, null, null));
        if (NativeCore.call(NativeCore.COMPONENT_CLOSE, created.handle, 0, null, null).status == 0) throw new AssertionError("Retired handle");
        if (NativeCore.call(NativeCore.PROTOCOL, 0, 0, bytes("{"), null).status == 0) throw new AssertionError("Malformed command");
        System.out.println("PASS Java JNI ABI, binary framing, finish, result copies, retired handle and malformed input");
    }
}
