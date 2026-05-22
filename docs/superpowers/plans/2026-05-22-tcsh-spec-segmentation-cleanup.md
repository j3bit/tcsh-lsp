# Tcsh Spec Segmentation Cleanup Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the current broad parenthesis-depth separator heuristic with a tcsh-spec-backed command segmenter that preserves `if` expression operators while recovering grouped command sequences.

**Architecture:** Keep lexer, segmentation, parser, semantics, and LSP features separated. Add a focused `src/syntax/segmenter.rs` module that owns command-boundary policy using tcsh manual rules; parser consumes segments and no longer embeds boundary heuristics. Do not patch semantics or diagnostics to compensate for parser segmentation mistakes.

**Tech Stack:** Rust, current tcsh-lsp lexer/parser/semantic analyzer, `cargo test`, official tcsh manual/manpage references.

---

## SSOT references and interpretation

Use these as the normative behavior for this plan:

1. tcsh manual, “Simple commands, pipelines and sequences”: simple commands and pipelines are joined into sequences with `;`, `||`, and `&&`; a simple command, pipeline, or sequence may be placed in `()` to form a simple command; `&` runs a command/pipeline/sequence without waiting.
2. tcsh manual, “Control flow” and “Expressions”: `if`, `while`, and `exit` use expressions; `if ... then ... else` and loop forms require major keywords to appear in a single simple command on an input line.
3. tcsh manual, “Logical, arithmetical and comparison operators”: expression operators include `!`, `&&`, and `||`; expression components should be separated unless adjacent to parser-significant characters such as `&`, `|`, `<`, `>`, `(`, `)`.

Project interpretation for recoverable LSP parsing:

- `Newline` is always a recovery boundary. A single unclosed `(` must not merge the rest of the file.
- `;` is always a command sequence boundary, including inside grouped command sequences.
- single `&` is always a background command boundary, including inside grouped command sequences.
- `&&` and `||` are ambiguous: command sequence operators generally, expression operators in `if`/`while`/`exit` expression command heads.
- Therefore, keep `&&`/`||` inside parentheses only when the current segment is an expression-bearing command head (`if`, `while`, or `exit`). Otherwise split them as command sequence boundaries.
- This is intentionally conservative: it fixes the current dogfood and review issues without attempting a full tcsh AST for grouped subshell commands.

## Scope and stop condition

Stop when all are true:

1. `( echo one ; echo two )` segments into recoverable command pieces rather than one opaque segment.
2. `( echo one && echo two )` and `( echo one || echo two )` do not hide the later command behind the parenthesized group opener.
3. `if ( -e ~/.tcshrc && $count >= 3 ) then` remains one `IfBlock` opener.
4. `if ( $?prompt || $?user ) then` remains one `IfBlock` opener after `||` lexing support.
5. Existing dogfood zero-diagnostics tests still pass.
6. Full validation passes: `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`.

Non-goals:

- Do not implement full tcsh grouped-command AST nesting.
- Do not change symbol/diagnostic policy unrelated to segmentation.
- Do not broaden formatter, rename, or workspace indexing behavior.
- Do not hardcode sample-file-specific behavior.

## File structure

- Create: `src/syntax/segmenter.rs`
  - Owns tcsh command-boundary policy.
  - Contains `CommandSegment<'a>` and `command_segments()`.
  - Contains focused unit tests for SSOT segmentation cases.
- Modify: `src/syntax/mod.rs`
  - Export `segmenter` internally.
- Modify: `src/syntax/parser.rs`
  - Remove private `Segment`, `command_segments()`, and `trim_trivia()`.
  - Import `crate::syntax::segmenter::command_segments`.
  - Keep `classify_segment()` and block recovery unchanged except for type names.
- Modify: `src/syntax/lexer.rs`
  - Add `||` lexing as `TokenKind::Separator` while preserving `|` and `|&` as `TokenKind::Pipe`.
  - Add focused lexer regression for `||`.
- Modify: `src/semantics/mod.rs`
  - Add one semantic integration regression showing aliases or references after grouped separators are still visible.
- Optionally modify: `docs/known-limitations.md`
  - Add a short note that grouped command parsing is recovery-oriented, not a full nested AST.

---

### Task 1: Add lexer support for `||` as a sequence separator

**Files:**
- Modify: `src/syntax/lexer.rs`

- [ ] **Step 1: Write the failing lexer test**

Append this test in the existing `#[cfg(test)] mod tests` in `src/syntax/lexer.rs`:

