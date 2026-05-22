use crate::syntax::lexer::{Token, TokenKind};

#[derive(Debug)]
pub(crate) struct CommandSegment<'a> {
    pub(crate) tokens: Vec<&'a Token>,
    pub(crate) end: usize,
}

pub(crate) fn command_segments(tokens: &[Token]) -> Vec<CommandSegment<'_>> {
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
            TokenKind::Newline => {
                flush_segment(&mut segments, &mut current, token.span.end, &mut last_end);
                paren_depth = 0;
            }
            TokenKind::Separator if should_split_separator(token, paren_depth, &current) => {
                flush_segment(&mut segments, &mut current, token.span.end, &mut last_end);
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
        segments.push(CommandSegment {
            tokens: filtered,
            end,
        });
    }
    segments
}

fn should_split_separator(token: &Token, paren_depth: usize, current: &[&Token]) -> bool {
    match token.text.as_str() {
        ";" | "&" => true,
        "&&" | "||" => paren_depth == 0 || !segment_starts_with_expression_builtin(current),
        _ => paren_depth == 0,
    }
}

fn segment_starts_with_expression_builtin(tokens: &[&Token]) -> bool {
    tokens
        .iter()
        .find(|token| !matches!(token.kind, TokenKind::Whitespace | TokenKind::Comment))
        .is_some_and(|token| {
            token.kind == TokenKind::Word
                && matches!(
                    token.text.to_ascii_lowercase().as_str(),
                    "if" | "while" | "exit"
                )
        })
}

fn flush_segment<'a>(
    segments: &mut Vec<CommandSegment<'a>>,
    current: &mut Vec<&'a Token>,
    end: usize,
    last_end: &mut usize,
) {
    let filtered = trim_trivia(std::mem::take(current));
    if !filtered.is_empty() {
        segments.push(CommandSegment {
            tokens: filtered,
            end,
        });
    }
    *last_end = end;
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::syntax::lexer::lex;

    fn segment_texts(input: &str) -> Vec<String> {
        let lexed = lex(input);
        command_segments(&lexed.tokens)
            .into_iter()
            .map(|segment| {
                segment
                    .tokens
                    .into_iter()
                    .map(|token| token.text.as_str())
                    .collect::<String>()
                    .trim()
                    .to_string()
            })
            .collect()
    }

    #[test]
    fn if_expression_keeps_logical_and_or_inside_parentheses() {
        assert_eq!(
            segment_texts("if ( -e ~/.tcshrc && $?prompt || $?user ) then\nendif\n"),
            vec!["if ( -e ~/.tcshrc && $?prompt || $?user ) then", "endif"]
        );
    }

    #[test]
    fn grouped_command_lists_split_on_sequence_separators() {
        assert_eq!(
            segment_texts("( echo one ; echo two )\n"),
            vec!["( echo one", "echo two )"]
        );
        assert_eq!(
            segment_texts("( echo one & echo two )\n"),
            vec!["( echo one", "echo two )"]
        );
        assert_eq!(
            segment_texts("( echo one && echo two || echo three )\n"),
            vec!["( echo one", "echo two", "echo three )"]
        );
    }

    #[test]
    fn newline_is_always_recovery_boundary_after_unclosed_parenthesis() {
        assert_eq!(
            segment_texts("if ( $x > 0\necho $foo\nendif\n"),
            vec!["if ( $x > 0", "echo $foo", "endif"]
        );
    }
}
