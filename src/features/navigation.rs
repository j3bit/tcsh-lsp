use crate::semantics::{AnalysisContext, ReferenceKind, Symbol, SymbolKind, analyze};
use crate::syntax::parser::{Span, parse};
use crate::util::{position_to_offset, span_to_range};
use tower_lsp::lsp_types::{Hover, HoverContents, Location, MarkedString, Position, Range, Url};

pub fn definition_for_position(
    text: &str,
    uri: &Url,
    position: Position,
    context: &AnalysisContext,
) -> Option<Location> {
    let offset = position_to_offset(text, position)?;
    let parsed = parse(text);
    let model = analyze(&parsed, context);

    for reference in &model.references {
        if contains(reference.span, offset) {
            if reference.kind == ReferenceKind::SourceTarget {
                if let Some(edge) = model
                    .source_edges
                    .iter()
                    .find(|edge| contains(edge.span, offset))
                {
                    if let Some(path) = &edge.resolved {
                        if let Ok(target_uri) = Url::from_file_path(path) {
                            return Some(Location {
                                uri: target_uri,
                                range: Range::default(),
                            });
                        }
                    }
                }
            }
            if let Some(symbol) =
                matching_symbol(&model.symbols, reference.name.as_str(), reference.kind)
            {
                return Some(Location {
                    uri: uri.clone(),
                    range: span_to_range(text, symbol.span),
                });
            }
        }
    }
    None
}

pub fn hover_for_position(
    text: &str,
    position: Position,
    context: &AnalysisContext,
) -> Option<Hover> {
    let offset = position_to_offset(text, position)?;
    let parsed = parse(text);
    let model = analyze(&parsed, context);

    for symbol in &model.symbols {
        if contains(symbol.span, offset) {
            return Some(hover(
                format!(
                    "{:?} `{}` defined here ({:?})",
                    symbol.kind, symbol.name, symbol.confidence
                ),
                text,
                symbol.span,
            ));
        }
    }
    model
        .references
        .iter()
        .filter(|reference| contains(reference.span, offset))
        .min_by_key(|reference| reference.span.end.saturating_sub(reference.span.start))
        .map(|reference| {
            hover(
                format!(
                    "{:?} `{}` reference ({:?})",
                    reference.kind, reference.name, reference.confidence
                ),
                text,
                reference.span,
            )
        })
}

fn matching_symbol<'a>(
    symbols: &'a [Symbol],
    name: &str,
    reference_kind: ReferenceKind,
) -> Option<&'a Symbol> {
    let wanted = match reference_kind {
        ReferenceKind::VariableUse => &[
            SymbolKind::ShellVariable,
            SymbolKind::EnvironmentVariable,
            SymbolKind::LoopVariable,
        ][..],
        ReferenceKind::AliasUse => &[SymbolKind::Alias][..],
        ReferenceKind::GotoLabel => &[SymbolKind::Label][..],
        ReferenceKind::SourceTarget => &[SymbolKind::SourceFile][..],
    };
    symbols
        .iter()
        .find(|symbol| symbol.name == name && wanted.contains(&symbol.kind))
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}

fn hover(value: String, text: &str, span: Span) -> Hover {
    Hover {
        contents: HoverContents::Scalar(MarkedString::String(value)),
        range: Some(span_to_range(text, span)),
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
    fn definition_and_hover_find_variable_symbol() {
        let text = "set foo = bar\necho $foo\n";
        let uri = Url::parse("file:///tmp/test.tcsh").unwrap();
        let def = definition_for_position(
            text,
            &uri,
            Position {
                line: 1,
                character: 7,
            },
            &context(),
        )
        .unwrap();
        assert_eq!(def.range.start.line, 0);
        let hover = hover_for_position(
            text,
            Position {
                line: 1,
                character: 7,
            },
            &context(),
        )
        .unwrap();
        assert!(format!("{:?}", hover.contents).contains("foo"));
    }
}