```rust
    #[test]
    fn double_pipe_is_sequence_separator_not_pipeline() {
        let result = lex("echo one || echo two\necho one |& cat\necho one | cat\n");
        assert!(result.errors.is_empty(), "unexpected lex errors: {:#?}", result.errors);
        assert!(result.tokens.iter().any(|token| {
            token.kind == TokenKind::Separator && token.text == "||"
        }));
        assert!(result.tokens.iter().any(|token| {
            token.kind == TokenKind::Pipe && token.text == "|&"
        }));
        assert!(result.tokens.iter().any(|token| {
            token.kind == TokenKind::Pipe && token.text == "|"
        }));
    }
```

- [ ] **Step 2: Run the focused test and verify RED**

```bash
cargo test syntax::lexer::tests::double_pipe_is_sequence_separator_not_pipeline -- --nocapture
```

Expected before implementation: FAIL because `||` is not emitted as a `Separator` token.

- [ ] **Step 3: Implement minimal lexing change**

Replace the current `|` arm in `Lexer::run`:

```rust
                '|' => self.consume_pipe(start),
```

with:

```rust
                '|' => self.consume_pipe_or_or_separator(start),
```

Add this method near `consume_pipe()`:

```rust
    fn consume_pipe_or_or_separator(&mut self, start: usize) {
        self.bump_char();
        if self.peek_char() == Some('|') {
            self.bump_char();
            self.push(TokenKind::Separator, start, self.cursor);
            self.at_command_start = true;
            return;
        }
        if self.peek_char() == Some('&') {
            self.bump_char();
        }
        self.push(TokenKind::Pipe, start, self.cursor);
        self.at_command_start = true;
    }
```

Then delete the old `consume_pipe()` method, or leave it unused only if `cargo clippy` does not warn. Prefer deleting it.

- [ ] **Step 4: Run focused lexer tests**

```bash
cargo test syntax::lexer::tests::double_pipe_is_sequence_separator_not_pipeline -- --nocapture
cargo test syntax::lexer::tests::bang_before_variable_preserves_variable_expansion_token -- --nocapture
cargo test syntax::lexer::tests::history_expansions_do_not_emit_malformed_variable_errors -- --nocapture
```

Expected: all PASS.

- [ ] **Step 5: Commit checkpoint**

```bash
git add src/syntax/lexer.rs
git commit -m "fix tcsh double-pipe lexing"
```

---

### Task 2: Extract spec-backed command segmenter

**Files:**
- Create: `src/syntax/segmenter.rs`
- Modify: `src/syntax/mod.rs`
- Modify: `src/syntax/parser.rs`

- [ ] **Step 1: Create `src/syntax/segmenter.rs` with tests first**

Create the file with this full initial content:

```rust
use crate::syntax::lexer::{Token, TokenKind};

#[derive(Debug)]
pub(crate) struct CommandSegment<'a> {
    pub(crate) tokens: Vec<&'a Token>,
    pub(crate) end: usize,
}

pub(crate) fn command_segments(tokens: &[Token]) -> Vec<CommandSegment<'_>> {
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
            TokenKind::Newline => {
                flush_segment(&mut segments, &mut current, token.span.end, &mut last_end);
                paren_depth = 0;
            }
            TokenKind::Separator if should_split_separator(token, paren_depth, &current) => {
                flush_segment(&mut segments, &mut current, token.span.end, &mut last_end);
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
        segments.push(CommandSegment { tokens: filtered, end });
    }
    segments
}

fn should_split_separator(token: &Token, paren_depth: usize, current: &[&Token]) -> bool {
    match token.text.as_str() {
        ";" | "&" => true,
        "&&" | "||" => paren_depth == 0 || !segment_starts_with_expression_builtin(current),
        _ => paren_depth == 0,
    }
}

fn segment_starts_with_expression_builtin(tokens: &[&Token]) -> bool {
    tokens
        .iter()
        .find(|token| !matches!(token.kind, TokenKind::Whitespace | TokenKind::Comment))
        .is_some_and(|token| {
            token.kind == TokenKind::Word
                && matches!(token.text.to_ascii_lowercase().as_str(), "if" | "while" | "exit")
        })
}

fn flush_segment<'a>(
    segments: &mut Vec<CommandSegment<'a>>,
    current: &mut Vec<&'a Token>,
    end: usize,
    last_end: &mut usize,
) {
    let filtered = trim_trivia(std::mem::take(current));
    if !filtered.is_empty() {
        segments.push(CommandSegment { tokens: filtered, end });
    }
    *last_end = end;
}

fn trim_trivia(mut tokens: Vec<&Token>) -> Vec<&Token> {
    while tokens
        .first()
        .is_some_and(|token| matches!(token.kind, TokenKind::Whitespace | TokenKind::Comment))
    {
        tokens.remove(0);
    }
    while tokens
        .last()
        .is_some_and(|token| matches!(token.kind, TokenKind::Whitespace | TokenKind::Comment))
    {
        tokens.pop();
    }
    tokens
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::lexer::lex;

    fn segment_texts(input: &str) -> Vec<String> {
        let lexed = lex(input);
        command_segments(&lexed.tokens)
            .into_iter()
            .map(|segment| {
                segment
                    .tokens
                    .into_iter()
                    .map(|token| token.text.as_str())
                    .collect::<String>()
                    .trim()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn if_expression_keeps_logical_and_or_inside_parentheses() {
        assert_eq!(
            segment_texts("if ( -e ~/.tcshrc && $?prompt || $?user ) then\nendif\n"),
            vec!["if ( -e ~/.tcshrc && $?prompt || $?user ) then", "endif"]
        );
    }

    #[test]
    fn grouped_command_lists_split_on_sequence_separators() {
        assert_eq!(
            segment_texts("( echo one ; echo two )\n"),
            vec!["( echo one", "echo two )"]
        );
        assert_eq!(
            segment_texts("( echo one & echo two )\n"),
            vec!["( echo one", "echo two )"]
        );
        assert_eq!(
            segment_texts("( echo one && echo two || echo three )\n"),
            vec!["( echo one", "echo two", "echo three )"]
        );
    }

    #[test]
    fn newline_is_always_recovery_boundary_after_unclosed_parenthesis() {
        assert_eq!(
            segment_texts("if ( $x > 0\necho $foo\nendif\n"),
            vec!["if ( $x > 0", "echo $foo", "endif"]
        );
    }
}
```

