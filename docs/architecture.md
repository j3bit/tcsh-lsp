# Architecture

The project is split so protocol handling, parsing, analysis, and editor-facing features remain testable independently:

- `protocol`: LSP lifecycle, logging, error mapping, cancellation registry.
- `documents`: M1 document store and text sync.
- `syntax`: M2/M3 lexer, parser, CST/AST, recovery.
- `semantics`: M4 symbols, confidence-tracked references, conservative source resolver; background workspace index later.
- `features`: M5+ LSP feature providers, including M7 open-document read-only features, semantic tokens, and M8 safe edits.
- `format`: M8 conservative indentation-only formatter with off/on pragmas.
- `config`: M1 settings schema and defaults.
