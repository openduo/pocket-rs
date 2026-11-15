# Installation and native verification

English · [简体中文](install.zh-CN.md)

The GNU/Linux ARM64 bundle targets `aarch64-unknown-linux-gnu`, built natively in pinned Debian
Bookworm (glibc 2.36 baseline). The protocol shared library has no Go/JVM, OpenSSL, audio daemon or
Tailscale dependency. The optional CLI uses Rust TLS and the system network. Do not copy the GNU
bundle to Alpine/musl or a 32-bit userspace.

## Release bundle

Verify the `.sha256` file, extract, and run its installer with a dedicated absolute prefix:

```sh
sha256sum -c pocket-rs-0.1.0-aarch64-unknown-linux-gnu.tar.gz.sha256
tar -xzf pocket-rs-0.1.0-aarch64-unknown-linux-gnu.tar.gz
cd pocket-rs-0.1.0-aarch64-unknown-linux-gnu
./install.sh --prefix "$HOME/.local/pocket-rs"
"$HOME/.local/pocket-rs/bin/pocket" self-test
"$HOME/.local/pocket-rs/bin/pocket-c-smoke"
```

The installer verifies every bundled file, checks OS/architecture and writes only the chosen
prefix. The CLI is independent of the SDK shared library; `pocket-c-smoke` deliberately links it
with a relative loader path to verify installation. Compile your own consumer:

```sh
export PKG_CONFIG_PATH="$HOME/.local/pocket-rs/lib/pkgconfig"
cc example.c $(pkg-config --cflags --libs pocket-core) \
  -Wl,-rpath,"$HOME/.local/pocket-rs/lib" -o example
```

The physical acceptance device has a read-only root and 485 MiB memory. Its dedicated prefix is
under `/tmp`, so installation is volatile across reboot. No package manager, Rust toolchain,
partition changes, microphone acquisition or boot service is needed. Production deployment should
choose an existing persistent application volume; this release does not mount or repartition it.

## Building the same ARM64 artifacts

On an ARM64 Mac/Linux host with Docker:

```sh
scripts/build-linux.sh
```

The script pins the container digest, asserts native `uname -m=aarch64`, runs all Rust contracts
and loopback acceptance, builds release libraries/client, compiles a separate C consumer and packages
the result. On x86 hosts this command uses emulation; do not report it as native evidence.

## Connecting to a real room

The device must already reach the room. System Tailscale is one option. Keep a fresh pairing link
in a private file, then:

```sh
pocket pair --link-file /private/pair-link --config /private/pocket/config.json
pocket state --config /private/pocket/config.json
pocket history --config /private/pocket/config.json --since 0
pocket text --config /private/pocket/config.json --id unique-stable-id --text-file /private/message.txt
pocket voice --config /private/pocket/config.json --id another-stable-id --packets /private/voice.opus-packets
```

Sending commands are explicit; `state`, `history` and startup do not drain the durable outbox.
`retry --id` reuses an existing stored request. Configuration/admission files use private permissions.
`speech` downloads packet-framed Opus; `live` can read/write prepared packet files over real sockets.
The reference client does not capture microphones, decode/play speakers, claim acoustic acceptance
or send fabricated `played` receipts for downloaded audio. Audio integrations must follow ABI rules.

## Apple integration

`scripts/build-apple.sh` builds a macOS shared/static library, runs the installed C and Swift
consumers, and creates an iOS-device/ARM64-simulator static XCFramework with the same header/module
map. Add `PocketCore.xcframework` to Xcode and import `PocketCore`; use the sample to manage result
memory. Keep AVAudioSession, AVAudioEngine, Keychain and background execution in Swift. This is SDK
build/consumer verification; the original iOS application has not been migrated or device-tested.
