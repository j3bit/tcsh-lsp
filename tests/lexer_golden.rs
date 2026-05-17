use anyhow::{Context, Result};
use std::fs;
use std::path::Path;
use tcsh_lsp::syntax::lexer::{format_tokens_for_golden, lex};

const CASES: &[(&str, &str)] = &[
    (
        "fixtures/corpus/valid/lexer_core.tcsh",
        "fixtures/golden/lexer/lexer_core.tokens",
    ),
    (
        "fixtures/corpus/invalid/lexer_errors.tcsh",
        "fixtures/golden/lexer/lexer_errors.tokens",
    ),
    (
        "fixtures/corpus/invalid/unterminated_double.tcsh",
        "fixtures/golden/lexer/unterminated_double.tokens",
    ),
    (
        "fixtures/corpus/invalid/unterminated_backtick.tcsh",
        "fixtures/golden/lexer/unterminated_backtick.tokens",
    ),
    (
        "fixtures/corpus/invalid/malformed_variable.tcsh",
        "fixtures/golden/lexer/malformed_variable.tokens",
    ),
    (
        "fixtures/corpus/invalid/unbalanced_command_substitution.tcsh",
        "fixtures/golden/lexer/unbalanced_command_substitution.tokens",
    ),
    (
        "fixtures/corpus/incomplete/lexer_editing.tcsh",
        "fixtures/golden/lexer/lexer_editing.tokens",
    ),
];

#[test]
fn lexer_golden_fixtures_match() -> Result<()> {
    for (input_path, golden_path) in CASES {
        let input = fs::read_to_string(input_path).with_context(|| format!("read {input_path}"))?;
        let actual = format_tokens_for_golden(&lex(&input));
        if std::env::var_os("UPDATE_GOLDENS").is_some() {
            if let Some(parent) = Path::new(golden_path).parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(golden_path, &actual)?;
        }
        let expected =
            fs::read_to_string(golden_path).with_context(|| format!("read {golden_path}"))?;
        assert_eq!(actual, expected, "golden mismatch for {input_path}");
    }
    Ok(())
}

#[test]
fn lexer_never_panics_on_representative_mutations() {
    let seeds = [
        "",
        "$",
        "${",
        "`",
        "'",
        "\"",
        "$(echo $(nested)",
        "foreach x ( a b )\n echo $x",
        "label:\n goto label",
        "😀 $emoji \\\n",
    ];
    for seed in seeds {
        let result = lex(seed);
        assert!(
            result
                .tokens
                .iter()
                .all(|token| token.span.start <= token.span.end)
        );
    }
}
