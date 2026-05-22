use crate::config::TcshLspConfig;
use crate::syntax::lexer::{TokenKind, lex};
use crate::syntax::parser::{Node, NodeKind, ParseResult, Span};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SymbolKind {
    ShellVariable,
    EnvironmentVariable,
    Alias,
    Label,
    LoopVariable,
    SourceFile,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReferenceKind {
    VariableUse,
    AliasUse,
    GotoLabel,
    SourceTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Confidence {
    Certain,
    Likely,
    Uncertain,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Symbol {
    pub name: String,
    pub kind: SymbolKind,
    pub span: Span,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reference {
    pub name: String,
    pub kind: ReferenceKind,
    pub span: Span,
    pub confidence: Confidence,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceEdge {
    pub raw: String,
    pub span: Span,
    pub resolved: Option<PathBuf>,
    pub confidence: Confidence,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct SemanticModel {
    pub symbols: Vec<Symbol>,
    pub references: Vec<Reference>,
    pub source_edges: Vec<SourceEdge>,
}

#[derive(Debug, Clone)]
pub struct AnalysisContext {
    pub current_dir: PathBuf,
    pub home_dir: Option<PathBuf>,
    pub env: Vec<(String, String)>,
    pub config: TcshLspConfig,
}

impl AnalysisContext {
    pub fn new(current_dir: PathBuf, config: TcshLspConfig) -> Self {
        Self {
            current_dir,
            home_dir: std::env::var_os("HOME").map(PathBuf::from),
            env: std::env::vars().collect(),
            config,
        }
    }
}

pub fn analyze(parse: &ParseResult, context: &AnalysisContext) -> SemanticModel {
    let mut model = SemanticModel::default();
    visit_node(&parse.root, context, &mut model);
    model
}

fn visit_node(node: &Node, context: &AnalysisContext, model: &mut SemanticModel) {
    match &node.kind {
        NodeKind::Set => record_word_after_command(node, SymbolKind::ShellVariable, model),
        NodeKind::Setenv => record_word_after_command(node, SymbolKind::EnvironmentVariable, model),
        NodeKind::Alias => record_word_after_command(node, SymbolKind::Alias, model),
        NodeKind::Label { name } => model.symbols.push(Symbol {
            name: name.clone(),
            kind: SymbolKind::Label,
            span: Span {
                start: node.span.start,
                end: node.span.start + name.len(),
            },
            confidence: Confidence::Certain,
        }),
        NodeKind::ForeachBlock => record_foreach_variable(node, model),
        NodeKind::Goto => {
            record_word_reference_after_command(node, ReferenceKind::GotoLabel, model)
        }
        NodeKind::Source => record_source(node, context, model),
        NodeKind::Command { name } if name == "@" => {
            record_arithmetic_assignment(node, model);
        }
        NodeKind::Command { name } if !name.is_empty() => {
            let span = nth_significant_word(node, 0)
                .map(|(_, span)| span)
                .unwrap_or(node.span);
            model.references.push(Reference {
                name: name.clone(),
                kind: ReferenceKind::AliasUse,
                span,
                confidence: Confidence::Likely,
            });
        }
        _ => {}
    }

    record_variable_uses(node, model);

    for child in &node.children {
        visit_node(child, context, model);
    }
}

fn record_word_after_command(node: &Node, kind: SymbolKind, model: &mut SemanticModel) {
    if let Some((name, span)) = nth_significant_word(node, 1) {
        model.symbols.push(Symbol {
            name,
            kind,
            span,
            confidence: Confidence::Certain,
        });
    }
}

fn record_word_reference_after_command(
    node: &Node,
    kind: ReferenceKind,
    model: &mut SemanticModel,
) {
    if let Some((name, span)) = nth_significant_word(node, 1) {
        model.references.push(Reference {
            name,
            kind,
            span,
            confidence: Confidence::Certain,
        });
    }
}

fn record_foreach_variable(node: &Node, model: &mut SemanticModel) {
    if let Some((name, span)) = nth_significant_word(node, 1) {
        model.symbols.push(Symbol {
            name,
            kind: SymbolKind::LoopVariable,
            span,
            confidence: Confidence::Certain,
        });
    }
}

fn record_arithmetic_assignment(node: &Node, model: &mut SemanticModel) {
    if let Some((name, span)) = nth_significant_word(node, 1) {
        let normalized = name
            .trim_end_matches("++")
            .trim_end_matches("--")
            .to_string();
        if is_shell_identifier(&normalized) {
            model.symbols.push(Symbol {
                span: Span {
                    start: span.start,
                    end: span.start + normalized.len(),
                },
                name: normalized,
                kind: SymbolKind::ShellVariable,
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

fn record_source(node: &Node, context: &AnalysisContext, model: &mut SemanticModel) {
    if let Some((raw, span)) = nth_significant_word(node, 1) {
        model.references.push(Reference {
            name: raw.clone(),
            kind: ReferenceKind::SourceTarget,
            span,
            confidence: if is_static_path(&raw) {
                Confidence::Certain
            } else {
                Confidence::Uncertain
            },
        });
        model.source_edges.push(resolve_source(&raw, span, context));
    }
}

fn record_variable_uses(node: &Node, model: &mut SemanticModel) {
    let lexed = lex(&node.text);
    for token in lexed.tokens {
        if token.kind == TokenKind::VariableExpansion {
            let name = token
                .text
                .trim_start_matches('$')
                .trim_start_matches('{')
                .trim_end_matches('}')
                .to_string();
            if !name.is_empty() {
                model.references.push(Reference {
                    name,
                    kind: ReferenceKind::VariableUse,
                    span: Span {
                        start: node.span.start + token.span.start,
                        end: node.span.start + token.span.end,
                    },
                    confidence: Confidence::Likely,
                });
            }
        }
    }
}

fn nth_significant_word(node: &Node, index: usize) -> Option<(String, Span)> {
    let lexed = lex(&node.text);
    let token = lexed
        .tokens
        .into_iter()
        .filter(|token| {
            !matches!(
                token.kind,
                TokenKind::Whitespace | TokenKind::Comment | TokenKind::Newline
            )
        })
        .nth(index)?;
    Some((
        token
            .text
            .trim_matches(['\'', '"'])
            .trim_end_matches(':')
            .to_string(),
        Span {
            start: node.span.start + token.span.start,
            end: node.span.start + token.span.end,
        },
    ))
}

pub fn resolve_source(raw: &str, span: Span, context: &AnalysisContext) -> SourceEdge {
    if !is_static_path(raw) {
        return SourceEdge {
            raw: raw.to_string(),
            span,
            resolved: None,
            confidence: Confidence::Uncertain,
            reason: "dynamic source path is not resolved without executing shell code".to_string(),
        };
    }

    let stripped = raw.trim_matches(['\'', '"']);
    let candidates = source_candidates(stripped, context);
    for candidate in candidates {
        if candidate.exists() {
            return SourceEdge {
                raw: raw.to_string(),
                span,
                resolved: Some(candidate),
                confidence: Confidence::Certain,
                reason: "static source path resolved".to_string(),
            };
        }
    }

    SourceEdge {
        raw: raw.to_string(),
        span,
        resolved: None,
        confidence: Confidence::Certain,
        reason: "static source path did not exist in current/include paths".to_string(),
    }
}

fn source_candidates(raw: &str, context: &AnalysisContext) -> Vec<PathBuf> {
    let mut candidates = Vec::new();
    let raw_path = Path::new(raw);
    if raw_path.is_absolute() {
        candidates.push(raw_path.to_path_buf());
        return candidates;
    }
    if let Some(rest) = raw.strip_prefix("~/") {
        if let Some(home) = &context.home_dir {
            candidates.push(home.join(rest));
        }
        return candidates;
    }
    candidates.push(context.current_dir.join(raw));
    for include in &context.config.include_paths {
        candidates.push(PathBuf::from(include).join(raw));
    }
    candidates
}

fn is_static_path(raw: &str) -> bool {
    let stripped = raw.trim_matches(['\'', '"']);
    !stripped.is_empty()
        && !stripped.contains('$')
        && !stripped.contains('`')
        && !stripped.contains('*')
        && !stripped.contains('?')
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::parser::parse;

    #[test]
    fn records_symbols_and_references_conservatively() {
        let parsed = parse(
            "set foo = bar\nsetenv EDITOR vi\nalias ll 'ls -l'\nstart:\ngoto start\nforeach item ( a b )\n echo $item $foo\nend\n",
        );
        let context =
            AnalysisContext::new(std::env::current_dir().unwrap(), TcshLspConfig::default());
        let model = analyze(&parsed, &context);
        assert!(
            model
                .symbols
                .iter()
                .any(|symbol| symbol.kind == SymbolKind::ShellVariable && symbol.name == "foo")
        );
        assert!(model.symbols.iter().any(
            |symbol| symbol.kind == SymbolKind::EnvironmentVariable && symbol.name == "EDITOR"
        ));
        assert!(
            model
                .symbols
                .iter()
                .any(|symbol| symbol.kind == SymbolKind::Alias && symbol.name == "ll")
        );
        assert!(
            model
                .symbols
                .iter()
                .any(|symbol| symbol.kind == SymbolKind::Label && symbol.name == "start")
        );
        assert!(
            model
                .symbols
                .iter()
                .any(|symbol| symbol.kind == SymbolKind::LoopVariable && symbol.name == "item")
        );
        assert!(model.references.iter().any(|reference| reference.kind
            == ReferenceKind::GotoLabel
            && reference.name == "start"));
        assert!(model.references.iter().any(|reference| reference.kind
            == ReferenceKind::VariableUse
            && reference.name == "foo"));
    }

    #[test]
    fn records_arithmetic_assignment_symbols() {
        let parsed = parse(
            "@ count = 1 + 2
@ count++
echo $count
",
        );
        let context =
            AnalysisContext::new(std::env::current_dir().unwrap(), TcshLspConfig::default());
        let model = analyze(&parsed, &context);
        assert!(
            model.symbols.iter().any(|symbol| {
                symbol.kind == SymbolKind::ShellVariable && symbol.name == "count"
            })
        );
    }

    #[test]
    fn resolves_static_source_and_keeps_dynamic_uncertain() {
        let root = std::env::temp_dir().join(format!("tcsh-lsp-source-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let include = root.join("include");
        std::fs::create_dir_all(&include).unwrap();
        std::fs::write(include.join("common.csh"), "echo common\n").unwrap();
        let mut config = TcshLspConfig::default();
        config
            .include_paths
            .push(include.to_string_lossy().to_string());
        let mut context = AnalysisContext::new(root, config);
        context.home_dir = None;

        let static_edge = resolve_source("common.csh", Span { start: 0, end: 10 }, &context);
        assert_eq!(static_edge.confidence, Confidence::Certain);
        assert!(static_edge.resolved.is_some());

        let dynamic_edge =
            resolve_source("$CONFIG/common.csh", Span { start: 0, end: 18 }, &context);
        assert_eq!(dynamic_edge.confidence, Confidence::Uncertain);
        assert!(dynamic_edge.resolved.is_none());
    }
}
