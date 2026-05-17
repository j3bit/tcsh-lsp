-- Minimal Neovim 0.11+ example. Adjust cmd to the installed binary path.
vim.filetype.add({
  extension = {
    csh = 'tcsh',
    tcsh = 'tcsh',
  },
  filename = {
    ['.cshrc'] = 'tcsh',
    ['.tcshrc'] = 'tcsh',
  },
})

vim.lsp.config('tcsh_lsp', {
  cmd = { 'tcsh-lsp', '--stdio' },
  filetypes = { 'tcsh' },
  root_markers = { '.git' },
  settings = {
    tcshLsp = {
      dialect = 'auto',
    },
  },
})

vim.lsp.enable('tcsh_lsp')
