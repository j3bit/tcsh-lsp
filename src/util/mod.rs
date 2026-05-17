use crate::syntax::parser::Span;
use tower_lsp::lsp_types::{Position, Range};

pub fn span_to_range(text: &str, span: Span) -> Range {
    Range {
        start: offset_to_position(text, span.start.min(text.len())),
        end: offset_to_position(text, span.end.min(text.len())),
    }
}

pub fn offset_to_position(text: &str, offset: usize) -> Position {
    let mut line = 0_u32;
    let mut line_start = 0_usize;
    for (idx, ch) in text.char_indices() {
        if idx >= offset {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = idx + ch.len_utf8();
        }
    }
    let character = text[line_start..offset]
        .chars()
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    Position { line, character }
}

pub fn position_to_offset(text: &str, position: Position) -> Option<usize> {
    let mut line = 0_u32;
    let mut line_start = 0_usize;
    for (idx, ch) in text.char_indices() {
        if line == position.line {
            break;
        }
        if ch == '\n' {
            line += 1;
            line_start = idx + ch.len_utf8();
        }
    }
    if line != position.line {
        return None;
    }
    let line_end = text[line_start..]
        .find('\n')
        .map(|relative| line_start + relative)
        .unwrap_or(text.len());
    let mut seen_utf16 = 0_u32;
    for (relative, ch) in text[line_start..line_end].char_indices() {
        if seen_utf16 == position.character {
            return Some(line_start + relative);
        }
        seen_utf16 += ch.len_utf16() as u32;
        if seen_utf16 > position.character {
            return None;
        }
    }
    (seen_utf16 == position.character).then_some(line_end)
}
