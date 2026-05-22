# Dogfood Diagnostics Zero Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the copied tree-sitter tcsh examples report zero tcsh-lsp diagnostics while preserving server stability and conservative behavior.

**Architecture:** Add tests first around the two copied fixtures, then remove the specific false/noisy diagnostics by improving lexer/parser/semantic modeling. Do not add broad full-tcsh grammar work; constrain changes to history expansion tokenization, separator handling inside expressions, arithmetic assignment recognition, predefined variable handling, and default diagnostic policy for unresolved `goto`.

**Tech Stack:** Rust, `tower-lsp`, current tcsh-lsp lexer/parser/semantic analyzer, existing `cargo test` integration suite.

---

## Scope and stop condition

The fixtures copied from `~/Dev/tree-sitter-tcsh/examples` are:

- `fixtures/corpus/parser/valid/tree_sitter_sample.tcsh`
- `fixtures/corpus/parser/valid/tree_sitter_showcase.tcsh`

Stop when all are true:

1. `diagnostics_for_text()` returns an empty vector for both files.
2. Opening both files over stdio LSP publishes empty diagnostics and still answers `documentSymbol`, `foldingRange`, and `semanticTokens/full`.
3. Existing intentional diagnostics tests still prove real parse/lex/form errors are reported.
4. `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, and `cargo test` pass.

Non-goals:

- Do not implement full tcsh history expansion semantics.
- Do not execute tcsh scripts.
- Do not make formatter or rename behavior broader.
- Do not hardcode user/company/domain paths or command dictionaries.

## File structure

- Keep: `fixtures/corpus/parser/valid/tree_sitter_sample.tcsh`
  - Exact copied dogfood input.
- Keep: `fixtures/corpus/parser/valid/tree_sitter_showcase.tcsh`
  - Exact copied dogfood input.
- Modify: `src/features/diagnostics.rs`
  - Add dogfood zero-diagnostic regression tests.
  - Make unresolved `goto` non-published by default, because the analyzer can still model references but day-to-day diagnostics should avoid noisy semantic certainty claims.
- Modify: `src/syntax/lexer.rs`
  - Treat tcsh history expansions such as `!!`, `!$`, `!:1`, and event references as lexical words/history-like tokens instead of splitting at `$` and emitting malformed variable diagnostics.
- Modify: `src/syntax/parser.rs`
  - Make command segmentation parenthesis-aware so `&&` / `||` inside `if ( ... ) then` conditions do not split the `if` opener.
- Modify: `src/semantics/mod.rs`
  - Record `@ name = ...`, `@ name++`, `@ name--`, `@ name += ...`, `@ name -= ...` as shell-variable assignments.
  - Suppress “used before assignment” for built-in/special tcsh variables such as `argv`, `argc`, `status`, `cwd`, `owd`, `home`, `shell`, `user`, and `term`.
- Optionally modify: `tests/protocol_lifecycle.rs`
  - Add a dogfood LSP smoke test for the two fixture files if unit-level diagnostics tests are not considered enough.

---

### Task 1: Add zero-diagnostic dogfood regression test

**Files:**
- Modify: `src/features/diagnostics.rs`
- Read: `fixtures/corpus/parser/valid/tree_sitter_sample.tcsh`
- Read: `fixtures/corpus/parser/valid/tree_sitter_showcase.tcsh`

- [ ] **Step 1: Write the failing test**

Append this test inside `#[cfg(test)] mod tests` in `src/features/diagnostics.rs`:

```rust
    #[test]
    fn tree_sitter_dogfood_examples_are_quiet() {
        for path in [
            "fixtures/corpus/parser/valid/tree_sitter_sample.tcsh",
            "fixtures/corpus/parser/valid/tree_sitter_showcase.tcsh",
        ] {
            let text = std::fs::read_to_string(path).expect("read dogfood fixture");
            let diagnostics = diagnostics_for_text(&text, &context());
            assert!(
                diagnostics.is_empty(),
                "{path} produced diagnostics: {diagnostics:#?}"
            );
        }
    }
```

