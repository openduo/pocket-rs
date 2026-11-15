#!/usr/bin/env python3
# Copyright 2026 OpenDuo contributors
# SPDX-License-Identifier: FSL-1.1-Apache-2.0
"""Install into a dedicated tmpfs prefix and execute native Linux/C consumers over ADB."""
import argparse
import json
import pathlib
import shlex
import subprocess

ROOT = pathlib.Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument("--serial", required=True)
parser.add_argument("--bundle", type=pathlib.Path, required=True)
parser.add_argument("--prefix", default="/var/tmp/pocket-rs-0.1.1")
args = parser.parse_args()
if not args.prefix.startswith(("/var/tmp/pocket-rs-", "/tmp/pocket-rs-")) or ".." in pathlib.PurePosixPath(args.prefix).parts:
    raise SystemExit("Use a dedicated temporary pocket-rs prefix")
evidence = ROOT / ".build" / "arm64-device"
evidence.mkdir(parents=True, exist_ok=True)
adb = ["adb", "-s", args.serial]

def shell(command, *, save=None):
    result = subprocess.run([*adb, "shell", command], stdout=subprocess.PIPE, stderr=subprocess.STDOUT, check=True)
    if save:
        (evidence / save).write_bytes(result.stdout)
    return result.stdout.decode()

machine = shell("uname -m").strip()
if machine != "aarch64":
    raise SystemExit("Physical target is not native aarch64")
shell("uname -a; cat /etc/os-release; ldd --version | head -1; df -h /tmp /var", save="environment.log")
stage = "/tmp/pocket-rs-sdk-acceptance"
shell("mkdir -p " + shlex.quote(stage))
subprocess.run([*adb, "push", str(args.bundle.resolve()) + "/.", stage + "/"], check=True, stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
bundle = shlex.quote(stage)
prefix = shlex.quote(args.prefix)
shell(f"sh {bundle}/install.sh --prefix {prefix}", save="installation.log")
native = shell(f"{prefix}/bin/pocket self-test", save="protocol.log")
report = json.loads(native.strip().splitlines()[-1])
if report.get("status") != "pass" or report.get("os") != "linux" or report.get("arch") != "aarch64":
    raise SystemExit("Native protocol acceptance failed")
shell(f"{prefix}/bin/pocket-c-smoke", save="installed-c.log")
command = f"cc -std=c11 -Wall -Wextra -Werror -I{prefix}/include {prefix}/share/pocket-rs/examples/c/consumer.c -L{prefix}/lib -lpocket_core -Wl,-rpath,{prefix}/lib -o {prefix}/bin/pocket-c-native && {prefix}/bin/pocket-c-native"
shell(command, save="native-c.log")
shell(f"readelf -h {prefix}/lib/libpocket_core.so; readelf -d {prefix}/lib/libpocket_core.so; ldd {prefix}/lib/libpocket_core.so; sha256sum {prefix}/lib/libpocket_core.so {prefix}/bin/pocket; df -h /tmp /var", save="elf-and-dependencies.log")
shell("rm -rf " + shlex.quote(stage))
print(json.dumps({"status": "pass", "os": "linux", "arch": machine, "prefix": args.prefix, "volatile_installation": True, "checks": ["bundle_checksums", "installed_native_protocol_loopback", "installed_dynamic_library", "consumer_compiled_on_physical_target", "ELF_dependencies"], "private_evidence": str(evidence.relative_to(ROOT))}))
