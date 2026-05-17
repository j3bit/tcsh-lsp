use crate::semantics::{AnalysisContext, ReferenceKind, SymbolKind, analyze};
use crate::syntax::lexer::{TokenKind, lex};
use crate::syntax::parser::{Node, NodeKind, Span, parse};
use crate::util::{offset_to_position, position_to_offset, span_to_range};
use tower_lsp::lsp_types::{
    DocumentHighlight, DocumentHighlightKind, FoldingRange, FoldingRangeKind, Location, Position,
    SelectionRange, SemanticToken, SemanticTokenModifier, SemanticTokenType, SemanticTokens,
    SemanticTokensLegend, SymbolInformation, SymbolKind as LspSymbolKind, Url,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReferenceQuery {
    pub name: String,
    pub symbol_kinds: Vec<SymbolKind>,
    pub reference_kinds: Vec<ReferenceKind>,
}

pub fn references_for_position(
    text: &str,
    uri: &Url,
    position: Position,
    context: &AnalysisContext,
) -> Vec<Location> {
    let Some(query) = reference_query_at_position(text, position, context) else {
        return Vec::new();
    };
    references_for_query(text, uri, context, &query)
}

pub fn reference_query_at_position(
    text: &str,
    position: Position,
    context: &AnalysisContext,
) -> Option<ReferenceQuery> {
    let offset = position_to_offset(text, position)?;
    let parsed = parse(text);
    let model = analyze(&parsed, context);
    if let Some(reference) = model
        .references
        .iter()
        .filter(|reference| contains(reference.span, offset))
        .min_by_key(|reference| reference.span.end.saturating_sub(reference.span.start))
    {
        return Some(ReferenceQuery {
            name: reference.name.clone(),
            symbol_kinds: symbol_kinds_for_reference(reference.kind).to_vec(),
            reference_kinds: vec![reference.kind],
        });
    }
    model
        .symbols
        .iter()
        .find(|symbol| contains(symbol.span, offset))
        .map(|symbol| ReferenceQuery {
            name: symbol.name.clone(),
            symbol_kinds: vec![symbol.kind],
            reference_kinds: reference_kinds_for_symbol(symbol.kind).to_vec(),
        })
}

pub fn references_for_query(
    text: &str,
    uri: &Url,
    context: &AnalysisContext,
    query: &ReferenceQuery,
) -> Vec<Location> {
    let parsed = parse(text);
    let model = analyze(&parsed, context);
    let mut locations = Vec::new();
    for symbol in &model.symbols {
        if symbol.name == query.name && query.symbol_kinds.contains(&symbol.kind) {
            locations.push(Location {
                uri: uri.clone(),
                range: span_to_range(text, symbol.span),
            });
        }
    }
    for reference in &model.references {
        if reference.name == query.name && query.reference_kinds.contains(&reference.kind) {
            locations.push(Location {
                uri: uri.clone(),
                range: span_to_range(text, reference.span),
            });
        }
    }
    locations
}

pub fn highlights_for_position(
    text: &str,
    position: Position,
    context: &AnalysisContext,
) -> Vec<DocumentHighlight> {
    references_for_position(
        text,
        &Url::parse("file:///__tcsh_lsp_internal__").unwrap(),
        position,
        context,
    )
    .into_iter()
    .map(|location| DocumentHighlight {
        range: location.range,
        kind: Some(DocumentHighlightKind::READ),
    })
    .collect()
}

pub fn folding_ranges_for_text(text: &str) -> Vec<FoldingRange> {
    let parsed = parse(text);
    let mut ranges = Vec::new();
    collect_folding(text, &parsed.root, &mut ranges);
    ranges
}

pub fn semantic_token_legend() -> SemanticTokensLegend {
    SemanticTokensLegend {
        token_types: vec![
            SemanticTokenType::KEYWORD,
            SemanticTokenType::VARIABLE,
            SemanticTokenType::STRING,
            SemanticTokenType::COMMENT,
            SemanticTokenType::OPERATOR,
            SemanticTokenType::FUNCTION,
            SemanticTokenType::MACRO,
        ],
        token_modifiers: vec![
            SemanticTokenModifier::DECLARATION,
            SemanticTokenModifier::DEFAULT_LIBRARY,
        ],
    }
}

pub fn semantic_tokens_for_text(text: &str) -> SemanticTokens {
    let lexed = lex(text);
    let mut absolute_tokens = Vec::new();
    for token in lexed.tokens {
        let Some(token_type) = semantic_type_for_token(&token.kind, &token.text) else {
            continue;
        };
        push_semantic_token_segments(
            text,
            token.span.start,
            token.span.end,
            token_type,
            0,
            &mut absolute_tokens,
        );
    }
    absolute_tokens.sort_by_key(|token| (token.line, token.start));

    let mut last_line = 0_u32;
    let mut last_start = 0_u32;
    let data = absolute_tokens
        .into_iter()
        .map(|absolute| {
            let delta_line = absolute.line.saturating_sub(last_line);
            let delta_start = if delta_line == 0 {
                absolute.start.saturating_sub(last_start)
            } else {
                absolute.start
            };
            last_line = absolute.line;
            last_start = absolute.start;
            SemanticToken {
                delta_line,
                delta_start,
                length: absolute.length,
                token_type: absolute.token_type,
                token_modifiers_bitset: absolute.modifiers,
            }
        })
        .collect();

    SemanticTokens {
        result_id: None,
        data,
    }
}

pub fn selection_range_for_position(text: &str, position: Position) -> Option<SelectionRange> {
    let offset = position_to_offset(text, position)?;
    let parsed = parse(text);
    smallest_node_containing(&parsed.root, offset).map(|node| SelectionRange {
        range: span_to_range(text, node.span),
        parent: None,
    })
}

#[allow(deprecated)]
pub fn symbol_information_for_text(
    text: &str,
    uri: &Url,
    context: &AnalysisContext,
) -> Vec<SymbolInformation> {
    let parsed = parse(text);
    let model = analyze(&parsed, context);
    model
        .symbols
        .into_iter()
        .map(|symbol| SymbolInformation {
            name: symbol.name,
            kind: match symbol.kind {
                SymbolKind::ShellVariable | SymbolKind::LoopVariable => LspSymbolKind::VARIABLE,
                SymbolKind::EnvironmentVariable => LspSymbolKind::CONSTANT,
                SymbolKind::Alias => LspSymbolKind::FUNCTION,
                SymbolKind::Label => LspSymbolKind::KEY,
                SymbolKind::SourceFile => LspSymbolKind::FILE,
            },
            tags: None,
            deprecated: None,
            location: Location {
                uri: uri.clone(),
                range: span_to_range(text, symbol.span),
            },
            container_name: None,
        })
        .collect()
}

fn collect_folding(text: &str, node: &Node, ranges: &mut Vec<FoldingRange>) {
    if matches!(
        node.kind,
        NodeKind::IfBlock | NodeKind::ForeachBlock | NodeKind::WhileBlock | NodeKind::SwitchBlock
    ) {
        let range = span_to_range(text, node.span);
        if range.end.line > range.start.line {
            ranges.push(FoldingRange {
                start_line: range.start.line,
                start_character: Some(range.start.character),
                end_line: range.end.line,
                end_character: Some(range.end.character),
                kind: Some(FoldingRangeKind::Region),
                collapsed_text: None,
            });
        }
    }
    for child in &node.children {
        collect_folding(text, child, ranges);
    }
}

#[derive(Debug, Clone, Copy)]
struct AbsoluteSemanticToken {
    line: u32,
    start: u32,
    length: u32,
    token_type: u32,
    modifiers: u32,
}

fn semantic_type_for_token(kind: &TokenKind, text: &str) -> Option<u32> {
    match kind {
        TokenKind::Comment => Some(3),
        TokenKind::SingleQuotedString
        | TokenKind::DoubleQuotedString
        | TokenKind::BacktickString => Some(2),
        TokenKind::VariableExpansion => Some(1),
        TokenKind::Separator
        | TokenKind::CommandSubstitution
        | TokenKind::Redirection
        | TokenKind::Pipe
        | TokenKind::LeftParen
        | TokenKind::RightParen
        | TokenKind::Glob => Some(4),
        TokenKind::Label => Some(6),
        TokenKind::Word if is_tcsh_keyword(text) => Some(0),
        TokenKind::Word => Some(5),
        _ => None,
    }
}

fn push_semantic_token_segments(
    text: &str,
    span_start: usize,
    span_end: usize,
    token_type: u32,
    modifiers: u32,
    out: &mut Vec<AbsoluteSemanticToken>,
) {
    let mut segment_start = span_start;
    for (relative, ch) in text[span_start..span_end].char_indices() {
        if ch == '\n' {
            push_single_line_semantic_token(
                text,
                segment_start,
                span_start + relative,
                token_type,
                modifiers,
                out,
            );
            segment_start = span_start + relative + ch.len_utf8();
        }
    }
    push_single_line_semantic_token(text, segment_start, span_end, token_type, modifiers, out);
}

fn push_single_line_semantic_token(
    text: &str,
    start: usize,
    end: usize,
    token_type: u32,
    modifiers: u32,
    out: &mut Vec<AbsoluteSemanticToken>,
) {
    if start >= end {
        return;
    }
    let start_position = offset_to_position(text, start);
    let end_position = offset_to_position(text, end);
    if start_position.line != end_position.line
        || end_position.character <= start_position.character
    {
        return;
    }
    out.push(AbsoluteSemanticToken {
        line: start_position.line,
        start: start_position.character,
        length: end_position.character - start_position.character,
        token_type,
        modifiers,
    });
}

fn is_tcsh_keyword(text: &str) -> bool {
    matches!(
        text.to_ascii_lowercase().as_str(),
        "if" | "then"
            | "else"
            | "endif"
            | "foreach"
            | "while"
            | "switch"
            | "case"
            | "default"
            | "breaksw"
            | "endsw"
            | "end"
            | "goto"
            | "alias"
            | "unalias"
            | "set"
            | "unset"
            | "setenv"
            | "unsetenv"
            | "source"
            | "cd"
            | "pushd"
            | "popd"
            | "exit"
    )
}

fn symbol_kinds_for_reference(kind: ReferenceKind) -> &'static [SymbolKind] {
    match kind {
        ReferenceKind::VariableUse => &[
            SymbolKind::ShellVariable,
            SymbolKind::EnvironmentVariable,
            SymbolKind::LoopVariable,
        ],
        ReferenceKind::AliasUse => &[SymbolKind::Alias],
        ReferenceKind::GotoLabel => &[SymbolKind::Label],
        ReferenceKind::SourceTarget => &[SymbolKind::SourceFile],
    }
}