- [ ] **Step 2: Run test and verify current failure**

Run:

```bash
cargo test features::diagnostics::tests::tree_sitter_dogfood_examples_are_quiet -- --nocapture
```

Expected before fixes: FAIL with diagnostics matching the current dogfood run, including `variable_used_before_assignment`, `parse.unmatched_end`, `lex.error`, and `unresolved_goto`.

- [ ] **Step 3: Commit only the copied fixtures and failing test if using commit checkpoints**

```bash
git add fixtures/corpus/parser/valid/tree_sitter_sample.tcsh \
        fixtures/corpus/parser/valid/tree_sitter_showcase.tcsh \
        src/features/diagnostics.rs
git commit -m "test: add tcsh dogfood examples"
```

If working without intermediate commits, keep these files unstaged until the final validation.

---

### Task 2: Fix history expansion lexing noise

**Files:**
- Modify: `src/syntax/lexer.rs`
- Modify: `tests/lexer_golden.rs` or add unit test under `src/syntax/lexer.rs`

- [ ] **Step 1: Add focused lexer test**

Add this unit test under `#[cfg(test)]` in `src/syntax/lexer.rs`. If the file has no test module yet, create one at the bottom.

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn history_expansions_do_not_emit_malformed_variable_errors() {
        let result = lex("echo !$ !! !:1 !-2 %1\n");
        assert!(
            result.errors.is_empty(),
            "history/job syntax should not produce lex errors: {:#?}",
            result.errors
        );
        assert!(result.tokens.iter().any(|token| token.text == "!$"));
        assert!(result.tokens.iter().any(|token| token.text == "!!"));
        assert!(result.tokens.iter().any(|token| token.text == "!:1"));
    }
}
```

If `src/syntax/lexer.rs` already has a `tests` module when implementing, merge the test into the existing module instead of creating a duplicate.

- [ ] **Step 2: Run test and verify failure**

```bash
cargo test syntax::lexer::tests::history_expansions_do_not_emit_malformed_variable_errors -- --nocapture
```

Expected before fix: FAIL because `!$` is split and `$` becomes a malformed variable expansion.

- [ ] **Step 3: Implement minimal history-token consumption**

In `TokenKind`, prefer adding a distinct token if the semantic-token mapping can tolerate it:

```rust
HistoryExpansion,
```

In `Lexer::run`, add a branch before the `_ => self.consume_word_or_label(start)` branch:

```rust
'!' => self.consume_history_expansion(start),
```

Add this method to `impl Lexer<'_>`:

```rust
    fn consume_history_expansion(&mut self, start: usize) {
        self.bump_char();
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace()
                || matches!(
                    ch,
                    '#' | ';' | '&' | '|' | '<' | '>' | '(' | ')' | '\'' | '"' | '`' | '\\'
                )
            {
                break;
            }
            self.bump_char();
        }
        self.push(TokenKind::HistoryExpansion, start, self.cursor);
        self.at_command_start = false;
    }
```

If adding a new token kind causes broad match updates, use `TokenKind::Word` in `push()` instead:

```rust
        self.push(TokenKind::Word, start, self.cursor);
