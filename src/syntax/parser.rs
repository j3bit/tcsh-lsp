use crate::syntax::lexer::{Token, TokenKind, lex};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseDiagnostic {
    pub code: ParseDiagnosticCode,
    pub span: Span,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ParseDiagnosticCode {
    UnmatchedEnd,
    MissingEnd,
    LexerError,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub kind: NodeKind,
    pub span: Span,
    pub text: String,
    pub children: Vec<Node>,
    pub diagnostics: Vec<ParseDiagnostic>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeKind {
    Script,
    Command { name: String },
    IfBlock,
    SingleLineIf,
    Else,
    ForeachBlock,
    WhileBlock,
    SwitchBlock,
    Case,
    Default,
    Breaksw,
    Label { name: String },
    Goto,
    Alias,
    Unalias,
    Set,
    Unset,
    Setenv,
    Unsetenv,
    Source,
    DirectoryChange { command: String },
    Exit,
    UnknownEnd { word: String },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ParseResult {
    pub root: Node,
    pub diagnostics: Vec<ParseDiagnostic>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum BlockKind {
    If,
    Foreach,
    While,
    Switch,
}

#[derive(Debug, Clone)]
struct OpenBlock {
    kind: BlockKind,
    path: Vec<usize>,
    opener_span: Span,
}

pub fn parse(input: &str) -> ParseResult {
    let lexed = lex(input);
    let mut root = Node {
        kind: NodeKind::Script,
        span: Span {
            start: 0,
            end: input.len(),
        },
        text: String::new(),
        children: Vec::new(),
        diagnostics: lexed
            .errors
            .iter()
            .map(|err| ParseDiagnostic {
                code: ParseDiagnosticCode::LexerError,
                span: Span {
                    start: err.span.start,
                    end: err.span.end,
                },
                message: err.message.clone(),
            })
            .collect(),
    };
    let mut diagnostics = root.diagnostics.clone();
    let mut stack: Vec<OpenBlock> = Vec::new();

    for segment in command_segments(&lexed.tokens) {
        if segment.tokens.is_empty() {
            continue;
        }
        let node = classify_segment(input, &segment.tokens);
        if is_close_node(&node.kind) {
            if let Some(index) = stack
                .iter()
                .rposition(|open| closes_block(&node.kind, open.kind))
            {
                while stack.len() > index + 1 {
                    let open = stack.pop().expect("open block");
                    let diag = missing_end(open.opener_span, open.kind);
                    diagnostics.push(diag.clone());
                    get_mut_node(&mut root, &open.path).diagnostics.push(diag);
                }
                let open = stack.pop().expect("matching open block");
                get_mut_node(&mut root, &open.path).span.end = segment.end;
                get_mut_node(&mut root, &open.path).children.push(node);
            } else {
                let mut unmatched = node;
                let diag = ParseDiagnostic {
                    code: ParseDiagnosticCode::UnmatchedEnd,
                    span: unmatched.span,
                    message: "unmatched block terminator".to_string(),
                };
                unmatched.diagnostics.push(diag.clone());
                diagnostics.push(diag);
                current_children_mut(&mut root, &stack).push(unmatched);
            }
            continue;
        }

        let opens = block_open_kind(&node.kind);
        current_children_mut(&mut root, &stack).push(node);
        if let Some(kind) = opens {
            let children = current_children_mut(&mut root, &stack);
            let child_index = children.len() - 1;
            let mut path = stack
                .last()
                .map(|open| open.path.clone())
                .unwrap_or_default();
            path.push(child_index);
            let opener_span = get_mut_node(&mut root, &path).span;
            stack.push(OpenBlock {
                kind,
                path,
                opener_span,
            });
        }
    }

    while let Some(open) = stack.pop() {
        let diag = missing_end(open.opener_span, open.kind);
        diagnostics.push(diag.clone());
        get_mut_node(&mut root, &open.path).diagnostics.push(diag);
    }

    ParseResult { root, diagnostics }
}

#[derive(Debug)]
struct Segment<'a> {
    tokens: Vec<&'a Token>,
    end: usize,
}

fn command_segments(tokens: &[Token]) -> Vec<Segment<'_>> {
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
            TokenKind::Newline | TokenKind::Separator if paren_depth == 0 => {
                let filtered = trim_trivia(current);
                if !filtered.is_empty() {
                    segments.push(Segment {
                        tokens: filtered,
                        end: token.span.end,
                    });
                }
                current = Vec::new();
                last_end = token.span.end;
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
        segments.push(Segment {
            tokens: filtered,
            end,
        });
    }
    segments
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

fn classify_segment(input: &str, tokens: &[&Token]) -> Node {
    let start = tokens.first().map(|token| token.span.start).unwrap_or(0);
    let end = tokens.last().map(|token| token.span.end).unwrap_or(start);
    let words = significant_words(tokens);
    let first = words.first().map(|word| word.as_str()).unwrap_or_default();
    let lower = first.to_ascii_lowercase();
    let kind = if tokens
        .first()
        .is_some_and(|token| token.kind == TokenKind::Label)
    {
        NodeKind::Label {
            name: first.trim_end_matches(':').to_string(),
        }
    } else {
        match lower.as_str() {
            "if" if words.iter().any(|word| word.eq_ignore_ascii_case("then")) => NodeKind::IfBlock,
            "if" => NodeKind::SingleLineIf,
            "else" => NodeKind::Else,
            "foreach" => NodeKind::ForeachBlock,
            "while" => NodeKind::WhileBlock,
            "switch" => NodeKind::SwitchBlock,
            "case" => NodeKind::Case,
            "default" => NodeKind::Default,
            "breaksw" => NodeKind::Breaksw,
            "endif" | "end" | "endsw" => NodeKind::UnknownEnd {
                word: lower.clone(),
            },
            "goto" => NodeKind::Goto,
            "alias" => NodeKind::Alias,
            "unalias" => NodeKind::Unalias,
            "set" => NodeKind::Set,
            "unset" => NodeKind::Unset,
            "setenv" => NodeKind::Setenv,
            "unsetenv" => NodeKind::Unsetenv,
            "source" => NodeKind::Source,
            "cd" | "pushd" | "popd" => NodeKind::DirectoryChange {
                command: lower.clone(),
            },
            "exit" => NodeKind::Exit,
            _ => NodeKind::Command {
                name: first.to_string(),
            },
        }
    };
    Node {
        kind,
        span: Span { start, end },
        text: input[start..end].trim().to_string(),
        children: Vec::new(),
        diagnostics: Vec::new(),
    }
}

fn significant_words(tokens: &[&Token]) -> Vec<String> {
    tokens
        .iter()
        .filter(|token| !matches!(token.kind, TokenKind::Whitespace | TokenKind::Comment))
        .map(|token| token.text.clone())
        .collect()
}

fn block_open_kind(kind: &NodeKind) -> Option<BlockKind> {
    match kind {
        NodeKind::IfBlock => Some(BlockKind::If),
        NodeKind::ForeachBlock => Some(BlockKind::Foreach),
        NodeKind::WhileBlock => Some(BlockKind::While),
        NodeKind::SwitchBlock => Some(BlockKind::Switch),
        _ => None,
    }
}

fn is_close_node(kind: &NodeKind) -> bool {
    matches!(kind, NodeKind::UnknownEnd { word } if matches!(word.as_str(), "endif" | "end" | "endsw"))
}

fn closes_block(kind: &NodeKind, open: BlockKind) -> bool {
    match kind {
        NodeKind::UnknownEnd { word } if word == "endif" => open == BlockKind::If,
        NodeKind::UnknownEnd { word } if word == "end" => {
            matches!(open, BlockKind::Foreach | BlockKind::While)
        }
        NodeKind::UnknownEnd { word } if word == "endsw" => open == BlockKind::Switch,
        _ => false,
    }
}

fn missing_end(span: Span, kind: BlockKind) -> ParseDiagnostic {
    let expected = match kind {
        BlockKind::If => "endif",
        BlockKind::Foreach | BlockKind::While => "end",
        BlockKind::Switch => "endsw",
    };
    ParseDiagnostic {
        code: ParseDiagnosticCode::MissingEnd,
        span,
        message: format!("missing {expected} for block"),
    }
}

fn current_children_mut<'a>(root: &'a mut Node, stack: &[OpenBlock]) -> &'a mut Vec<Node> {
    if let Some(open) = stack.last() {
        &mut get_mut_node(root, &open.path).children
    } else {
        &mut root.children
    }
}

fn get_mut_node<'a>(root: &'a mut Node, path: &[usize]) -> &'a mut Node {
    let mut node = root;
    for &index in path {
        node = &mut node.children[index];
    }
    node
}

