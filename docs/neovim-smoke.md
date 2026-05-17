# Neovim Smoke Test

The integration example lives at `editors/nvim/tcsh_lsp.lua` and uses the
Neovim 0.11+ `vim.lsp.config()` / `vim.lsp.enable()` API.

Manual smoke:

```sh
cargo build
XDG_STATE_HOME="$PWD/target/nvim-smoke/state" \
XDG_DATA_HOME="$PWD/target/nvim-smoke/data" \
XDG_CACHE_HOME="$PWD/target/nvim-smoke/cache" \
PATH="$PWD/target/debug:$PATH" \
  nvim --headless -u NONE \
    -c 'luafile editors/nvim/tcsh_lsp.lua' \
    -c 'quit'
```

Open a `.tcsh`, `.csh`, `.tcshrc`, or `.cshrc` file and run:

```vim
:checkhealth vim.lsp
:lua print(vim.inspect(vim.lsp.get_clients({ name = 'tcsh_lsp' })))
```

The expected result is one active `tcsh_lsp` client for detected tcsh/csh
buffers. The server communicates over stdio and writes logs to stderr, so LSP
framing remains isolated on stdout.
