#!/usr/bin/env bash
# Copyright 2026 OpenDuo contributors
# SPDX-License-Identifier: FSL-1.1-Apache-2.0
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$project_dir"
ndk="${ANDROID_NDK_HOME:-${ANDROID_HOME:-/opt/homebrew/share/android-commandlinetools}/ndk/28.2.13676358}"
case "$(uname -s)" in Darwin) host=darwin-x86_64 ;; Linux) host=linux-x86_64 ;; *) exit 1 ;; esac
bin="$ndk/toolchains/llvm/prebuilt/$host/bin"
output="${1:-$project_dir/.build/android/jni}"
for spec in 'aarch64-linux-android:aarch64-linux-android:arm64-v8a' 'x86_64-linux-android:x86_64-linux-android:x86_64'; do
  IFS=: read -r target compiler abi <<< "$spec"
  linker="$bin/${compiler}26-clang"
  [[ -x "$linker" ]] || { echo 'Pinned Android NDK is missing' >&2; exit 1; }
  env "CARGO_TARGET_$(echo "$target" | tr '[:lower:]-' '[:upper:]_')_LINKER=$linker" \
    RUSTFLAGS='-C link-arg=-Wl,-z,max-page-size=16384' \
    cargo build -p pocket-ffi --release --locked --target "$target"
  mkdir -p "$output/$abi"
  cp "target/$target/release/libpocket_core.so" "$output/$abi/"
  "$linker" -shared -fPIC -Wall -Wextra -Werror -Iinclude bindings/java/pocket_jni.c \
    -L"$output/$abi" -lpocket_core -Wl,-soname,libpocket_jni.so \
    -Wl,-z,max-page-size=16384,-z,relro,-z,now,-z,noexecstack,--no-undefined -o "$output/$abi/libpocket_jni.so"
done
