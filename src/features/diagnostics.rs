use crate::semantics::{AnalysisContext, Confidence, ReferenceKind, SymbolKind, analyze};
use crate::syntax::lexer::{TokenKind, lex};
use crate::syntax::parser::{Node, NodeKind, ParseDiagnosticCode, ParseResult, Span, parse};
use crate::util::span_to_range;
use std::collections::{HashMap, HashSet};
use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};

pub fn diagnostics_for_text(text: &str, context: &AnalysisContext) -> Vec<Diagnostic> {
    let parsed = parse(text);
    let model = analyze(&parsed, context);
    let mut diagnostics = Vec::new();

    for diagnostic in &parsed.diagnostics {
        let (code, severity) = match diagnostic.code {
            ParseDiagnosticCode::LexerError => ("tcsh-lsp.lex.error", DiagnosticSeverity::ERROR),
            ParseDiagnosticCode::MissingEnd => {
                ("tcsh-lsp.parse.missing_end", DiagnosticSeverity::ERROR)
            }
            ParseDiagnosticCode::UnmatchedEnd => {
                ("tcsh-lsp.parse.unmatched_end", DiagnosticSeverity::ERROR)
            }
        };
        diagnostics.push(lsp_diagnostic(
            text,
            diagnostic.span,
            severity,
            code,
            &diagnostic.message,
        ));
    }

    diagnostics.extend(parenthesis_diagnostics(text));
    diagnostics.extend(form_diagnostics(text, &parsed));

    let mut labels = HashMap::<String, Span>::new();
    for symbol in model
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Label)
    {
        if let Some(first) = labels.insert(symbol.name.clone(), symbol.span) {
            diagnostics.push(lsp_diagnostic(
                text,
                symbol.span,
                DiagnosticSeverity::WARNING,
                "tcsh-lsp.semantic.duplicate_label",
                &format!(
                    "duplicate label `{}`; first definition starts at byte {}",
                    symbol.name, first.start
                ),
            ));
        }
    }

    // `goto` references are still recorded for navigation/reference features, but unresolved
    // target publication is intentionally disabled by default. In day-to-day editing and syntax
    // showcase files, unresolved goto often has too much reachability/context uncertainty for a
    // low-noise diagnostic. A future strict semantic diagnostics category can re-enable it.

    for edge in &model.source_edges {
        if edge.confidence == Confidence::Certain && edge.resolved.is_none() {
            diagnostics.push(lsp_diagnostic(
                text,
                edge.span,
                DiagnosticSeverity::WARNING,
                "tcsh-lsp.semantic.unresolved_source",
                &format!("unresolved source file `{}`", edge.raw),
            ));
        }
    }

    let assigned_vars = model
        .symbols
        .iter()
        .filter(|symbol| {
            matches!(
                symbol.kind,
                SymbolKind::ShellVariable
                    | SymbolKind::EnvironmentVariable
                    | SymbolKind::LoopVariable
            )
        })
        .map(|symbol| symbol.name.clone())
        .collect::<HashSet<_>>();
    for reference in model
        .references
        .iter()
        .filter(|reference| reference.kind == ReferenceKind::VariableUse)
    {
        if is_conservative_shell_var_name(&reference.name)
            && !is_predefined_tcsh_variable(&reference.name)
            && !assigned_vars.contains(&reference.name)
        {
            diagnostics.push(lsp_diagnostic(
                text,
                reference.span,
                DiagnosticSeverity::WARNING,
                "tcsh-lsp.semantic.variable_used_before_assignment",
                &format!(
                    "variable `{}` is used before a known assignment",
                    reference.name
                ),
            ));
        }
    }

    let alias_symbols = model
        .symbols
        .iter()
        .filter(|symbol| symbol.kind == SymbolKind::Alias)
        .map(|symbol| (symbol.name.clone(), symbol.span.start))
        .collect::<HashMap<_, _>>();
    for reference in model
        .references
        .iter()
        .filter(|reference| reference.kind == ReferenceKind::AliasUse)
    {
        if let Some(definition_start) = alias_symbols.get(&reference.name) {
            if reference.span.start < *definition_start {
                diagnostics.push(lsp_diagnostic(
                    text,
                    reference.span,
                    DiagnosticSeverity::WARNING,
                    "tcsh-lsp.semantic.alias_used_before_definition",
                    &format!("alias `{}` is used before its definition", reference.name),
                ));
            }
        }
    }

    diagnostics.sort_by_key(|diagnostic| {
        (
            diagnostic.range.start.line,
            diagnostic.range.start.character,
            diagnostic.message.clone(),
        )
    });
    diagnostics
}

