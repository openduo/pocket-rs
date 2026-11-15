# Third-party dependencies

Pocket Rust derives room-protocol behavior from the FSL-1.1-Apache-2.0 OpenDuo iOS/Android projects
identified in NOTICE. Test-vector generation extracts pinned source into ignored build storage;
no modified upstream source or proprietary firmware is bundled.

Cargo.lock is the exact dependency graph for this release. `scripts/dependency-notices.py`
generates `THIRD_PARTY_DEPENDENCIES.json`, recording each resolved registry package's version,
declared license and repository. `THIRD_PARTY_LICENSES.txt` preserves the resolved packages' original
license/notice texts and is bundled with binary distributions. Core dependencies
include serde, serde_json, url, percent-encoding, sha2 and thiserror. The optional reference client
adds reqwest, tungstenite, rustls and clap. No Go package is linked.

Cargo.lock also contains UEFI/WASI-only `r-efi`/`wasip2` packages; these do not enter the supported
Linux/Apple binaries. Their target-only declarations are identified separately in the license list.
