use crate::format::document_formatting_edits;
use crate::semantics::{
    AnalysisContext, Confidence, Reference, ReferenceKind, Symbol, SymbolKind, analyze,
};
use crate::syntax::parser::{Span, parse};
use crate::util::{offset_to_position, position_to_offset, span_to_range};
use std::collections::{HashMap, HashSet};
use tower_lsp::lsp_types::{
    CodeAction, CodeActionKind, CodeActionOrCommand, Diagnostic, FormattingOptions, Location,
    NumberOrString, Position, PrepareRenameResponse, Range, TextEdit, Url, WorkspaceEdit,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameTarget {
    pub name: String,
    pub range: Range,
    pub symbol_kinds: Vec<SymbolKind>,
    pub reference_kinds: Vec<ReferenceKind>,
}

pub fn prepare_rename_for_position(
    text: &str,
    position: Position,
    context: &AnalysisContext,
) -> Option<PrepareRenameResponse> {
    let target = rename_target_at_position(text, position, context)?;
    Some(PrepareRenameResponse::RangeWithPlaceholder {
        range: target.range,
        placeholder: target.name,
    })
}

pub fn rename_target_at_position(
    text: &str,
    position: Position,
    context: &AnalysisContext,
) -> Option<RenameTarget> {
    let offset = position_to_offset(text, position)?;
    let parsed = parse(text);
    let model = analyze(&parsed, context);

    if let Some(symbol) = model
        .symbols
        .iter()
        .filter(|symbol| is_supported_symbol(symbol) && contains(symbol.span, offset))
        .min_by_key(|symbol| symbol.span.end.saturating_sub(symbol.span.start))
    {
        return Some(RenameTarget {
            name: symbol.name.clone(),
            range: span_to_range(text, symbol.span),
            symbol_kinds: vec![symbol.kind],
            reference_kinds: reference_kinds_for_symbol(symbol.kind).to_vec(),
        });
    }

    let reference = model
        .references
        .iter()
        .filter(|reference| is_supported_reference(reference) && contains(reference.span, offset))
        .min_by_key(|reference| reference.span.end.saturating_sub(reference.span.start))?;
    let symbol_kinds = symbol_kinds_for_reference(reference.kind);
    if !model.symbols.iter().any(|symbol| {
        symbol.name == reference.name
            && symbol.confidence == Confidence::Certain
            && symbol_kinds.contains(&symbol.kind)
    }) {
        return None;
    }
    Some(RenameTarget {
        name: reference.name.clone(),
        range: span_to_range(text, reference_name_span(text, reference)),
        symbol_kinds: symbol_kinds.to_vec(),
        reference_kinds: vec![reference.kind],
    })
}

pub fn rename_edits_for_text(
    text: &str,
    target: &RenameTarget,
    new_name: &str,
    context: &AnalysisContext,
) -> Option<Vec<TextEdit>> {
    if !is_valid_rename_name(new_name) {
        return None;
    }
    let parsed = parse(text);
    let model = analyze(&parsed, context);
    let mut seen = HashSet::new();
    let mut edits = Vec::new();

    for symbol in &model.symbols {
        if symbol.name == target.name
            && target.symbol_kinds.contains(&symbol.kind)
            && symbol.confidence == Confidence::Certain
        {
            push_unique_edit(text, symbol.span, new_name, &mut seen, &mut edits);
        }
    }

    for reference in &model.references {
        if reference.name == target.name && target.reference_kinds.contains(&reference.kind) {
            push_unique_edit(
                text,
                reference_name_span(text, reference),
                new_name,
                &mut seen,
                &mut edits,
            );
        }
    }

    edits.sort_by_key(|edit| (edit.range.start.line, edit.range.start.character));
    Some(edits)
}

pub fn workspace_edit_from_changes(changes: HashMap<Url, Vec<TextEdit>>) -> Option<WorkspaceEdit> {
    if changes.values().all(Vec::is_empty) {
        return None;
    }
    Some(WorkspaceEdit {
        changes: Some(changes),
        document_changes: None,
        change_annotations: None,
    })
}

pub fn conservative_code_actions(
    uri: &Url,
    text: &str,
    diagnostics: &[Diagnostic],
    options: &FormattingOptions,
) -> Vec<CodeActionOrCommand> {
    let mut actions = Vec::new();
    for diagnostic in diagnostics {
        if let Some(expected) = missing_end_expected(diagnostic) {
            let insertion = offset_to_position(text, text.len());
            let edit = TextEdit {
                range: Range {
                    start: insertion,
                    end: insertion,
                },
                new_text: if text.ends_with('\n') {
                    format!("{expected}\n")
                } else {
                    format!("\n{expected}\n")
                },
            };
            actions.push(CodeActionOrCommand::CodeAction(CodeAction {
                title: format!("Insert missing {expected}"),
                kind: Some(CodeActionKind::QUICKFIX),
                diagnostics: Some(vec![diagnostic.clone()]),
                edit: workspace_edit_from_changes(HashMap::from([(uri.clone(), vec![edit])])),
                command: None,
                is_preferred: Some(false),
                disabled: None,
                data: None,
            }));
        }
    }

    let format_edits = document_formatting_edits(text, options);
    if !format_edits.is_empty() {
        actions.push(CodeActionOrCommand::CodeAction(CodeAction {
            title: "Format tcsh/csh indentation".to_string(),
            kind: Some(CodeActionKind::SOURCE),
            diagnostics: None,
            edit: workspace_edit_from_changes(HashMap::from([(uri.clone(), format_edits)])),
            command: None,
            is_preferred: Some(false),
            disabled: None,
            data: None,
        }));
    }
    actions
}

fn is_supported_symbol(symbol: &Symbol) -> bool {
    symbol.confidence == Confidence::Certain
        && matches!(
            symbol.kind,
            SymbolKind::ShellVariable
                | SymbolKind::EnvironmentVariable
                | SymbolKind::LoopVariable
                | SymbolKind::Alias
                | SymbolKind::Label
        )
}

fn is_supported_reference(reference: &Reference) -> bool {
    matches!(
        reference.kind,
        ReferenceKind::VariableUse | ReferenceKind::AliasUse | ReferenceKind::GotoLabel
    )
}

fn reference_name_span(text: &str, reference: &Reference) -> Span {
    if reference.kind != ReferenceKind::VariableUse {
        return reference.span;
    }
    let raw = &text[reference.span.start..reference.span.end];
    if raw.starts_with("${") && raw.ends_with('}') && raw.len() >= 4 {
        Span {
            start: reference.span.start + 2,
            end: reference.span.end - 1,
        }
    } else if raw.starts_with('$') && raw.len() >= 2 {
        Span {
            start: reference.span.start + 1,
            end: reference.span.end,
        }
    } else {
        reference.span
    }
}

fn push_unique_edit(
    text: &str,
    span: Span,
    new_name: &str,
    seen: &mut HashSet<(usize, usize)>,
    edits: &mut Vec<TextEdit>,
) {
    if span.start >= span.end || !seen.insert((span.start, span.end)) {
        return;
    }
    edits.push(TextEdit {
        range: span_to_range(text, span),
        new_text: new_name.to_string(),
    });
}

fn is_valid_rename_name(name: &str) -> bool {
    let mut chars = name.chars();
    let Some(first) = chars.next() else {
        return false;
    };
    (first == '_' || first.is_ascii_alphabetic())
        && chars.all(|ch| ch == '_' || ch.is_ascii_alphanumeric())
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

fn missing_end_expected(diagnostic: &Diagnostic) -> Option<&'static str> {
    let code = diagnostic.code.as_ref()?;
    let is_missing_end = matches!(
        code,
        NumberOrString::String(value) if value == "tcsh-lsp.parse.missing_end"
    );
    if !is_missing_end {
        return None;
    }
    if diagnostic.message.contains("endif") {
        Some("endif")
    } else if diagnostic.message.contains("endsw") {
        Some("endsw")
    } else if diagnostic.message.contains("end") {
        Some("end")
    } else {
        None
    }
}

fn contains(span: Span, offset: usize) -> bool {
    span.start <= offset && offset <= span.end
}

#[allow(dead_code)]
fn _location_for_edit(uri: Url, range: Range) -> Location {
    Location { uri, range }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::TcshLspConfig;
    use crate::features::diagnostics::diagnostics_for_text;
    use std::path::PathBuf;

    fn context() -> AnalysisContext {
        AnalysisContext::new(PathBuf::from("/tmp"), TcshLspConfig::default())
    }

    #[test]
    fn prepare_and_rename_variable_use_without_replacing_dollar_prefix() {
        let text = "set foo = bar\necho $foo ${foo}\n";
        let target = rename_target_at_position(
            text,
            Position {
                line: 1,
                character: 8,
            },
            &context(),
        )
        .expect("rename target");
        assert_eq!(target.name, "foo");
        let edits = rename_edits_for_text(text, &target, "baz", &context()).expect("edits");
        assert_eq!(edits.len(), 3);
        assert_eq!(edits[1].range.start.character, 6);
        assert_eq!(edits[1].new_text, "baz");
    }

    #[test]
    fn rename_arithmetic_assignment_preserves_operator_suffix() {
        for text in [
            "@ count++
echo $count
",
            "@ count--
echo $count
",
        ] {
            let target = rename_target_at_position(
                text,
                Position {
                    line: 0,
                    character: 3,
                },
                &context(),
            )
            .expect("arithmetic assignment rename target");
            assert_eq!(target.name, "count");

            let edits = rename_edits_for_text(text, &target, "total", &context()).expect("edits");
            assert_eq!(edits.len(), 2);
            assert_eq!(edits[0].range.start.character, 2);
            assert_eq!(edits[0].range.end.character, 7);
            assert_eq!(edits[0].new_text, "total");
        }
    }

    #[test]
    fn rename_alias_use_edits_command_word_only() {
        let text = "alias ll 'ls -l'\nll /tmp\n";
        let target = rename_target_at_position(
            text,
            Position {
                line: 1,
                character: 1,
            },
            &context(),
        )
        .expect("alias target");
        let edits = rename_edits_for_text(text, &target, "list", &context()).expect("edits");
        assert_eq!(edits.len(), 2);
        assert_eq!(edits[1].range.start.character, 0);
        assert_eq!(edits[1].range.end.character, 2);
    }

    #[test]
    fn code_action_offers_missing_end_fix() {
        let text = "if ( $foo ) then\n echo $foo";
        let uri = Url::parse("file:///tmp/test.tcsh").unwrap();
        let diagnostics = diagnostics_for_text(text, &context());
        let actions = conservative_code_actions(
            &uri,
            text,
            &diagnostics,
            &FormattingOptions {
                tab_size: 2,
                insert_spaces: true,
                ..FormattingOptions::default()
            },
        );
        assert!(actions.iter().any(|action| match action {
            CodeActionOrCommand::CodeAction(action) => {
                action.title.contains("endif")
                    && action
                        .edit
                        .as_ref()
                        .and_then(|edit| edit.changes.as_ref())
                        .and_then(|changes| changes.get(&uri))
                        .is_some_and(|edits| {
                            edits[0].range.start.line == 1 && edits[0].new_text == "\nendif\n"
                        })
            }
            CodeActionOrCommand::Command(_) => false,
        }));
    }
}
