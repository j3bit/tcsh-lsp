# LSP Capability Matrix

Status values: Implemented, Partial, Planned, Unsupported.

| Capability | Status | Advertised in M0? | Gate |
|---|---:|---:|---|
| JSON-RPC stdio framing | Implemented | n/a | Lifecycle integration test |
| initialize / initialized | Implemented | n/a | Lifecycle integration test |
| shutdown / exit | Implemented | n/a | Lifecycle integration test |
| cancellation handling | Partial | n/a | Registry stub exists; provider checks later |
| textDocument synchronization | Implemented M1 | Yes | UTF-16 incremental/full sync tests and protocol smoke |
| publishDiagnostics | Implemented M5 scaffold | Yes | Unit diagnostic tests and protocol publish on open/change |
| documentSymbol | Implemented M5 scaffold | Yes | Unit symbol hierarchy tests and protocol smoke |
| definition / hover / completion | Implemented M6 scaffold | Yes | Unit tests and protocol smoke |
| references | Implemented M7 scaffold | Yes | Open-document semantic query tests and protocol smoke |
| documentHighlight | Implemented M7 scaffold | Yes | Open-document semantic query tests and protocol smoke |
| foldingRange | Implemented M7 scaffold | Yes | Parser block folding tests and protocol smoke |
| selectionRange | Implemented M7 scaffold | Yes | Parser node selection tests and protocol smoke |
| semanticTokens/full | Implemented M7 scaffold | Yes | Lexer-token semantic classification tests and protocol smoke |
| workspace/symbol | Partial M7 | Yes | Open-document symbol index only; background workspace scan remains planned |
| formatting / rangeFormatting | Implemented M8 scaffold | Yes | Indentation-only edits with idempotency and pragma tests |
| rename / prepareRename / codeAction | Implemented M8 scaffold | Yes | Certain variables/aliases/labels only; conservative code-action tests |
| codeLens / inlayHint | Planned | No | Only if low-noise tcsh concepts are identified |
| callHierarchy / workspace diagnostic | Planned | No | Requires workspace graph/index |
| typeDefinition / implementation | Unsupported initially | No | Only add if meaningful tcsh concept is defined |