fn reference_kinds_for_symbol(kind: SymbolKind) -> &'static [ReferenceKind] {
    match kind {
        SymbolKind::ShellVariable | SymbolKind::EnvironmentVariable | SymbolKind::LoopVariable => {
            &[ReferenceKind::VariableUse]
        }
        SymbolKind::Alias => &[ReferenceKind::AliasUse],
        SymbolKind::Label => &[ReferenceKind::GotoLabel],
        SymbolKind::SourceFile => &[ReferenceKind::SourceTarget],
    }
}

fn smallest_node_containing(node: &Node, offset: usize) -> Option<&Node> {
    if !contains(node.span, offset) {
        return None;
    }
    node.children
        .iter()
        .filter_map(|child| smallest_node_containing(child, offset))
        .min_by_key(|child| child.span.end.saturating_sub(child.span.start))
        .or(Some(node))
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
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
    fn read_only_features_return_low_noise_results() {
        let text = "set foo = bar\nif ( $foo ) then\n echo $foo\nendif\n";
        let uri = Url::parse("file:///tmp/test.tcsh").unwrap();
        assert!(
            references_for_position(
                text,
                &uri,
                Position {
                    line: 2,
                    character: 8
                },
                &context()
            )
            .len()
                >= 2
        );
        assert!(
            !highlights_for_position(
                text,
                Position {
                    line: 2,
                    character: 8
                },
                &context()
            )
            .is_empty()
        );
        assert_eq!(folding_ranges_for_text(text).len(), 1);
        assert!(
            selection_range_for_position(
                text,
                Position {
                    line: 1,
                    character: 1
                }
            )
            .is_some()
        );
        assert!(
            symbol_information_for_text(text, &uri, &context())
                .iter()
                .any(|symbol| symbol.name == "foo")
        );
        assert!(!semantic_tokens_for_text(text).data.is_empty());
        assert_eq!(
            semantic_token_legend().token_types[0],
            SemanticTokenType::KEYWORD
        );
    }
}
