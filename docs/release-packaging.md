# Release Packaging

Release artifacts are intentionally simple: one native `tcsh-lsp` binary plus
the user-facing documentation and editor integration examples.

## Verification gate

Run this gate before creating an archive:

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo run -- --version
XDG_STATE_HOME="$PWD/target/nvim-smoke/state" \
XDG_DATA_HOME="$PWD/target/nvim-smoke/data" \
XDG_CACHE_HOME="$PWD/target/nvim-smoke/cache" \
  nvim --headless -u NONE -c 'luafile editors/nvim/tcsh_lsp.lua' -c 'quit'
```

## Build an archive

```sh
./scripts/package-release.sh
```

The script creates:

- `dist/tcsh-lsp-<version>-<target-triple>.tar.gz`
- `dist/tcsh-lsp-<version>-<target-triple>.tar.gz.sha256`

Archive layout:

```text
tcsh-lsp-<version>-<target-triple>/
  bin/tcsh-lsp
  README.md
  Cargo.toml
  Cargo.lock
  LICENSE-APACHE
  LICENSE-MIT
  docs/*.md
  editors/nvim/tcsh_lsp.lua
```

## Install from an archive

```sh
tar -xzf tcsh-lsp-<version>-<target-triple>.tar.gz
install -m 0755 tcsh-lsp-<version>-<target-triple>/bin/tcsh-lsp ~/.local/bin/tcsh-lsp
tcsh-lsp --version
```

Keep the `.sha256` file with the archive so recipients can verify transfer
integrity before installation.
