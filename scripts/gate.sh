#!/usr/bin/env bash
# Rust build/test/documentation gate. Dependency policy runs separately through
# cargo-deny in CI and the release procedure.
set -euo pipefail

cd "$(dirname "$0")/.."

tracked_reference_pdfs=$(git ls-files 'references/*.pdf' 'references/**/*.pdf')
if [[ -n "$tracked_reference_pdfs" ]]; then
    printf 'copyrighted reference PDFs must not be tracked:\n%s\n' "$tracked_reference_pdfs" >&2
    exit 1
fi

python3 scripts/check-packages.py

cargo fmt --all -- --check
cargo build --workspace --all-targets --all-features --locked
cargo test --workspace --all-targets --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked

cargo check -p openbim-cde-oauth --locked --no-default-features
cargo check -p openbim-cde-oauth --locked --no-default-features --features reqwest
cargo check -p openbim-cde-oauth --locked --no-default-features --features native-keyring
cargo check -p openbim-cde-oauth --locked --no-default-features --features reqwest,native-keyring
