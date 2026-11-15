#!/usr/bin/env python3
# Copyright 2026 OpenDuo contributors
# SPDX-License-Identifier: FSL-1.1-Apache-2.0
"""Package verified dual-ABI JNI libraries with Java bindings as a standard Android AAR."""
import argparse
import gzip
import hashlib
from pathlib import Path
import shutil
import subprocess
import tarfile
import tomllib
import zipfile

root = Path(__file__).resolve().parent.parent
parser = argparse.ArgumentParser()
parser.add_argument('--jni', type=Path, default=root / '.build/android/jni')
args = parser.parse_args()
version = tomllib.loads((root / 'Cargo.toml').read_text())['workspace']['package']['version']
build = root / '.build/android-package'
shutil.rmtree(build, ignore_errors=True)
(build / 'classes').mkdir(parents=True)
subprocess.run(['javac', '--release', '8', '-d', str(build / 'classes'), str(root / 'bindings/java/src/main/java/dev/openduo/pocket/protocol/NativeCore.java')], check=True)
def archive(path, files):
    with zipfile.ZipFile(path, 'w', zipfile.ZIP_DEFLATED) as output:
        for name, data in sorted(files.items()):
            info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
            info.compress_type = zipfile.ZIP_DEFLATED
            output.writestr(info, data)
archive(build / 'classes.jar', {str(p.relative_to(build / 'classes')):p.read_bytes() for p in (build / 'classes').rglob('*.class')})
files = {
    'classes.jar': (build / 'classes.jar').read_bytes(),
    'AndroidManifest.xml': b'<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="dev.openduo.pocket.protocol"><uses-sdk android:minSdkVersion="26"/></manifest>',
    'R.txt': b'',
    'proguard.txt': (root / 'bindings/java/consumer-rules.pro').read_bytes(),
}
for abi in ['arm64-v8a', 'x86_64']:
    for name in ['libpocket_core.so', 'libpocket_jni.so']:
        files[f'jni/{abi}/{name}'] = (args.jni / abi / name).read_bytes()
for name in ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_DEPENDENCIES.json', 'THIRD_PARTY_LICENSES.txt']:
    files[f'assets/pocket-rs/licenses/{name}'] = (root / name).read_bytes()
dist = root / 'dist'
dist.mkdir(exist_ok=True)
aar = dist / f'pocket-core-{version}.aar'
archive(aar, files)
aar.with_suffix('.aar.sha256').write_text(f'{hashlib.sha256(aar.read_bytes()).hexdigest()}  {aar.name}\n')
bundle = dist / f'pocket-rs-{version}-android-sdk'
shutil.rmtree(bundle, ignore_errors=True)
bundle.mkdir()
shutil.copy2(aar, bundle / aar.name)
for directory in ['bindings', 'include', 'docs', 'examples/java']:
    shutil.copytree(root / directory, bundle / directory)
for name in ['LICENSE', 'NOTICE', 'THIRD_PARTY_NOTICES.md', 'THIRD_PARTY_DEPENDENCIES.json', 'THIRD_PARTY_LICENSES.txt']:
    shutil.copy2(root / name, bundle / name)
(bundle / 'SHA256SUMS').write_text(''.join(f'{hashlib.sha256(p.read_bytes()).hexdigest()}  {p.relative_to(bundle)}\n' for p in sorted(bundle.rglob('*')) if p.is_file()))
output = bundle.with_name(bundle.name + '.tar.gz')
with output.open('wb') as raw, gzip.GzipFile(filename='', mode='wb', fileobj=raw, mtime=0) as compressed, tarfile.open(fileobj=compressed, mode='w') as tar:
    for p in sorted(bundle.rglob('*')):
        info = tar.gettarinfo(str(p), arcname=str(p.relative_to(bundle.parent)))
        info.uid = info.gid = info.mtime = 0
        info.uname = info.gname = ''
        if p.is_file():
            with p.open('rb') as source: tar.addfile(info, source)
        else: tar.addfile(info)
output.with_name(output.name + '.sha256').write_text(f'{hashlib.sha256(output.read_bytes()).hexdigest()}  {output.name}\n')
print(f'Packaged {aar.name} and {output.name}')
