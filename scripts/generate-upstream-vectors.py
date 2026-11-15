#!/usr/bin/env python3
# Copyright 2026 OpenDuo contributors
# SPDX-License-Identifier: FSL-1.1-Apache-2.0
"""Run pinned, unmodified iOS sources. No SwiftPM or vendor binary dependency is required."""
import argparse
import hashlib
import json
import pathlib
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent.parent
REVISION = "b9d91bb9b77a12c3e59e6f359af1e4537a4ded39"
FILES = ["Pairing.swift", "ByteIO.swift", "VoiceNote.swift", "PocketConstants.swift", "Surface.swift", "RedialBackoff.swift", "AmbientEdge.swift", "Sight.swift"]
parser = argparse.ArgumentParser()
parser.add_argument("--ios", type=pathlib.Path, required=True)
parser.add_argument("--check", action="store_true")
args = parser.parse_args()
build = ROOT / ".build" / "upstream-vectors"
build.mkdir(parents=True, exist_ok=True)
sources = []
hashes = {}
for name in FILES:
    source = "Packages/PocketKit/Sources/PocketCore/" + name
    data = subprocess.check_output(["git", "-C", str(args.ios), "show", REVISION + ":" + source])
    hashes[source] = hashlib.sha256(data).hexdigest()
    output = build / name
    output.write_bytes(data)
    sources.append(str(output))
binary = build / "generate"
subprocess.run(["swiftc", "-O", "-o", str(binary), *sources, str(ROOT / "tests/upstream/Support.swift"), str(ROOT / "tests/upstream/Generate.swift")], check=True)
vectors = subprocess.check_output([str(binary)])
provenance = json.dumps({"repository": "https://github.com/openduo/pocket-ios", "revision": REVISION, "sources_sha256": hashes, "harness_sha256": {name: hashlib.sha256((ROOT / "tests/upstream" / name).read_bytes()).hexdigest() for name in ["Generate.swift", "Support.swift"]}, "vectors_sha256": hashlib.sha256(vectors).hexdigest()}, indent=2, sort_keys=True).encode() + b"\n"
for name, data in [("ios-vectors.json", vectors), ("ios-provenance.json", provenance)]:
    path = ROOT / "tests/fixtures" / name
    if args.check:
        if not path.exists() or path.read_bytes() != data:
            raise SystemExit("Upstream fixture differs: " + name)
    else:
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(data)
print("PASS original Swift vector generation" + (" matches committed fixtures" if args.check else ""))
