#!/bin/sh
# Copyright 2026 OpenDuo contributors
# SPDX-License-Identifier: FSL-1.1-Apache-2.0
set -eu
bundle=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
prefix=
while [ "$#" -gt 0 ]; do
    case "$1" in
        --prefix) prefix=$2; shift 2 ;;
        --artifact-dir) bundle=$2; shift 2 ;;
        *) echo 'Usage: install.sh --prefix /absolute/path [--artifact-dir bundle]' >&2; exit 2 ;;
    esac
done
case "$prefix" in /*) ;; *) echo 'An explicit absolute --prefix is required' >&2; exit 2 ;; esac
case "$prefix" in /|/usr|/bin|/lib|/etc) echo 'Use a dedicated installation prefix' >&2; exit 2 ;; esac
case "$prefix" in *'|'*|*'&'*|*'\'*) echo 'Unsupported prefix character' >&2; exit 2 ;; esac
target=$(cat "$bundle/TARGET")
case "$(uname -s):$(uname -m):$target" in
    Linux:aarch64:aarch64-unknown-linux-gnu|Linux:x86_64:x86_64-unknown-linux-gnu|Darwin:arm64:aarch64-apple-darwin) ;;
    *) echo 'Bundle architecture does not match this operating system' >&2; exit 1 ;;
esac
(cd "$bundle" && if command -v sha256sum >/dev/null 2>&1; then sha256sum -c SHA256SUMS; else shasum -a 256 -c SHA256SUMS; fi)
mkdir -p "$prefix/bin" "$prefix/lib" "$prefix/include" "$prefix/share/pocket-rs"
cp -R "$bundle/bin/." "$prefix/bin/"
cp -R "$bundle/lib/." "$prefix/lib/"
cp -R "$bundle/include/." "$prefix/include/"
cp -R "$bundle/share/pocket-rs/." "$prefix/share/pocket-rs/"
if [ -f "$prefix/lib/pkgconfig/pocket-core.pc" ]; then
    sed "s|^prefix=.*|prefix=$prefix|" "$prefix/lib/pkgconfig/pocket-core.pc" > "$prefix/lib/pkgconfig/.pocket-core.pc.tmp"
    mv "$prefix/lib/pkgconfig/.pocket-core.pc.tmp" "$prefix/lib/pkgconfig/pocket-core.pc"
fi
echo "Installed Pocket Rust $target at $prefix"
echo "Run: $prefix/bin/pocket self-test"
