# Known Limitations

- The parser is intentionally tolerant and conservative. It supports the core tcsh/csh forms in the corpus, but it is not a byte-for-byte implementation of every shell grammar ambiguity.
- Workspace features currently index open documents only. Background filesystem indexing is still configuration-gated and planned after the request surface is stable.
- Diagnostics intentionally prefer silence over noisy false positives; some ambiguous alias, variable, glob, and source-path cases are reported as uncertain or skipped.
- Some semantic diagnostics, including unresolved `goto`, are intentionally conservative by default. The analyzer still records labels and goto references for navigation, but publishDiagnostics avoids noisy reachability/context guesses unless a stricter category is added later.
- Cancellation is represented by a registry consulted by expensive providers; `tower-lsp` does not expose every LSP cancellation path directly in the current integration.
- Formatting is indentation-only. It does not normalize quoting, command layout, redirection spacing, or complex command continuations.
- Rename is limited to certain variables, aliases, and labels in open/indexed documents. It deliberately rejects ambiguous or invalid shell names.
- codeLens and inlayHint are not advertised yet because no low-noise tcsh/csh subset has been proven useful enough to enable by default.
