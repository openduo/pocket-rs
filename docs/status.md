# Validation status

English · [简体中文](status.zh-CN.md)

Updated 2026-11-15. The 0.1.0 protocol/shared SDK passes local native acceptance. Android/iOS
application migration remains subsequent work. Public source/release: `openduo/pocket-rs`.

Implemented: validated pairing/credentials, origin-bound API request construction, state/history/
messages/attachments/speech contracts, bounded incremental Opus framing, cursor/live lifecycle,
Ambient seat/hello/press/mute/playback/interruption, durable admission effects and bounded retries,
generation cancellation, a pure Rust network reference client, and C ABI v1.

Compatibility uses executed original iOS sources at the pinned migration revision: 15 pairing cases,
4 voice packet cases, 8 surface cases, 10 backoff steps and 11 Ambient scenarios with 110 steps.
Focused Rust regressions cover malformed/truncated framing, authentication boundaries, persistence,
immutable retries/deadlines, stale callbacks, microphone restoration and playback reception/drain.

## Executed acceptance

| Gate | Actual evidence |
| --- | --- |
| Rust automation | 17 test cases pass on macOS ARM64 and native Linux ARM64, including the Swift-vector suites and real loopback transport; rustfmt and warning-free Clippy pass |
| Original iOS vectors | Regenerating with pinned, unmodified Swift sources produces byte-identical committed fixtures; no SwiftPM/vendor framework required |
| Native build | Pinned `rust:1.100.0-bookworm` ARM64 image, actual aarch64 kernel/userspace, locked dependencies; shared/static libraries and reference client built |
| Physical Linux installation | Ubuntu 26.04.1 aarch64, glibc 2.43, 485 MiB memory, read-only root; bundle installed at `/var/tmp/pocket-rs-0.1.0` without changing system storage |
| Physical protocol runtime | Installed CLI passes 13 native-loopback areas: pairing/auth, limits, durable admission, text retry, voice upload, speech framing, attachments, redirect boundary, replay, Ambient hello/media, playback, interruption and final unmute |
| Shared-library consumer | Installed C consumer passes 54 ABI assertions; the physical target's GCC 15.2 independently recompiles the distributed C source and that executable also passes |
| ELF/load dependencies | `ELF64`, `AArch64`, `DYN`; `libpocket_core.so` links only libc and libgcc_s, with the system ELF loader; no Go/JVM/Tailscale runtime |
| Swift consumer | macOS ARM64 executable imports the installed C module and runs pairing/create/connect/ready/close, returning every Rust buffer |
| iOS preparation | Static archives built for `aarch64-apple-ios` and `aarch64-apple-ios-sim`, combined into an XCFramework; no original iOS app/device migration is claimed |

The physical shared library SHA-256 is
`60edf68be47a908be596d9a6c456ed8b242c28e04bfd853d023414e4870014c3`.
The installed CLI SHA-256 is
`8493e9c908a2faca03cbaf35c8cd0c644795607081ba9e0091a9989e96d14d74`.
Raw device logs remain ignored; public evidence excludes hostnames, serials, real room ids and tokens.
The tests use prepared Opus bytes and explicitly simulated hardware completion, never a microphone,
speaker or external room. Installation is in tmpfs and will disappear on reboot.

CI runs the same contracts/consumer packaging on x86_64 Linux, native ARM64 Linux and ARM64 macOS;
Apple also regenerates Swift vectors and builds the XCFramework. Runner labels follow
[GitHub's architecture reference](https://docs.github.com/en/actions/reference/runners/github-hosted-runners).
CI execution is recorded separately from local/physical evidence in the release.

## Remaining product integration

- Android and iOS applications currently retain their existing engines. JNI/Swift application
  integration, storage migration and actual app background/audio acceptance are subsequent work.
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
