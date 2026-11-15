#!/usr/bin/env bash
# Copyright 2026 OpenDuo contributors
# SPDX-License-Identifier: FSL-1.1-Apache-2.0
set -euo pipefail
project_dir="$(cd -- "$(dirname -- "$0")/.." && pwd)"
cd "$project_dir"
cargo build -p pocket-ffi --release --locked
output="${1:-$project_dir/.build/jni-host}"
mkdir -p "$output"
case "$(uname -s)" in
  Darwin)
    java_dir="${JAVA_HOME:-$(/usr/libexec/java_home -v 17)}"
    cp target/release/libpocket_core.dylib "$output/"
    install_name_tool -id '@rpath/libpocket_core.dylib' "$output/libpocket_core.dylib"
    cc -dynamiclib -fPIC -Wall -Wextra -Werror -Iinclude -I"$java_dir/include" -I"$java_dir/include/darwin" \
       bindings/java/pocket_jni.c -L"$output" -lpocket_core -Wl,-rpath,@loader_path -o "$output/libpocket_jni.dylib"
    ;;
  Linux)
    java_dir="${JAVA_HOME:-$(dirname "$(dirname "$(readlink -f "$(command -v javac)")")")}"
    cp target/release/libpocket_core.so "$output/"
    cc -shared -fPIC -Wall -Wextra -Werror -Iinclude -I"$java_dir/include" -I"$java_dir/include/linux" \
       bindings/java/pocket_jni.c -L"$output" -lpocket_core -Wl,-rpath,'$ORIGIN' -o "$output/libpocket_jni.so"
    ;;
  *) echo 'Unsupported JNI host' >&2; exit 1 ;;
esac
mkdir -p "$output/classes"
javac --release 8 -d "$output/classes" bindings/java/src/main/java/dev/openduo/pocket/protocol/NativeCore.java examples/java/Consumer.java
java -Djava.library.path="$output" -cp "$output/classes" Consumer
