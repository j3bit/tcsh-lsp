# tcsh/csh Coverage Matrix

| Area | Status | Evidence target |
|---|---:|---|
| File extension / rc-file / language ID / shebang detection | Implemented M1 | Detection tests |
| Lexical categories | Implemented M2 | Lexer golden fixtures for valid/invalid/incomplete cases |
| Core commands and blocks | Implemented M3 scaffold | Parser golden fixtures and seed-corpus no-panic tests |
| Symbols and source resolution | Implemented M4 scaffold | Analyzer/resolver tests with confidence model |
| Conservative diagnostics | Implemented M5 scaffold | Diagnostics unit tests; golden expansion later |
| Navigation, hover, completion | Implemented M6 scaffold | Unit tests and protocol smoke |
| Read-only cross-reference features | Implemented M7 scaffold | References, highlights, open-document workspace symbols, folding, selection range, semantic token tests |
| Safe edits and formatting | Implemented M8 scaffold | PrepareRename/rename, code action, formatting idempotency and pragma tests |
