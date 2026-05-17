use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};
use tcsh_lsp::syntax::parser::{NodeKind, ParseDiagnosticCode, format_parse_for_golden, parse};

const GOLDEN_CASES: &[(&str, &str)] = &[
    (
        "fixtures/corpus/parser/valid/02_if.tcsh",
        "fixtures/golden/parser/02_if.parse",
    ),
    (
        "fixtures/corpus/parser/valid/03_foreach.tcsh",
        "fixtures/golden/parser/03_foreach.parse",
    ),
    (
        "fixtures/corpus/parser/valid/05_switch.tcsh",
        "fixtures/golden/parser/05_switch.parse",
    ),
    (
        "fixtures/corpus/parser/invalid/02_broken.tcsh",
        "fixtures/golden/parser/unmatched_endif.parse",
    ),
    (
        "fixtures/corpus/parser/incomplete/01_editing.tcsh",
        "fixtures/golden/parser/incomplete_foreach.parse",
    ),
];

#[test]
fn parser_golden_fixtures_match() -> Result<()> {
    for (input_path, golden_path) in GOLDEN_CASES {
        let input = fs::read_to_string(input_path).with_context(|| format!("read {input_path}"))?;
        let actual = format_parse_for_golden(&parse(&input));
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
fn parser_seed_corpus_meets_feasibility_gate_counts_and_never_panics() -> Result<()> {
    let groups = [
        ("fixtures/corpus/parser/valid", 10usize),
        ("fixtures/corpus/parser/invalid", 10usize),
        ("fixtures/corpus/parser/incomplete", 10usize),
        ("fixtures/corpus/parser/source-resolution", 5usize),
    ];
    for (dir, minimum) in groups {
        let files = files_in(dir)?;
        assert!(
            files.len() >= minimum,
            "{dir} has {} files, expected at least {minimum}",
            files.len()
        );
        for path in files {
            let text = fs::read_to_string(&path)?;
            let result = parse(&text);
            assert!(result.root.span.end <= text.len());
        }
    }
    Ok(())
}

#[test]
fn vertical_slice_set_reference_shape_is_stable() {
    let parsed = parse("set foo = bar\necho $foo\n");
    assert!(matches!(parsed.root.children[0].kind, NodeKind::Set));
    assert!(
        matches!(parsed.root.children[1].kind, NodeKind::Command { ref name } if name == "echo")
    );
    assert!(parsed.diagnostics.is_empty());
}

#[test]
fn incomplete_blocks_recover_with_missing_end_diagnostic() {
    let parsed = parse("while ( $x )\n  echo $x\n");
    assert!(
        parsed
            .diagnostics
            .iter()
            .any(|diagnostic| diagnostic.code == ParseDiagnosticCode::MissingEnd)
    );
}

fn files_in(dir: &str) -> Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in fs::read_dir(dir).with_context(|| format!("read_dir {dir}"))? {
        let entry = entry?;
        if entry.file_type()?.is_file() {
            files.push(entry.path());
        }
    }
    files.sort();
    if files.is_empty() {
        bail!("no files in {dir}");
    }
    Ok(files)
}
