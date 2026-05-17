# tcsh-lsp

`tcsh-lsp` is a conservative Language Server Protocol implementation for `tcsh`/`csh` scripts.

Status: M9 release-candidate scaffold. The server currently provides a real JSON-RPC/LSP stdio lifecycle, incremental text document synchronization, conservative file detection, a lossless lexer, a tolerant parser/recovery scaffold, semantic analyzer/source resolver scaffold, publishDiagnostics, documentSymbol, definition, hover, completion, references, document highlights, folding ranges, selection ranges, semantic tokens/full, open-document workspace symbols, prepareRename/rename for certain symbols, conservative code actions, and indentation-only formatting/range formatting.

## Install and run

```sh
cargo build
cargo run -- --stdio
```

The server writes LSP frames to stdout and logs to stderr only.

Release archive and air-gapped installation details are documented in:

- [`docs/release-packaging.md`](docs/release-packaging.md)
- [`docs/offline-install.md`](docs/offline-install.md)

## Supported file detection target

The document layer detects `.csh`, `.tcsh`, `.cshrc`, `.tcshrc`, LSP language IDs (`csh`, `tcsh`), and shebang-based `tcsh`/`csh` files.

## Neovim

See [`editors/nvim/tcsh_lsp.lua`](editors/nvim/tcsh_lsp.lua) for the `vim.lsp.config()` / `vim.lsp.enable()` example and [`docs/neovim-smoke.md`](docs/neovim-smoke.md) for a headless smoke check.

## Development gates

M0 is complete only when:

- `cargo fmt --check` passes.
- `cargo test` passes, including lifecycle integration.
- README, Neovim config skeleton, and capability matrix scaffold exist.
- The server can initialize, handle open/change/save/close notifications, shut down, and exit over stdio without crashing.

## Lexer corpus

M2 adds checked-in lexer fixtures under `fixtures/corpus/{valid,invalid,incomplete}` and textual goldens under `fixtures/golden/lexer`. Regenerate intentionally with `UPDATE_GOLDENS=1 cargo test --test lexer_golden`.

## Parser corpus

M3 adds parser seed fixtures under `fixtures/corpus/parser` and textual goldens under `fixtures/golden/parser`. Regenerate intentionally with `UPDATE_GOLDENS=1 cargo test --test parser_golden`. The feasibility gate is documented in `docs/parser-feasibility-gate.md`.

## Semantic analyzer

M4 adds conservative symbol/reference extraction for variables, environment variables, aliases, labels, loop variables, goto labels, alias-like command uses, and source targets. Source resolution never executes shell code and marks dynamic paths as uncertain.

## Diagnostics and document symbols

M5 publishes conservative diagnostics on open/change and clears them on close. `textDocument/documentSymbol` returns nested symbols for block constructs and named declarations where the parser can identify them.

## Navigation and completion

M6 adds conservative same-document definition/hover over known semantic symbols and completion for builtins, symbols, and block snippets. Source-target definition resolves only statically resolved files.

## Read-only post-MVP features

M7 adds open-document `textDocument/references`, `textDocument/documentHighlight`, `textDocument/foldingRange`, `textDocument/selectionRange`, `textDocument/semanticTokens/full`, and `workspace/symbol`. Workspace symbols intentionally index open documents only until background workspace indexing lands.

## Safe edits and formatting

M8 adds `textDocument/prepareRename` and `textDocument/rename` for certain variables, aliases, and labels only. Formatting is indentation-only, preserves formatter off/on pragmas (`# tcsh-lsp format: off` / `# tcsh-lsp format: on`), skips continuation lines, and has idempotency tests.

## Coverage and limitations

- LSP support matrix: [`docs/lsp-capability-matrix.md`](docs/lsp-capability-matrix.md)
- tcsh/csh coverage matrix: [`docs/tcsh-csh-coverage-matrix.md`](docs/tcsh-csh-coverage-matrix.md)
- Known limitations and tradeoffs: [`docs/known-limitations.md`](docs/known-limitations.md)

## Release and offline installation

Use [`scripts/package-release.sh`](scripts/package-release.sh) to create a checksummed release archive. See [`docs/release-packaging.md`](docs/release-packaging.md), [`docs/offline-install.md`](docs/offline-install.md), and [`docs/known-limitations.md`](docs/known-limitations.md) before distributing or installing in restricted environments.
