# Offline / Air-Gapped Installation Notes

The preferred air-gapped artifact is the release archive from
[`release-packaging.md`](release-packaging.md). It contains the compiled binary,
documentation, and Neovim example; it does not require Cargo or network access
on the target host.

## Prepare outside the restricted environment

```sh
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
./scripts/package-release.sh
```

Transfer both files:

```text
dist/tcsh-lsp-<version>-<target-triple>.tar.gz
dist/tcsh-lsp-<version>-<target-triple>.tar.gz.sha256
```

## Verify and install inside the restricted environment

```sh
shasum -a 256 -c tcsh-lsp-<version>-<target-triple>.tar.gz.sha256
tar -xzf tcsh-lsp-<version>-<target-triple>.tar.gz
install -m 0755 tcsh-lsp-<version>-<target-triple>/bin/tcsh-lsp ~/.local/bin/tcsh-lsp
~/.local/bin/tcsh-lsp --version
```

Then point the editor integration at `~/.local/bin/tcsh-lsp`.

## Source-vendored fallback

If a target policy requires building inside the restricted environment, vendor
dependencies outside first:

```sh
cargo vendor vendor/
```

Transfer the repository plus `vendor/`, configure Cargo to use the vendor
directory, and run the same verification gate before installing the release
binary. Prefer the binary archive when target libc/toolchain compatibility is
known.
