#!/usr/bin/env bash
# Copyright 2026 OpenDuo contributors
# SPDX-License-Identifier: FSL-1.1-Apache-2.0
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "$0")/.." && pwd)"
mkdir -p "$project_dir/.build/cargo-linux-registry"
image='rust:1.100.0-bookworm@sha256:ade39a710ef85f3506cffcfec3e8a24ba090badef1dff2fb798f71d878833fea'
docker run --rm --platform linux/arm64 \
  --mount "type=bind,source=$project_dir,target=/work" \
  --mount "type=bind,source=$project_dir/.build/cargo-linux-registry,target=/usr/local/cargo/registry" \
  -e CARGO_TARGET_DIR=/work/.build/target-linux-arm64 \
  -e 'RUSTFLAGS=--remap-path-prefix=/work=.' \
  -w /work "$image" bash -c '
    set -euo pipefail
    test "$(uname -m)" = aarch64
    cargo test --workspace --locked
    cargo build --workspace --release --locked
    cc -std=c11 -Wall -Wextra -Werror -O2 -Iinclude examples/c/consumer.c \
      -L.build/target-linux-arm64/release -lpocket_core \
      -Wl,-rpath,\$ORIGIN/../lib -o .build/target-linux-arm64/release/pocket-c-smoke
  '
python3 "$project_dir/scripts/package.py" --target aarch64-unknown-linux-gnu \
  --build-dir "$project_dir/.build/target-linux-arm64/release"
