# Parser Feasibility Gate

Status: initial M3 gate passed for the current tolerant parser scaffold.

Evidence:

- Minimum seed corpus exists under `fixtures/corpus/parser/`:
  - `valid/`: at least 10 files
  - `invalid/`: at least 10 files
  - `incomplete/`: at least 10 files
  - `source-resolution/`: at least 5 files
  - `dialect/`: paired tcsh/csh starter fixtures
- Checked-in parser goldens exist under `fixtures/golden/parser/`.
- `tests/parser_golden.rs` verifies golden parse output, seed-corpus counts, no-panic parsing over the seed corpus, incomplete-block recovery, and the first vertical slice shape (`set foo = bar` plus `$foo` reference command).

This gate is intentionally conservative. M4/M5 may continue only if this file and the tests stay current when parser coverage expands.
