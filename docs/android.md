# Android protocol integration

English · [简体中文](android.zh-CN.md)

Pocket Android consumes this SDK through `NativeCore`, a small Java/C JNI binding of C ABI v1.
The application pins `openduo/pocket-rs` as a submodule; Gradle builds the same Rust source for the
host JVM and Android `arm64-v8a` / `x86_64` (API26+, 16 KiB ELF alignment). No shared library is
copied into public source. Native handles are explicit owners, never credential or socket pointers.

| Shared Rust implementation | Android integration |
| --- | --- |
| Endpoint, pasted pairing, pairing request/result, query encoding | `Endpoint`, `ChannelClient.pair` |
| Origin-bound Chat, voice, state/history, files, speech and live requests | `ChannelClient`, `RoomAccess.speech` |
| Upload acknowledgement/digest admission | `ChannelClient.upload`; host hashes the actual file bytes |
| Incremental LE16 Opus framing and malformed/truncated-stream rejection | `PacketStream`; stream worker closes it |
| Live ready/row/message validation and additive events | `PocketController.frame` |
| Ambient hello/seat, media framing, overlapping presses and final unmute | `AmbientInput`, controller uplink/downlink |
| Playback declaration, reception/completion/idle clock, interruption and stale receipts | `AmbientOutput`, `RoomPlayback` |
| Monotonic voice budgets and live backoff | `VoiceRetry`, `RedialBackoff`; host supplies time/scheduling |

Java facades map existing domain values and snapshots; their previous protocol reducers are removed.
No Java fallback silently handles a failed Rust load. Structured Rust codes cross the bridge;
existing localized application errors remain the presentation boundary. AEC, codecs, microphone,
AudioTrack, focus, mediaPlayback service, UI, network requests and SQLite remain platform-owned.

Android keeps its existing OutboxSender/RoomLink admission, request cancellation, database atomicity
and resource ordering. It uses granular Rust owners rather than driving the aggregate Rust Session
as a second sender. New Linux clients can instead use Session commands/effects for the whole
protocol session. Both routes call the same core implementations. Component handles are closed on
stream completion/cancellation or owning-component disposal; process-wide owners match the app's
controller lifetime. Results are copied into Java arrays and freed once in JNI, including errors.

## Build

Install Rust 1.100.0 and targets `aarch64-linux-android` / `x86_64-linux-android`, JDK17 and Android
NDK `28.2.13676358`. `scripts/build-android.sh` builds only `pocket-ffi` and the C JNI shim; it does
not link the Rust network client or Go. `python3 scripts/package-android.py` emits a dual-ABI AAR,
consumer keep rules, licenses and a checksummed source/integration SDK archive. AAR consumers use:

```groovy
dependencies { implementation files('libs/pocket-core-0.1.0.aar') }
```

The AAR exposes `dev.openduo.pocket.protocol.NativeCore`; its operation constants map to the
versioned C functions. See [ABI](abi.md) and [independent Java consumer](../examples/java/Consumer.java).
Pocket Android's source integration builds the pinned revision instead of consuming a prebuilt AAR.
Its existing Go/Tailscale adapter remains the Android network implementation; the protocol library
has no Go dependency. Linux uses the device's available network and does not require that adapter.

## Acceptance boundary

Host JVM contracts load the actual JNI/Rust library; they do not substitute a Java reducer. The
bounded E2 check uses the existing public application/signature/pairing and verifies native ABI,
binary framing, seat rules, microphone restoration and playback clocks, plus prepared Try text,
voice, files, streaming speech, cancellation and live reconnect. Actual microphone/room speech,
session-notification sends, Sight and broad UI matrices are outside this migration check. The
screen-off reply check uses silent native Opus/AudioTrack and a prepared local source; it exercises
the shared playback owner and existing Android background policy without contacting a real room.

Exact executed results and published revisions are recorded in [status](status.md); do not infer
human acoustic acceptance or all-platform app parity from library/ABI tests.
