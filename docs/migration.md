# Rust migration plan

English · [简体中文](migration.zh-CN.md)

The requested deliverable is an independent, Go-free room protocol library, installable shared
library and native ARM64 Linux verification, with actual Android protocol adoption and an interface suitable for future iOS
adoption. This is not an Android UI rewrite or a replacement Tailscale implementation.

## Sources and compatibility

- iOS `openduo/pocket-ios` at `b9d91bb9b77a12c3e59e6f359af1e4537a4ded39`:
  `Packages/PocketKit/Sources/PocketCore` is the original wire-behavior reference.
- Android `openduo/pocket-android` at `9f59814df246c22c6e01c18b0c150ea1e93f37b0`:
  preserve its generation/cancellation, final-unmute and playback-reception corrections.
- Client API major 2, 16 kHz mono, 20 ms capture frames, length-prefixed Opus voice uploads,
  original surface values and device-token authorization remain unchanged.
- Rust semantic versions and C ABI versions are independent of the server's client API major.

## Work and acceptance gates

1. **Protocol core:** validated endpoint/pairing and scope, authenticated request construction,
   message/history/attachment/speech contracts, incremental bounded packet framing, live cursor
   and lifecycle events, Ambient hello/seat/mute/playback/interruption, retries and cancellation.
   Core code has no OS audio, networking, UI or Go dependencies.
2. **Compatibility:** run selected unmodified iOS source in a standalone Swift vector generator;
   commit sanitized inputs/outputs with source hashes. Consume those vectors from Rust tests.
   Add focused regressions for Android's newer final-unmute and locked-playback lifetime fixes.
3. **Consumers:** a pure Rust HTTP/WebSocket reference client and versioned C ABI. Linux uses
   networking already available to the process, including an optional system Tailscale connection.
   C/Swift consumers use the same protocol/state owners, with documented memory and thread rules.
4. **Native ARM64:** build on native aarch64 Linux, install libraries/header/client under a
   dedicated prefix, execute loopback Chat/voice/Ambient tests and a separately linked C consumer
   on the physical Linux target. Record ELF architecture, dependencies and checksums. The target
   root is read-only; its acceptance prefix is temporary and installation must not alter boot/storage.
5. **Android adoption:** wire the shared protocol into the existing Android app through JNI; retain
   its public package, signature, pairing, database and Tailscale node. Require host contracts, dual-ABI
   APK build and bounded E2 Chat/Ambient plus native playback acceptance before calling migration done.
6. **Apple readiness:** compile and execute a Swift consumer on macOS against the C ABI; build
   an iOS static library when the SDK is available, provide packaging/integration instructions,
   and keep this separate from actual iOS app acceptance.
7. **Release:** clean format/lint/tests, reproducible packaging, checksums, CI with native ARM64
   coverage, public source and versioned assets under `openduo/pocket-rs` after the gates pass.

## Application ownership

Android now replaces its wire construction, pairing, packet framing, Ambient input/output, live
validation and retry owners with Rust behind JNI. Existing Views, device adapters, credential storage,
database transactions, OutboxSender/RoomLink platform scheduling and foreground service remain. The
Rust aggregate Session is an alternative for new hosts; Android uses granular owners rather than
starting a duplicate aggregate. See [integration map](android.md). Each behavior has one executor.

iOS can import the C module from a static library/XCFramework, keeping SwiftUI, AVAudioSession,
AVAudioEngine, Keychain and background policy. Preserve its storage schemas initially. Translate
typed commands/effects at one small bridge, and move shared behavior only after compatibility tests.
Do not expose Rust layouts, Tokio types or Rust-owned pointers as Swift domain models.

The current release covers room Chat/voice/Ambient and adjacent room data contracts. Its media-v1
downlink fix supports Android's resumed Sight connection; camera capture/uplink stays in Pocket
Android and has separate acceptance. Push remains deferred. Passport BLE, geographic monitoring, Markdown/UI reducers,
full app database migration, wake words, microphone drivers and acoustic AEC are platform or later
migration work, listed explicitly in status rather than represented as completed parity.
