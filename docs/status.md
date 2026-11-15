# Validation status

English · [简体中文](status.zh-CN.md)

Updated 2026-11-15. The 0.1.0 protocol/shared SDK passes local native acceptance. Android protocol integration is included; iOS
application integration remains future work. Public source/release: `openduo/pocket-rs`.

Implemented: validated pairing/credentials, origin-bound API request construction, state/history/
messages/attachments/speech contracts, bounded incremental Opus framing, cursor/live lifecycle,
Ambient seat/hello/press/mute/playback/interruption, durable admission effects and bounded retries,
generation cancellation, a pure Rust network reference client, and C ABI v1.

Compatibility uses executed original iOS sources at the pinned migration revision: 15 pairing cases,
4 voice packet cases, 8 surface cases, 10 backoff steps and 11 Ambient scenarios with 110 steps.
Focused Rust regressions cover malformed/truncated framing, authentication boundaries, persistence,
immutable retries/deadlines, stale callbacks, microphone restoration and playback reception/drain.

## Android adoption

The current Pocket Android source adopts the shared protocol through JNI, with its Java protocol
reducers replaced rather than run beside Rust. Host JVM contracts load the real native library:
2,622 existing assertions pass. The public-identity dual-ABI Release APK is installed on E2 without
uninstalling, clearing data, pairing again or creating a new Tailscale node. The bounded device
check passes 16 Rust/ART assertions plus 22 Try application-coordination assertions. A separate
15-assertion screen-off check passes with silent native Opus/AudioTrack, completion/drain,
service/wake-lock cleanup and settings restoration. The original real room returns Online.
No microphone or actual room test content was used. See [integration ownership](android.md).
The Android published Preview 2 APK is unchanged; this evidence covers the updated source/local APK.

## Executed acceptance

| Gate | Actual evidence |
| --- | --- |
| Rust automation | 19 test cases pass on macOS ARM64 and native Linux ARM64, including the Swift-vector suites and real loopback transport; rustfmt and warning-free Clippy pass |
| Original iOS vectors | Regenerating with pinned, unmodified Swift sources produces byte-identical committed fixtures; no SwiftPM/vendor framework required |
| Native build | Pinned `rust:1.100.0-bookworm` ARM64 image, actual aarch64 kernel/userspace, locked dependencies; shared/static libraries and reference client built |
| Physical Linux installation | Ubuntu 26.04.1 aarch64, glibc 2.43, 485 MiB memory, read-only root; bundle installed at `/var/tmp/pocket-rs-0.1.0` without changing system storage |
| Physical protocol runtime | Installed CLI passes 13 native-loopback areas: pairing/auth, limits, durable admission, text retry, voice upload, speech framing, attachments, redirect boundary, replay, Ambient hello/media, playback, interruption and final unmute |
| Shared-library consumer | Installed C consumer passes 54 ABI assertions; the physical target's GCC 15.2 independently recompiles the distributed C source and that executable also passes |
| ELF/load dependencies | `ELF64`, `AArch64`, `DYN`; `libpocket_core.so` links only libc and libgcc_s, with the system ELF loader; no Go/JVM/Tailscale runtime |
| Swift consumer | macOS ARM64 executable imports the installed C module and runs pairing/create/connect/ready/close, returning every Rust buffer |
| iOS preparation | Static archives built for `aarch64-apple-ios` and `aarch64-apple-ios-sim`, combined into an XCFramework; no original iOS app/device migration is claimed |

The physical shared library SHA-256 is
`4646dd7bbb638d10617b86371740a9f64d9f75074ba2c33a3e4ac9ea631bb9a3`.
The installed CLI SHA-256 is
`18b3d2513b5b7745013d9ba37a964478219f2d372e10a7f46656d0852a427490`.
Raw device logs remain ignored; public evidence excludes hostnames, serials, real room ids and tokens.
Linux protocol tests use prepared Opus bytes and explicitly simulated hardware completion, never a
microphone, speaker or external room. Android evidence uses prepared Try data and silent native
decode/playback; it is separate from prior human-speech acceptance. Installation is in tmpfs and will disappear on reboot.

CI runs the same contracts/consumer packaging on x86_64 Linux, native ARM64 Linux and ARM64 macOS;
Apple also regenerates Swift vectors and builds the XCFramework; Android builds the dual-ABI
JNI/AAR. Java JNI consumers execute on native Linux hosts, and E2 evidence is recorded separately. Runner labels follow
[GitHub's architecture reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
Verified remote runs: [SDK executable source 16c43fa](https://github.com/openduo/pocket-rs/actions/runs/43675723689)
and [Android integration 4208864](https://github.com/openduo/pocket-android/actions/runs/43675746653)
both pass. The subsequent release documentation commit changes no executable source.

## Remaining product integration

- Android uses shared Rust wire/framing/Ambient/retry owners through JNI. SessionViewer session-watch/
  notification command serialization and Passport/Sight/location feature protocols retain their
  existing Android implementations; these are outside this Chat/Ambient adoption gate. Existing UI, SQLite
  transactions, credential/network/audio resources and platform task lifetimes remain Android-owned.
  It does not run the aggregate Rust Session beside those owners. Full database/product migration
  is outside this protocol integration. iOS app integration remains future work.
- The Linux reference client uses pre-encoded packet files. Microphone/speaker drivers, resampling,
  Opus device codecs, acoustic AEC, wake words and hands-free UI are not part of this protocol SDK.
- Native loopback acceptance proves protocol execution and OS socket use, not actual room-service
  recognition, audible replies or real session notification receipt.
- Push and Sight are deferred. Camera capture/uplink, Passport BLE, location monitoring and UI/
  Markdown migration are not included. Unknown additive live events remain available to the host.
- C ABI v1 is implemented with C/Swift examples. iOS application integration is not implied by
  static-library/XCFramework compilation or a macOS Swift consumer.
- ARM64 GNU/Linux is the primary Linux binary target. Other OS/ABI combinations require their own
  build and native evidence. The physical target's temporary installation is lost on reboot.

Operational constants: client API 2 and audio 16 kHz/20 ms come from the upstream wire contract;
Opus packets are bounded at 1,275 bytes. SDK queue admission is bounded at 64 messages and 16 MiB
total body bytes. A single send slot has a 25-second monotonic budget (including storage/queueing),
with 0.5/1/2/4-second retry pauses. Live backoff is 1/2/4/8/16/30 seconds. Missing playback receipts
expire only after 30 seconds without progress and without pending hardware audio. Platform audio,
credentials, network and database transaction boundaries are described in [ABI](abi.md).