```

The accepted minimal result is no `LexErrorCode::MalformedVariableExpansion` for `!$`, `!!`, or `!:1`.

- [ ] **Step 4: Run focused and dogfood tests**

```bash
cargo test syntax::lexer::tests::history_expansions_do_not_emit_malformed_variable_errors -- --nocapture
cargo test features::diagnostics::tests::tree_sitter_dogfood_examples_are_quiet -- --nocapture
```

Expected after this task: the malformed-variable diagnostics on the history lines are gone; other dogfood diagnostics may remain.

---

### Task 3: Make parser segmentation parenthesis-aware

**Files:**
- Modify: `src/syntax/parser.rs`

- [ ] **Step 1: Add parser regression test**

Add this test to the existing `#[cfg(test)]` parser test module if present, or create one at the bottom of `src/syntax/parser.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_condition_with_and_separator_inside_parentheses_stays_one_block() {
        let parsed = parse("if ( -e ~/.tcshrc && $count >= 3 ) then\n  echo ok\nendif\n");
        assert!(
            parsed.diagnostics.is_empty(),
            "if condition should not be split at && inside parentheses: {:#?}",
            parsed.diagnostics
        );
        assert!(matches!(parsed.root.children[0].kind, NodeKind::IfBlock));
    }
}
```

If `src/syntax/parser.rs` already has a `tests` module when implementing, merge this test into it.

- [ ] **Step 2: Run test and verify failure**

```bash
cargo test syntax::parser::tests::if_condition_with_and_separator_inside_parentheses_stays_one_block -- --nocapture
```

Expected before fix: FAIL with unmatched/missing block behavior because `&&` splits the `if` opener.

- [ ] **Step 3: Update `command_segments()`**

Modify `command_segments()` so separators only terminate a segment when parenthesis depth is zero. The shape should be:

```rust
fn command_segments(tokens: &[Token]) -> Vec<Segment<'_>> {
    let mut segments = Vec::new();
    let mut current = Vec::new();
    let mut last_end = 0;
    let mut paren_depth = 0usize;

    for token in tokens {
        match token.kind {
            TokenKind::LeftParen => {
                paren_depth += 1;
                current.push(token);
            }
            TokenKind::RightParen => {
                paren_depth = paren_depth.saturating_sub(1);
                current.push(token);
            }
            TokenKind::Newline | TokenKind::Separator if paren_depth == 0 => {
                let filtered = trim_trivia(current);
                if !filtered.is_empty() {
                    segments.push(Segment {
                        tokens: filtered,
                        end: token.span.end,
                    });
                }
                current = Vec::new();
                last_end = token.span.end;
            }
            TokenKind::Whitespace | TokenKind::Comment => current.push(token),
            _ => current.push(token),
        }
    }

    let filtered = trim_trivia(current);
    if !filtered.is_empty() {
        let end = filtered
            .last()
            .map(|token| token.span.end)
            .unwrap_or(last_end);
        segments.push(Segment { tokens: filtered, end });
    }
    segments
}
```

- [ ] **Step 4: Run parser and dogfood tests**

```bash
cargo test syntax::parser::tests::if_condition_with_and_separator_inside_parentheses_stays_one_block -- --nocapture
cargo test features::diagnostics::tests::tree_sitter_dogfood_examples_are_quiet -- --nocapture
```

Expected after this task: `parse.unmatched_end` diagnostics caused by `if ( ... && ... ) then` disappear. Other dogfood diagnostics may remain.

---

### Task 4: Recognize tcsh arithmetic assignment symbols and built-in variables

**Files:**
- Modify: `src/semantics/mod.rs`
- Modify: `src/features/diagnostics.rs`

- [ ] **Step 1: Add semantic analyzer test for `@` assignments**

Add this test to the existing `#[cfg(test)] mod tests` in `src/semantics/mod.rs`:

```rust
    #[test]
    fn records_arithmetic_assignment_symbols() {
        let parsed = parse("@ count = 1 + 2\n@ count++\necho $count\n");
        let context =
            AnalysisContext::new(std::env::current_dir().unwrap(), TcshLspConfig::default());
        let model = analyze(&parsed, &context);
        assert!(model.symbols.iter().any(|symbol| {
            symbol.kind == SymbolKind::ShellVariable && symbol.name == "count"
        }));
    }
```

- [ ] **Step 2: Add diagnostics test for predefined variables**

Add this test to `src/features/diagnostics.rs` tests:

