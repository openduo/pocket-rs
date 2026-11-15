# Pocket Rust engineering rules

Build a transport-independent Rust implementation of the DuoDuo room protocol, a stable C ABI,
and a runnable native Linux reference client. The Android and iOS applications remain consumers
to integrate through the ABI. Android Chat/Ambient protocol adoption and a bounded E2 check are
required by the user's latest completion gate; iOS application integration remains future work.
Neither Go nor an embedded Tailscale node belongs in this library.

- Pin upstream iOS and Android source revisions. Generate compatibility fixtures by executing
  original upstream code; do not describe self-written expected values as upstream vectors.
- Core protocol/state code performs no network, audio-device, credential-store or UI operations.
  Represent effects explicitly. All asynchronous results carry an operation/connection generation.
- Preserve actual playback drain, hello-after-first-packet, microphone restoration, immutable
  idempotent retries and authorization failure semantics. Keep resource/queue bounds explicit.
- Use typed Rust APIs and a versioned C boundary. Validate FFI input, contain panics, document
  memory/thread ownership and expose structured errors, never UI-language strings.
- Install Linux artifacts only under the chosen prefix. The connected ARM64 device has a read-only
  root filesystem; use its temporary writable prefix for acceptance and document volatility.
  Do not repartition, mount storage, change boot services or acquire microphones for protocol tests.
- Relevant deterministic tests, native loopback transport and installed-library consumers are the
  acceptance scope. Do not repeat Android UI/lifecycle matrices or send to actual rooms/people.
- No subagents. English source/comments and public primary documentation, with Chinese migration,
  installation and status companions. Retain FSL/SPDX and original attribution.
- Actual hosts, room/device identities, tokens and device logs belong only in ignored `.build/`.
  Public fixtures use reserved example addresses and clearly prepared data.
- Release only after native ARM64 installation, C ABI and Swift-consumer checks pass. Keep Android,
  Linux, macOS and iOS validation evidence distinct; cross-compilation alone is not native execution.
