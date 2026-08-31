#!/usr/bin/env python3
"""Fail closed when any workspace package crosses release boundaries."""
import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
COMMON = {
    ".cargo_vcs_info.json",
    "Cargo.lock",
    "Cargo.toml",
    "Cargo.toml.orig",
    "LICENSE",
    "LICENSES/MIT.txt",
    "README.md",
    "THIRD_PARTY_NOTICES.md",
}
EXPECTED = {
    "openbim-cde": COMMON | {
        "src/documents.rs",
        "src/foundation.rs",
        "src/lib.rs",
        "src/optional_non_null.rs",
        "tests/wire_contract.rs",
    },
    "openbim-cde-http": COMMON | {
        "src/client.rs",
        "src/error.rs",
        "src/lib.rs",
        "src/operation.rs",
        "src/server/documents_service.rs",
        "src/server/mod.rs",
        "src/server/router.rs",
        "src/server/types.rs",
        "tests/client_security.rs",
        "tests/client_server.rs",
        "tests/inventory.rs",
        "tests/request_context.rs",
        "tests/server_routes.rs",
    },
    "openbim-cde-oauth": COMMON | {
        "src/discovery.rs",
        "src/lib.rs",
        "src/native_keyring.rs",
        "src/oauth.rs",
        "src/reqwest_transport.rs",
        "src/secrets.rs",
        "src/store.rs",
        "tests/discovery.rs",
        "tests/oauth_execution.rs",
        "tests/secrets.rs",
        "tests/token_store.rs",
    },
    "openbim-cde-validation": COMMON | {
        "src/budget.rs",
        "src/lib.rs",
        "src/normalize.rs",
        "src/source.rs",
        "src/validator.rs",
        "tests/external_corpus.rs",
        "tests/resource_limits.rs",
        "tests/synthetic.rs",
    },
}
REQUIRED = {"LICENSE", "LICENSES/MIT.txt", "README.md", "THIRD_PARTY_NOTICES.md"}

def run(*args: str) -> str:
    return subprocess.run(args, cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE).stdout

metadata = json.loads(run("cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"))
failures: list[str] = []
packages = {package["name"]: package for package in metadata["packages"]}
if set(packages) != set(EXPECTED):
    failures.append(
        f"workspace packages differ: expected {sorted(EXPECTED)}, got {sorted(packages)}"
    )

for name, expected_files in EXPECTED.items():
    package = packages.get(name)
    if package is None:
        continue
    crate_dir = Path(package["manifest_path"]).parent
    expected_dir = ROOT / name
    if crate_dir.resolve() != expected_dir.resolve():
        failures.append(f"{name}: manifest must be at {expected_dir.relative_to(ROOT)}")
    if package["publish"] != []:
        failures.append(f"{name}: publication must remain disabled")
    if package["edition"] != "2021":
        failures.append(f"{name}: edition must remain 2021")
    if package["rust_version"] != "1.85":
        failures.append(f"{name}: rust-version must remain 1.85")
    if package["license"] != "AGPL-3.0-or-later":
        failures.append(f"{name}: license metadata must be AGPL-3.0-or-later")
    if not (crate_dir / "LICENSE").is_file():
        failures.append(f"{name}: LICENSE is missing")
    elif (crate_dir / "LICENSE").read_bytes() != (ROOT / "LICENSE").read_bytes():
        failures.append(f"{name}: LICENSE differs from repository license")

    files = set(
        run("cargo", "package", "-p", name, "--locked", "--allow-dirty", "--list").splitlines()
    )
    missing_required = REQUIRED - files
    if missing_required:
        failures.append(f"{name}: missing required files: {sorted(missing_required)}")
    missing = sorted(expected_files - files)
    unexpected = sorted(files - expected_files)
    if missing:
        failures.append(f"{name}: package allowlist files missing: {missing}")
    if unexpected:
        failures.append(f"{name}: undeclared package files: {unexpected}")
    if any("references/" in path or path.lower().endswith(".pdf") for path in files):
        failures.append(f"{name}: forbidden standards material is packaged")

if failures:
    print("\n".join(failures), file=sys.stderr)
    raise SystemExit(1)
