use crate::syntax::parser::{Node, NodeKind, parse};
use crate::util::span_to_range;
use tower_lsp::lsp_types::{DocumentSymbol, SymbolKind};

pub fn document_symbols_for_text(text: &str) -> Vec<DocumentSymbol> {
    let parsed = parse(text);
    parsed
        .root
        .children
        .iter()
        .filter_map(|node| symbol_for_node(text, node))
        .collect()
}

#[allow(deprecated)]
fn symbol_for_node(text: &str, node: &Node) -> Option<DocumentSymbol> {
    let (name, kind) = match &node.kind {
        NodeKind::IfBlock => ("if".to_string(), SymbolKind::NAMESPACE),
        NodeKind::ForeachBlock => ("foreach".to_string(), SymbolKind::NAMESPACE),
        NodeKind::WhileBlock => ("while".to_string(), SymbolKind::NAMESPACE),
        NodeKind::SwitchBlock => ("switch".to_string(), SymbolKind::NAMESPACE),
        NodeKind::Label { name } => (name.clone(), SymbolKind::KEY),
        NodeKind::Alias => (
            second_word(&node.text).unwrap_or_else(|| "alias".to_string()),
            SymbolKind::FUNCTION,
        ),
        NodeKind::Set => (
            second_word(&node.text).unwrap_or_else(|| "set".to_string()),
            SymbolKind::VARIABLE,
        ),
        NodeKind::Setenv => (
            second_word(&node.text).unwrap_or_else(|| "setenv".to_string()),
            SymbolKind::CONSTANT,
        ),
        NodeKind::Source => (
            second_word(&node.text).unwrap_or_else(|| "source".to_string()),
            SymbolKind::FILE,
        ),
        _ => return None,
    };
    Some(DocumentSymbol {
        name,
        detail: Some(node.text.clone()),
        kind,
        tags: None,
        deprecated: None,
        range: span_to_range(text, node.span),
        selection_range: span_to_range(text, node.span),
        children: Some(
            node.children
                .iter()
                .filter_map(|child| symbol_for_node(text, child))
                .collect(),
        ),
    })
}

fn second_word(text: &str) -> Option<String> {
    text.split_whitespace().nth(1).map(|word| {
        word.trim_matches(['\'', '"'])
            .trim_end_matches(':')
            .to_string()
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_hierarchical_block_symbols() {
        let symbols = document_symbols_for_text("if ( $x ) then\nset foo = bar\nendif\nlabel:\n");
        assert_eq!(symbols[0].name, "if");
        assert_eq!(symbols[0].children.as_ref().unwrap()[0].name, "foo");
        assert_eq!(symbols[1].name, "label");
    }
}
