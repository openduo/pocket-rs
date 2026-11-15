#!/usr/bin/env bash
# Copyright 2026 OpenDuo contributors
# SPDX-License-Identifier: FSL-1.1-Apache-2.0
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$project_dir"
sdk_version="$(python3 -c 'import tomllib; print(tomllib.load(open("Cargo.toml", "rb"))["workspace"]["package"]["version"])')"
test "$(uname -s):$(uname -m)" = Darwin:arm64
export MACOSX_DEPLOYMENT_TARGET=14.0
cargo build --workspace --release --locked
install_name_tool -id '@rpath/libpocket_core.dylib' target/release/libpocket_core.dylib
cc -std=c11 -Wall -Wextra -Werror -O2 -Iinclude examples/c/consumer.c \
    -Ltarget/release -lpocket_core -Wl,-rpath,@executable_path/../lib \
    -o target/release/pocket-c-smoke
python3 scripts/package.py --target aarch64-apple-darwin --build-dir target/release
prefix="$project_dir/.build/install-macos"
"dist/pocket-rs-$sdk_version-aarch64-apple-darwin/install.sh" --prefix "$prefix"
"$prefix/bin/pocket-c-smoke"
swiftc -parse-as-library -I "$prefix/include" -L "$prefix/lib" -lpocket_core \
    -Xlinker -rpath -Xlinker "$prefix/lib" examples/swift/Consumer.swift \
    -o .build/swift-consumer
.build/swift-consumer
export IPHONEOS_DEPLOYMENT_TARGET=17.0
cargo build -p pocket-ffi --release --locked --target aarch64-apple-ios
cargo build -p pocket-ffi --release --locked --target aarch64-apple-ios-sim
rm -rf .build/PocketCore.xcframework
xcodebuild -create-xcframework \
    -library target/aarch64-apple-ios/release/libpocket_core.a -headers include \
    -library target/aarch64-apple-ios-sim/release/libpocket_core.a -headers include \
    -output .build/PocketCore.xcframework
cp LICENSE NOTICE THIRD_PARTY_NOTICES.md THIRD_PARTY_DEPENDENCIES.json THIRD_PARTY_LICENSES.txt .build/PocketCore.xcframework/
COPYFILE_DISABLE=1 tar -czf "dist/PocketCore-$sdk_version-apple-xcframework.tar.gz" -C .build PocketCore.xcframework
(cd dist && shasum -a 256 "PocketCore-$sdk_version-apple-xcframework.tar.gz" > "PocketCore-$sdk_version-apple-xcframework.tar.gz.sha256")
