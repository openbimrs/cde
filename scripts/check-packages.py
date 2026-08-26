#!/usr/bin/env python3
"""Fail closed when any workspace package crosses release boundaries."""
import json
from pathlib import Path
import re
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
ALLOWED = re.compile(r"^(?:\.cargo_vcs_info\.json|Cargo\.lock|Cargo\.toml|Cargo\.toml\.orig|LICENSE|README\.md|THIRD_PARTY_NOTICES\.md|src/.+\.rs|tests/.+\.rs)$")
REQUIRED = {"LICENSE", "README.md", "THIRD_PARTY_NOTICES.md"}

def run(*args: str) -> str:
    return subprocess.run(args, cwd=ROOT, check=True, text=True, stdout=subprocess.PIPE).stdout

metadata = json.loads(run("cargo", "metadata", "--no-deps", "--locked", "--format-version", "1"))
failures: list[str] = []
for package in metadata["packages"]:
    name = package["name"]
    crate_dir = Path(package["manifest_path"]).parent
    if package["publish"] != []:
        failures.append(f"{name}: publication must remain disabled")
    if not (crate_dir / "LICENSE").is_file():
        failures.append(f"{name}: LICENSE is missing")
    elif (crate_dir / "LICENSE").read_bytes() != (ROOT / "LICENSE").read_bytes():
        failures.append(f"{name}: LICENSE differs from repository license")
    files = set(run("cargo", "package", "-p", name, "--locked", "--allow-dirty", "--list").splitlines())
    missing = REQUIRED - files
    if missing:
        failures.append(f"{name}: missing package files: {sorted(missing)}")
    invalid = sorted(path for path in files if not ALLOWED.fullmatch(path))
    if invalid:
        failures.append(f"{name}: undeclared package files: {invalid}")
    if any("references/" in path or path.lower().endswith(".pdf") for path in files):
        failures.append(f"{name}: forbidden standards material is packaged")
    run("cargo", "package", "-p", name, "--locked", "--allow-dirty")

if failures:
    print("\n".join(failures), file=sys.stderr)
    raise SystemExit(1)
