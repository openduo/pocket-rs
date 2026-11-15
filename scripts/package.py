#!/usr/bin/env python3
# Copyright 2026 OpenDuo contributors
# SPDX-License-Identifier: FSL-1.1-Apache-2.0
"""Create a relocatable, checksummed SDK bundle. Build outputs are never committed."""
import argparse
import gzip
import hashlib
import pathlib
import shutil
import tarfile
import tomllib

ROOT = pathlib.Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument("--target", required=True, choices=["aarch64-unknown-linux-gnu", "x86_64-unknown-linux-gnu", "aarch64-apple-darwin"])
parser.add_argument("--build-dir", type=pathlib.Path, required=True)
args = parser.parse_args()
version = tomllib.loads((ROOT / "Cargo.toml").read_text())["workspace"]["package"]["version"]
name = f"pocket-rs-{version}-{args.target}"
bundle = ROOT / "dist" / name
if bundle.exists():
    shutil.rmtree(bundle)
for directory in ["bin", "lib/pkgconfig", "include", "share/pocket-rs/examples/c", "share/pocket-rs/examples/swift", "share/pocket-rs/docs"]:
    (bundle / directory).mkdir(parents=True, exist_ok=True)
extension = "dylib" if "apple" in args.target else "so"
for source, destination in [("pocket", "bin/pocket"), ("pocket-c-smoke", "bin/pocket-c-smoke"), (f"libpocket_core.{extension}", f"lib/libpocket_core.{extension}"), ("libpocket_core.a", "lib/libpocket_core.a")]:
    shutil.copy2(args.build_dir / source, bundle / destination)
for path in (ROOT / "include").iterdir():
    shutil.copy2(path, bundle / "include" / path.name)
for path in ["LICENSE", "NOTICE", "THIRD_PARTY_NOTICES.md", "THIRD_PARTY_DEPENDENCIES.json", "THIRD_PARTY_LICENSES.txt", "README.md", "README.zh-CN.md"]:
    shutil.copy2(ROOT / path, bundle / "share/pocket-rs" / path)
for directory in ["examples", "docs", "bindings"]:
    shutil.copytree(ROOT / directory, bundle / "share/pocket-rs" / directory, dirs_exist_ok=True)
shutil.copy2(ROOT / "scripts/install.sh", bundle / "install.sh")
(bundle / "install.sh").chmod(0o755)
(bundle / "TARGET").write_text(args.target + "\n")
(bundle / "lib/pkgconfig/pocket-core.pc").write_text((ROOT / "pocket-core.pc.in").read_text().replace("@PREFIX@", "/opt/pocket-rs").replace("@VERSION@", version))
files = sorted(path for path in bundle.rglob("*") if path.is_file())
(bundle / "SHA256SUMS").write_text("".join(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {path.relative_to(bundle)}\n" for path in files))
archive = bundle.with_name(bundle.name + ".tar.gz")
with archive.open("wb") as output, gzip.GzipFile(filename="", mode="wb", fileobj=output, mtime=0) as zipped, tarfile.open(fileobj=zipped, mode="w") as tar:
    for path in sorted(bundle.rglob("*")):
        info = tar.gettarinfo(str(path), arcname=str(path.relative_to(bundle.parent)))
        info.uid = info.gid = 0
        info.uname = info.gname = ""
        info.mtime = 0
        if path.is_file():
            with path.open("rb") as source:
                tar.addfile(info, source)
        else:
            tar.addfile(info)
digest = hashlib.sha256(archive.read_bytes()).hexdigest()
archive.with_name(archive.name + ".sha256").write_text(f"{digest}  {archive.name}\n")
print(f"Packaged {archive.name}: {archive.stat().st_size} bytes, SHA-256 {digest}")