fn parenthesis_diagnostics(text: &str) -> Vec<Diagnostic> {
    let mut stack = Vec::new();
    let mut diagnostics = Vec::new();
    for token in lex(text).tokens {
        match token.kind {
            TokenKind::LeftParen => stack.push(token.span),
            TokenKind::RightParen if stack.pop().is_none() => {
                diagnostics.push(lsp_diagnostic(
                    text,
                    Span {
                        start: token.span.start,
                        end: token.span.end,
                    },
                    DiagnosticSeverity::ERROR,
                    "tcsh-lsp.parse.unbalanced_parentheses",
                    "unmatched closing parenthesis",
                ));
            }
            _ => {}
        }
    }
    for span in stack {
        diagnostics.push(lsp_diagnostic(
            text,
            Span {
                start: span.start,
                end: span.end,
            },
            DiagnosticSeverity::ERROR,
            "tcsh-lsp.parse.unbalanced_parentheses",
            "unmatched opening parenthesis",
        ));
    }
    diagnostics
}

fn form_diagnostics(text: &str, parsed: &ParseResult) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    collect_form_diagnostics(text, &parsed.root, &mut diagnostics);
    diagnostics
}

fn collect_form_diagnostics(text: &str, node: &Node, diagnostics: &mut Vec<Diagnostic>) {
    let min_words = match node.kind {
        NodeKind::Set
        | NodeKind::Unset
        | NodeKind::Alias
        | NodeKind::Unalias
        | NodeKind::Setenv
        | NodeKind::Unsetenv => Some(2),
        _ => None,
    };
    if let Some(min_words) = min_words {
        let count = significant_word_count(&node.text);
        if count < min_words {
            diagnostics.push(lsp_diagnostic(
                text,
                node.span,
                DiagnosticSeverity::WARNING,
                "tcsh-lsp.semantic.suspicious_form",
                "suspicious command form has too few arguments",
            ));
        }
    }
    for child in &node.children {
        collect_form_diagnostics(text, child, diagnostics);
    }
}

fn significant_word_count(text: &str) -> usize {
    lex(text)
        .tokens
        .into_iter()
        .filter(|token| {
            !matches!(
                token.kind,
                TokenKind::Whitespace | TokenKind::Comment | TokenKind::Newline
            )
        })
        .count()
}

fn is_conservative_shell_var_name(name: &str) -> bool {
    name.chars()
        .next()
        .is_some_and(|ch| ch.is_ascii_lowercase())
        && name
            .chars()
            .all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
}

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

fn lsp_diagnostic(
    text: &str,
    span: Span,
    severity: DiagnosticSeverity,
    code: &'static str,
    message: &str,
) -> Diagnostic {
    Diagnostic {
        range: span_to_range(text, span),
        severity: Some(severity),
        code: Some(NumberOrString::String(code.to_string())),
        code_description: None,
        source: Some("tcsh-lsp".to_string()),
        message: message.to_string(),
        related_information: None,
        tags: None,
        data: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TcshLspConfig;
    use std::path::PathBuf;

    fn context() -> AnalysisContext {
        AnalysisContext::new(PathBuf::from("/tmp"), TcshLspConfig::default())
    }

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

    #[test]
    fn predefined_tcsh_variables_do_not_require_assignment() {
        let text = "echo $argv[1] $argc $status $cwd $home
";
        let diagnostics = diagnostics_for_text(text, &context());
        assert!(
            diagnostics.iter().all(|diagnostic| {
                match diagnostic.code.as_ref() {
                    Some(NumberOrString::String(value)) => {
                        !value.contains("variable_used_before_assignment")
                    }
                    _ => true,
                }
            }),
            "predefined variables should not produce used-before-assignment diagnostics: {diagnostics:#?}"
        );
    }

    #[test]
    fn reports_conservative_diagnostics() {
        let text = "endif\nstart:\nstart:\ngoto missing\nsource ./missing.csh\necho $foo\nset\n";
        let diagnostics = diagnostics_for_text(text, &context());
        let codes = diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.code.as_ref())
            .map(|code| match code {
                NumberOrString::String(value) => value.clone(),
                NumberOrString::Number(value) => value.to_string(),
            })
            .collect::<Vec<_>>();
        assert!(codes.iter().any(|code| code.contains("unmatched_end")));
        assert!(codes.iter().any(|code| code.contains("duplicate_label")));
        assert!(codes.iter().any(|code| code.contains("unresolved_source")));
        assert!(
            codes
                .iter()
                .any(|code| code.contains("variable_used_before_assignment"))
        );
        assert!(codes.iter().any(|code| code.contains("suspicious_form")));
    }
}