```rust
    #[test]
    fn predefined_tcsh_variables_do_not_require_assignment() {
        let text = "echo $argv[1] $argc $status $cwd $home\n";
        let diagnostics = diagnostics_for_text(text, &context());
        assert!(
            diagnostics.iter().all(|diagnostic| {
                diagnostic
                    .code
                    .as_ref()
                    .is_none_or(|code| !matches!(code, NumberOrString::String(value) if value.contains("variable_used_before_assignment")))
            }),
            "predefined variables should not produce used-before-assignment diagnostics: {diagnostics:#?}"
        );
    }
```

If the current Rust version does not support `Option::is_none_or`, write the predicate with `match` instead.

- [ ] **Step 3: Run tests and verify failure**

```bash
cargo test semantics::tests::records_arithmetic_assignment_symbols -- --nocapture
cargo test features::diagnostics::tests::predefined_tcsh_variables_do_not_require_assignment -- --nocapture
```

Expected before fix: arithmetic symbol test fails; predefined variable diagnostics test may fail for `argv`/other lowercase predefined variables.

- [ ] **Step 4: Implement arithmetic assignment recognition**

In `visit_node()` in `src/semantics/mod.rs`, add a case before the generic `NodeKind::Command { name } if !name.is_empty()` arm:

```rust
        NodeKind::Command { name } if name == "@" => {
            record_arithmetic_assignment(node, model);
        }
```

Add this helper:

```rust
fn record_arithmetic_assignment(node: &Node, model: &mut SemanticModel) {
    if let Some((name, span)) = nth_significant_word(node, 1) {
        let normalized = name
            .trim_end_matches("++")
            .trim_end_matches("--")
            .to_string();
        if is_shell_identifier(&normalized) {
            model.symbols.push(Symbol {
                name: normalized,
                kind: SymbolKind::ShellVariable,
                span,
                confidence: Confidence::Certain,
            });
        }
    }
}

fn is_shell_identifier(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch == '_' || ch.is_ascii_alphabetic())
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}
```

- [ ] **Step 5: Implement predefined-variable diagnostic exemption**

In `src/features/diagnostics.rs`, add:

```rust
fn is_predefined_tcsh_variable(name: &str) -> bool {
    matches!(
        name,
        "argv"
            | "argc"
            | "status"
            | "cwd"
            | "owd"
            | "home"
            | "shell"
            | "user"
            | "term"
            | "version"
            | "prompt"
    )
}
```

Then update the used-before-assignment condition:

```rust
        if is_conservative_shell_var_name(&reference.name)
            && !is_predefined_tcsh_variable(&reference.name)
            && !assigned_vars.contains(&reference.name)
        {
```

- [ ] **Step 6: Run focused and dogfood tests**

```bash
cargo test semantics::tests::records_arithmetic_assignment_symbols -- --nocapture
cargo test features::diagnostics::tests::predefined_tcsh_variables_do_not_require_assignment -- --nocapture
cargo test features::diagnostics::tests::tree_sitter_dogfood_examples_are_quiet -- --nocapture
```

Expected after this task: `count`, `total`, and `argv` diagnostics disappear. `unresolved_goto` may remain for `tree_sitter_sample.tcsh`.

---

### Task 5: Make unresolved `goto` non-noisy by default

**Files:**
- Modify: `src/features/diagnostics.rs`
- Modify: `src/semantics/mod.rs` tests if needed

- [ ] **Step 1: Decide default policy in code comments**

Add this comment above the unresolved-goto diagnostic area in `src/features/diagnostics.rs`:

```rust
    // `goto` references are still recorded for navigation/reference features, but unresolved
    // target publication is intentionally disabled by default. In day-to-day editing and syntax
    // showcase files, unresolved goto often has too much reachability/context uncertainty for a
    // low-noise diagnostic. A future strict semantic diagnostics category can re-enable it.
```

- [ ] **Step 2: Remove unresolved-goto publication from default diagnostics**

Delete or guard this block from `diagnostics_for_text()`:

