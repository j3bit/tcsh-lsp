#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

version="$(awk -F' *= *' '/^version/ {gsub(/"/, "", $2); print $2; exit}' Cargo.toml)"
target_triple="${TARGET:-$(rustc -vV | awk '/host:/ {print $2}')}"
archive_dir="dist/tcsh-lsp-${version}-${target_triple}"
archive="dist/tcsh-lsp-${version}-${target_triple}.tar.gz"

cargo build --release
rm -rf "$archive_dir"
mkdir -p "$archive_dir/bin" "$archive_dir/docs" "$archive_dir/editors/nvim"

cp target/release/tcsh-lsp "$archive_dir/bin/"
cp README.md Cargo.lock Cargo.toml LICENSE-* "$archive_dir/"
cp docs/*.md "$archive_dir/docs/"
cp editors/nvim/tcsh_lsp.lua "$archive_dir/editors/nvim/"

(
  cd dist
  COPYFILE_DISABLE=1 tar -czf "$(basename "$archive")" "$(basename "$archive_dir")"
  shasum -a 256 "$(basename "$archive")" > "$(basename "$archive").sha256"
)

printf 'Created %s and %s.sha256\n' "$archive" "$archive"