pub fn format_parse_for_golden(result: &ParseResult) -> String {
    let mut out = String::new();
    format_node(&result.root, 0, &mut out);
    if !result.diagnostics.is_empty() {
        out.push_str("diagnostics:\n");
        for diagnostic in &result.diagnostics {
            out.push_str(&format!(
                "{:?} {}..{} {}\n",
                diagnostic.code, diagnostic.span.start, diagnostic.span.end, diagnostic.message
            ));
        }
    }
    out
}

fn format_node(node: &Node, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    out.push_str(&format!(
        "{}{:?} {}..{} {:?}\n",
        indent, node.kind, node.span.start, node.span.end, node.text
    ));
    for diagnostic in &node.diagnostics {
        out.push_str(&format!(
            "{}  ! {:?} {}..{} {}\n",
            indent, diagnostic.code, diagnostic.span.start, diagnostic.span.end, diagnostic.message
        ));
    }
    for child in &node.children {
        format_node(child, depth + 1, out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn if_condition_with_and_separator_inside_parentheses_stays_one_block() {
        let parsed = parse("if ( -e ~/.tcshrc && $count >= 3 ) then\n  echo ok\nendif\n");
        assert!(
            parsed.diagnostics.is_empty(),
            "if condition should not be split at && inside parentheses: {:#?}",
            parsed.diagnostics
        );
        assert!(matches!(parsed.root.children[0].kind, NodeKind::IfBlock));
    }
}