```rust
    let label_names = labels.keys().cloned().collect::<HashSet<_>>();
    for reference in model
        .references
        .iter()
        .filter(|reference| reference.kind == ReferenceKind::GotoLabel)
    {
        if !label_names.contains(&reference.name) {
            diagnostics.push(lsp_diagnostic(
                text,
                reference.span,
                DiagnosticSeverity::WARNING,
                "tcsh-lsp.semantic.unresolved_goto",
                &format!("unresolved goto label `{}`", reference.name),
            ));
        }
    }
```

Do not remove `GotoLabel` references from the semantic model. Navigation/reference behavior still needs them.

- [ ] **Step 3: Update conservative diagnostics test**

In `reports_conservative_diagnostics()`, remove this assertion:

```rust
        assert!(codes.iter().any(|code| code.contains("unresolved_goto")));
```

Keep these assertions:

```rust
        assert!(codes.iter().any(|code| code.contains("unmatched_end")));
        assert!(codes.iter().any(|code| code.contains("duplicate_label")));
        assert!(codes.iter().any(|code| code.contains("unresolved_source")));
        assert!(codes.iter().any(|code| code.contains("variable_used_before_assignment")));
        assert!(codes.iter().any(|code| code.contains("suspicious_form")));
```

- [ ] **Step 4: Preserve semantic goto coverage**

Confirm `src/semantics/mod.rs::tests::records_symbols_and_references_conservatively` still contains:

```rust
        assert!(model.references.iter().any(|reference| reference.kind
            == ReferenceKind::GotoLabel
            && reference.name == "start"));
```

If this assertion was removed accidentally, restore it.

- [ ] **Step 5: Run focused dogfood test**

```bash
cargo test features::diagnostics::tests::tree_sitter_dogfood_examples_are_quiet -- --nocapture
```

Expected after this task: PASS. If any diagnostics remain, do not broaden the parser. Add a focused test for the remaining diagnostic and fix only that case.

---

### Task 6: Add LSP dogfood smoke for the copied examples

**Files:**
- Modify: `tests/protocol_lifecycle.rs`

- [ ] **Step 1: Add helper for opening fixture text**

In `tests/protocol_lifecycle.rs`, reuse the existing `frame`, `spawn_stdout_reader`, and `recv_matching` helpers. Add a second test rather than changing `initialize_shutdown_exit_lifecycle()`.

Add this test skeleton:

```rust
#[test]
fn dogfood_examples_publish_empty_diagnostics_and_answer_read_only_requests() -> Result<()> {
    let server = env!("CARGO_BIN_EXE_tcsh-lsp");
    let mut child = Command::new(server)
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn tcsh-lsp")?;

    let mut stdin = child.stdin.take().context("child stdin")?;
    let stdout = child.stdout.take().context("child stdout")?;
    let rx = spawn_stdout_reader(stdout);

    stdin.write_all(&frame(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": null,
            "rootUri": null,
            "capabilities": {},
            "clientInfo": {"name": "tcsh-lsp-dogfood-test", "version": "0"}
        }
    })))?;
    stdin.flush()?;
    let _init = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(1))
    })?;

    stdin.write_all(&frame(&json!({"jsonrpc":"2.0","method":"initialized","params":{}})))?;

    let fixtures = [
        "fixtures/corpus/parser/valid/tree_sitter_sample.tcsh",
        "fixtures/corpus/parser/valid/tree_sitter_showcase.tcsh",
    ];

    let mut next_id = 10;
    for path in fixtures {
        let text = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        let uri = format!("file:///{}", std::env::current_dir()?.join(path).display());
        stdin.write_all(&frame(&json!({
            "jsonrpc":"2.0",
            "method":"textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "tcsh",
                    "version": 1,
                    "text": text
                }
            }
        })))?;

        next_id += 1;
        stdin.write_all(&frame(&json!({
            "jsonrpc":"2.0",
            "id": next_id,
            "method":"textDocument/documentSymbol",
            "params": {"textDocument": {"uri": uri}}
        })))?;
        stdin.flush()?;

        let diagnostics = recv_matching(&rx, Duration::from_secs(10), |message| {
            message.get("method") == Some(&json!("textDocument/publishDiagnostics"))
        })?;
        assert_eq!(diagnostics["params"]["diagnostics"].as_array().map(Vec::len), Some(0));

        let symbols = recv_matching(&rx, Duration::from_secs(10), |message| {
            message.get("id") == Some(&json!(next_id))
        })?;
        assert!(symbols["result"].as_array().is_some());
    }

    stdin.write_all(&frame(&json!({"jsonrpc":"2.0","id":99,"method":"shutdown","params":null})))?;
    stdin.flush()?;
    let shutdown = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(99))
    })?;
    assert_eq!(shutdown["result"], Value::Null);
    stdin.write_all(&frame(&json!({"jsonrpc":"2.0","method":"exit"})))?;
    stdin.flush()?;
    drop(stdin);

    let status = child
        .wait_timeout(Duration::from_secs(5))?
        .context("server did not exit")?;
    assert!(status.success(), "server exit status: {status}");
    Ok(())
}
```