- [ ] **Step 2: Export the module**

In `src/syntax/mod.rs`, change:

```rust
pub mod lexer;
pub mod parser;
```

to:

```rust
pub mod lexer;
pub(crate) mod segmenter;
pub mod parser;
```

- [ ] **Step 3: Run segmenter tests and verify expected failures before parser wiring**

```bash
cargo test syntax::segmenter::tests -- --nocapture
```

Expected after Task 1 and new module: PASS for the segmenter module. If `||` still fails, return to Task 1 before touching parser.

- [ ] **Step 4: Wire parser to segmenter**

In `src/syntax/parser.rs`, replace the import:

```rust
use crate::syntax::lexer::{Token, TokenKind, lex};
```

with:

```rust
use crate::syntax::lexer::{Token, TokenKind, lex};
use crate::syntax::segmenter::command_segments;
```

Then delete these private parser items entirely:

```rust
#[derive(Debug)]
struct Segment<'a> {
    tokens: Vec<&'a Token>,
    end: usize,
}

fn command_segments(tokens: &[Token]) -> Vec<Segment<'_>> { /* old body */ }

fn trim_trivia(mut tokens: Vec<&Token>) -> Vec<&Token> { /* old body */ }
```

Keep `classify_segment(input: &str, tokens: &[&Token]) -> Node` unchanged.

- [ ] **Step 5: Run parser recovery tests**

```bash
cargo test syntax::parser::tests::if_condition_with_and_separator_inside_parentheses_stays_one_block -- --nocapture
cargo test syntax::parser::tests::newline_still_recovers_after_unclosed_parenthesis -- --nocapture
```

Expected: PASS.

- [ ] **Step 6: Commit checkpoint**

```bash
git add src/syntax/mod.rs src/syntax/segmenter.rs src/syntax/parser.rs
git commit -m "separate tcsh command segmentation"
```

---

### Task 3: Add parser and semantic integration coverage for grouped command sequences

**Files:**
- Modify: `src/syntax/parser.rs`
- Modify: `src/semantics/mod.rs`

- [ ] **Step 1: Add parser integration test**

Append this test to `#[cfg(test)] mod tests` in `src/syntax/parser.rs`:

```rust
    #[test]
    fn grouped_sequence_separator_keeps_later_commands_recoverable() {
        let parsed = parse("( echo one ; alias ll 'ls -l' )\nll /tmp\n");
        assert!(
            parsed.diagnostics.is_empty(),
            "grouped command sequence should remain recoverable: {:#?}",
            parsed.diagnostics
        );
        assert!(parsed.root.children.iter().any(|node| matches!(node.kind, NodeKind::Alias)));
        assert!(parsed.root.children.iter().any(|node| {
            matches!(&node.kind, NodeKind::Command { name } if name == "ll")
        }));
    }
```

- [ ] **Step 2: Add semantic integration test**

Append this test to `#[cfg(test)] mod tests` in `src/semantics/mod.rs`:

