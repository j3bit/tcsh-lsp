use crate::semantics::{AnalysisContext, SymbolKind, analyze};
use crate::syntax::parser::parse;
use tower_lsp::lsp_types::{CompletionItem, CompletionItemKind, InsertTextFormat};

const BUILTINS: &[&str] = &[
    "alias", "breaksw", "case", "cd", "default", "else", "endif", "end", "endsw", "exit",
    "foreach", "goto", "if", "popd", "pushd", "set", "setenv", "source", "switch", "unalias",
    "unset", "unsetenv", "while",
];

pub fn completions_for_text(text: &str, context: &AnalysisContext) -> Vec<CompletionItem> {
    let parsed = parse(text);
    let model = analyze(&parsed, context);
    let mut items = Vec::new();

    for builtin in BUILTINS {
        items.push(CompletionItem {
            label: (*builtin).to_string(),
            kind: Some(CompletionItemKind::KEYWORD),
            detail: Some("tcsh/csh builtin".to_string()),
            ..CompletionItem::default()
        });
    }

    for symbol in &model.symbols {
        let (prefix, kind) = match symbol.kind {
            SymbolKind::ShellVariable | SymbolKind::LoopVariable => {
                ("$", CompletionItemKind::VARIABLE)
            }
            SymbolKind::EnvironmentVariable => ("$", CompletionItemKind::CONSTANT),
            SymbolKind::Alias => ("", CompletionItemKind::FUNCTION),
            SymbolKind::Label => ("", CompletionItemKind::REFERENCE),
            SymbolKind::SourceFile => ("", CompletionItemKind::FILE),
        };
        items.push(CompletionItem {
            label: format!("{prefix}{}", symbol.name),
            kind: Some(kind),
            detail: Some(format!("{:?}", symbol.kind)),
            ..CompletionItem::default()
        });
    }

    for snippet in snippets() {
        items.push(snippet);
    }
    items.sort_by(|a, b| a.label.cmp(&b.label));
    items.dedup_by(|a, b| a.label == b.label);
    items
}

fn snippets() -> Vec<CompletionItem> {
    vec![
        CompletionItem {
            label: "if/then/endif".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("if ( ${1:condition} ) then\n  ${0}\nendif".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        },
        CompletionItem {
            label: "foreach/end".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("foreach ${1:item} ( ${2:list} )\n  ${0}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        },
        CompletionItem {
            label: "while/end".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some("while ( ${1:condition} )\n  ${0}\nend".to_string()),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        },
        CompletionItem {
            label: "switch/endsw".to_string(),
            kind: Some(CompletionItemKind::SNIPPET),
            insert_text: Some(
                "switch ( ${1:value} )\ncase ${2:pattern}:\n  ${0}\n  breaksw\ndefault:\nendsw"
                    .to_string(),
            ),
            insert_text_format: Some(InsertTextFormat::SNIPPET),
            ..CompletionItem::default()
        },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TcshLspConfig;
    use std::path::PathBuf;

    #[test]
    fn completion_includes_symbols_builtins_and_snippets() {
        let context = AnalysisContext::new(PathBuf::from("/tmp"), TcshLspConfig::default());
        let items = completions_for_text("set foo = bar\nalias ll 'ls -l'\n", &context);
        assert!(items.iter().any(|item| item.label == "$foo"));
        assert!(items.iter().any(|item| item.label == "ll"));
        assert!(items.iter().any(|item| item.label == "foreach"));
        assert!(items.iter().any(|item| item.label == "if/then/endif"));
    }
}