- [ ] **Step 2: Fix imports if needed**

`tests/protocol_lifecycle.rs` already imports most items. Ensure `std::fs` is imported. It already is not imported in the current file. Add:

```rust
use std::fs;
```

- [ ] **Step 3: Run protocol dogfood test**

```bash
cargo test --test protocol_lifecycle dogfood_examples_publish_empty_diagnostics_and_answer_read_only_requests -- --nocapture
```

Expected: PASS. If URI formatting fails on spaces or special characters, use `url::Url::from_file_path()` only if `url` is already a dependency; otherwise keep the current path because this repo path has no spaces.

---

### Task 7: Full validation and docs note

**Files:**
- Modify: `docs/known-limitations.md` if unresolved-goto default policy changed.
- Modify: `docs/tcsh-csh-coverage-matrix.md` only if wording currently promises default unresolved-goto diagnostics without caveat.

- [ ] **Step 1: Update limitation wording**

In `docs/known-limitations.md`, add or update one bullet:

```markdown
- Some semantic diagnostics, including unresolved `goto`, are intentionally conservative by default. The analyzer still records labels and goto references for navigation, but publishDiagnostics avoids noisy reachability/context guesses unless a stricter category is added later.
```

- [ ] **Step 2: Run formatting and lint gates**

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
```

Expected:

```text
cargo fmt --check: success
cargo clippy --all-targets -- -D warnings: success
cargo test: all tests passed
```

- [ ] **Step 3: Optional Neovim smoke**

```bash
XDG_STATE_HOME="$PWD/target/nvim-smoke/state" \
XDG_DATA_HOME="$PWD/target/nvim-smoke/data" \
XDG_CACHE_HOME="$PWD/target/nvim-smoke/cache" \
PATH="$PWD/target/debug:$PATH" \
  nvim --headless -u NONE \
    -c 'luafile editors/nvim/tcsh_lsp.lua' \
    -c 'edit fixtures/corpus/parser/valid/tree_sitter_sample.tcsh' \
    -c 'sleep 500m' \
    -c 'quit'
```

Expected: Neovim exits successfully and tcsh-lsp does not crash. Manual inspection with `:LspInfo` can follow in an interactive session.

- [ ] **Step 4: Final status check**

```bash
git status --short
```

Expected changed files are limited to the fixture files, focused lexer/parser/semantic/diagnostic files, optional protocol smoke test, and docs wording.

---

## Self-review

- Spec coverage: The plan targets exactly the current dogfood diagnostics: arithmetic variables, predefined `argv`, history expansion, if-condition separator splitting, and unresolved-goto noise.
- Placeholder scan: No task contains TBD/fill-in placeholders; each code change has a concrete snippet or exact deletion.
- Type consistency: New references use existing `Span`, `SymbolKind::ShellVariable`, `ReferenceKind::GotoLabel`, `TokenKind`, and `diagnostics_for_text()` APIs.
- Risk boundary: The only policy shift is unresolved `goto` publication. It preserves semantic references and documents the default diagnostic tradeoff.