```rust
    #[test]
    fn grouped_sequence_later_alias_remains_visible_to_semantics() {
        let parsed = parse("( echo one ; alias ll 'ls -l' )\nll /tmp\n");
        let context =
            AnalysisContext::new(std::env::current_dir().unwrap(), TcshLspConfig::default());
        let model = analyze(&parsed, &context);
        assert!(model.symbols.iter().any(|symbol| {
            symbol.kind == SymbolKind::Alias && symbol.name == "ll"
        }));
        assert!(model.references.iter().any(|reference| {
            reference.kind == ReferenceKind::AliasUse && reference.name == "ll"
        }));
    }
```

- [ ] **Step 3: Run focused integration tests**

```bash
cargo test syntax::parser::tests::grouped_sequence_separator_keeps_later_commands_recoverable -- --nocapture
cargo test semantics::tests::grouped_sequence_later_alias_remains_visible_to_semantics -- --nocapture
```

Expected: PASS after Task 2. If either fails, do not patch semantics first; inspect segment texts and parser classification first.

- [ ] **Step 4: Commit checkpoint**

```bash
git add src/syntax/parser.rs src/semantics/mod.rs
git commit -m "test grouped sequence recovery"
```

---

### Task 4: Document the recovery-oriented grouped command limitation

**Files:**
- Modify: `docs/known-limitations.md`

- [ ] **Step 1: Add limitation wording**

Add this bullet near the existing parser/diagnostics bullets:

```markdown
- Parenthesized command sequences are segmented for recovery and semantic visibility, but they are not modeled as a fully nested tcsh command-group AST yet. The segmenter follows tcsh command-sequence boundaries while preserving `if`/`while`/`exit` expression operators inside expression parentheses.
```

- [ ] **Step 2: Commit checkpoint**

```bash
git add docs/known-limitations.md
git commit -m "document grouped command recovery limits"
```

---

### Task 5: Full validation and PR update

**Files:**
- Read: all changed files
- No planned source changes unless validation reveals a regression.

- [ ] **Step 1: Run formatting and lint gates**

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

- [ ] **Step 2: Run dogfood and protocol focus tests explicitly**

```bash
cargo test features::diagnostics::tests::tree_sitter_dogfood_examples_are_quiet -- --nocapture
cargo test --test protocol_lifecycle dogfood_examples_publish_empty_diagnostics_and_answer_read_only_requests -- --nocapture
```

Expected: both PASS.

- [ ] **Step 3: Check PR review context**

```bash
python3 /Users/jeongsaebit/.codex/plugins/cache/openai-curated/github/004da724/skills/gh-address-comments/scripts/fetch_comments.py > /tmp/tcsh_lsp_pr1_comments_after_segmenter.json
python3 - <<'PY'
import json
p=json.load(open('/tmp/tcsh_lsp_pr1_comments_after_segmenter.json'))
for i,t in enumerate(p['review_threads'],1):
    print(i, 'resolved=', t['isResolved'], 'outdated=', t['isOutdated'], 'path=', t['path'], 'line=', t['line'], 'comments=', len(t['comments']['nodes']))
PY
```

Expected: the grouped-separator review thread is present and can be replied to after push.

- [ ] **Step 4: Push the branch**

```bash
git status --short --branch
git push
```

Expected: branch `dogfood-diagnostics-zero` pushes successfully to PR #1.

- [ ] **Step 5: Reply to grouped-separator review thread**

Use GraphQL `addPullRequestReviewThreadReply` with the current grouped-separator thread id. The body should be:

```text
Fixed in <commit> by extracting tcsh command segmentation into `src/syntax/segmenter.rs`. The segmenter now follows tcsh sequence boundaries: newline, `;`, and single `&` always split; `&&`/`||` split as command sequence operators except inside `if`/`while`/`exit` expression parentheses. Added tests for grouped `;`, `&`, `&&`, `||`, expression `&&`/`||`, newline recovery, and grouped alias semantic visibility. Full cargo validation passes.
```

Do not resolve the thread unless the user explicitly asks.

## Self-review

- Spec coverage: The plan maps directly to tcsh manual command sequences, parenthesized sequences, background `&`, control-flow expression commands, and expression logical operators.
- Placeholder scan: No `TBD`, broad “add tests”, or unspecified implementation steps remain; each code-changing step includes exact snippets.
- Type consistency: `CommandSegment<'a>` replaces parser-private `Segment<'a>` with the same `tokens` and `end` fields; parser `classify_segment()` still receives `&[&Token]`.
- Clean architecture: Boundary policy moves from parser into `syntax::segmenter`; lexer only tokenizes `||`; semantics only receives parser output and does not compensate for segmentation errors.
