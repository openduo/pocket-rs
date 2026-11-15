#!/usr/bin/env python3
# Copyright 2026 OpenDuo contributors
# SPDX-License-Identifier: FSL-1.1-Apache-2.0
import json
import pathlib
import subprocess
import sys

root = pathlib.Path(__file__).resolve().parent.parent
metadata = json.loads(subprocess.check_output(["cargo", "metadata", "--locked", "--format-version", "1"], cwd=root))
packages = [{key: package.get(key) for key in ["name", "version", "license", "repository", "source"]} for package in metadata["packages"] if package["source"]]
data = json.dumps(sorted(packages, key=lambda p: (p["name"], p["version"])), indent=2, sort_keys=True).encode() + b"\n"
path = root / "THIRD_PARTY_DEPENDENCIES.json"
if "--check" in sys.argv:
    if path.read_bytes() != data:
        raise SystemExit("Dependency notices differ from Cargo.lock")
else:
    path.write_bytes(data)
groups = {}
for package in sorted(metadata["packages"], key=lambda p: (p["name"], p["version"])):
    if not package["source"]:
        continue
    directory = pathlib.Path(package["manifest_path"]).parent
    files = sorted(path for path in directory.iterdir() if path.is_file() and path.name.upper().startswith(("LICENSE", "COPYING", "NOTICE", "UNLICENSE")))
    if package.get("license_file"):
        files.append(directory / package["license_file"])
    if not files:
        if package["name"] not in {"r-efi", "wasip2"}:
            raise SystemExit("Missing bundled license text for " + package["name"])
        # Cargo.lock also resolves UEFI/WASI-only packages, absent from shipped Linux/Apple binaries.
        text = "Not linked in the distributed Linux/Apple targets. Registry license declaration: " + package["license"]
        if (directory / "AUTHORS").exists():
            text += "\n\n" + (directory / "AUTHORS").read_text().strip()
        groups.setdefault(text, []).append(f"{package['name']} {package['version']} (target-only declaration)")
        continue
    for source in dict.fromkeys(files):
        text = source.read_text(errors="replace").replace("\r\n", "\n").strip()
        groups.setdefault(text, []).append(f"{package['name']} {package['version']} ({source.name})")
licenses = ("# Resolved dependency license texts\n\nGenerated from the exact Cargo.lock registry packages.\n\n" + "\n\n".join("Packages: " + ", ".join(names) + "\n\n" + text for text, names in groups.items()) + "\n").encode()
license_path = root / "THIRD_PARTY_LICENSES.txt"
if "--check" in sys.argv:
    if license_path.read_bytes() != licenses:
        raise SystemExit("Dependency license texts differ from Cargo.lock")
else:
    license_path.write_bytes(licenses)
print(f"PASS {len(packages)} dependency declarations")
