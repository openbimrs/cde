#!/usr/bin/env bash
# Complete standalone verification gate for openbimrs/cde.
set -euo pipefail

cd "$(dirname "$0")/.."

tracked_reference_pdfs=$(git ls-files 'references/*.pdf' 'references/**/*.pdf')
if [[ -n "$tracked_reference_pdfs" ]]; then
    printf 'copyrighted reference PDFs must not be tracked:\n%s\n' "$tracked_reference_pdfs" >&2
    exit 1
fi

if ! cmp -s LICENSE openbim-cde/LICENSE; then
    printf 'crate and repository license files differ\n' >&2
    exit 1
fi

cargo fmt --all -- --check
cargo build --workspace --all-targets --locked
cargo test --workspace --all-features --locked
cargo clippy --workspace --all-targets --all-features --locked -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps --locked

publish_policy=$(
    cargo metadata --no-deps --locked --format-version 1 |
        python3 -c 'import json,sys; p=next(x for x in json.load(sys.stdin)["packages"] if x["name"]=="openbim-cde"); print(p["publish"])'
)
if [[ "$publish_policy" != "[]" ]]; then
    printf 'openbim-cde publication must remain disabled pending rights clearance\n' >&2
    exit 1
fi

package_files=$(cargo package -p openbim-cde --locked --allow-dirty --list)
for required in LICENSE README.md THIRD_PARTY_NOTICES.md; do
    case "$package_files" in
        *"$required"*) ;;
        *)
            printf 'package is missing required file: %s\n' "$required" >&2
            exit 1
            ;;
    esac
done
while IFS= read -r package_file; do
    case "$package_file" in
        .cargo_vcs_info.json | Cargo.lock | Cargo.toml | Cargo.toml.orig | LICENSE | README.md | THIRD_PARTY_NOTICES.md | src/*.rs | tests/*.rs) ;;
        *)
            printf 'package contains an undeclared file: %s\n' "$package_file" >&2
            exit 1
            ;;
    esac
done <<<"$package_files"
case "$package_files" in
    *"references/"*)
        printf 'package contains forbidden standards material\n' >&2
        exit 1
        ;;
esac
case "$package_files" in
    *".pdf"*)
        printf 'package contains a forbidden PDF\n' >&2
        exit 1
        ;;
esac
cargo package -p openbim-cde --locked --allow-dirty
