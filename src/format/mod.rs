use crate::util::offset_to_position;
use tower_lsp::lsp_types::{FormattingOptions, Range, TextEdit};

const FORMAT_OFF: &str = "tcsh-lsp format: off";
const FORMAT_ON: &str = "tcsh-lsp format: on";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FormatLineRange {
    pub start_line: u32,
    pub end_line: u32,
}

pub fn document_formatting_edits(text: &str, options: &FormattingOptions) -> Vec<TextEdit> {
    formatting_edits(text, options, None)
}

pub fn range_formatting_edits(
    text: &str,
    options: &FormattingOptions,
    range: Range,
) -> Vec<TextEdit> {
    formatting_edits(
        text,
        options,
        Some(FormatLineRange {
            start_line: range.start.line,
            end_line: range.end.line,
        }),
    )
}

pub fn formatting_edits(
    text: &str,
    options: &FormattingOptions,
    line_range: Option<FormatLineRange>,
) -> Vec<TextEdit> {
    let mut edits = Vec::new();
    let mut indent = 0_usize;
    let mut enabled = true;
    let mut previous_continues = false;
    let tab = indent_unit(options);

    for (line_index, line) in logical_lines(text).into_iter().enumerate() {
        let line_number = line_index as u32;
        let original = &text[line.start..line.content_end];
        let trimmed = original.trim_start_matches([' ', '\t']);
        let leading_len = original.len() - trimmed.len();

        if original.contains(FORMAT_OFF) {
            enabled = false;
            previous_continues = line_continues(original);
            continue;
        }

        if !enabled {
            if original.contains(FORMAT_ON) {
                enabled = true;
            }
            previous_continues = line_continues(original);
            continue;
        }

        if trimmed.is_empty() || previous_continues {
            previous_continues = line_continues(original);
            continue;
        }

        let first = first_word(trimmed);
        let pre_dedent = pre_dedent_for(first);
        indent = indent.saturating_sub(pre_dedent);

        if line_range
            .is_none_or(|range| range.start_line <= line_number && line_number <= range.end_line)
        {
            let wanted = tab.repeat(indent);
            let current = &original[..leading_len];
            if current != wanted {
                edits.push(TextEdit {
                    range: Range {
                        start: offset_to_position(text, line.start),
                        end: offset_to_position(text, line.start + leading_len),
                    },
                    new_text: wanted,
                });
            }
        }

        indent = indent.saturating_add(post_indent_delta(first, trimmed));
        previous_continues = line_continues(original);
    }

    edits
}

fn indent_unit(options: &FormattingOptions) -> String {
    if options.insert_spaces {
        " ".repeat(options.tab_size.max(1) as usize)
    } else {
        "\t".to_string()
    }
}

#[derive(Debug, Clone, Copy)]
struct LogicalLine {
    start: usize,
    content_end: usize,
}

fn logical_lines(text: &str) -> Vec<LogicalLine> {
    let mut lines = Vec::new();
    let mut start = 0_usize;
    for segment in text.split_inclusive('\n') {
        let end = start + segment.len();
        let content_end = if segment.ends_with('\n') {
            end - 1
        } else {
            end
        };
        let content_end = if content_end > start && text.as_bytes()[content_end - 1] == b'\r' {
            content_end - 1
        } else {
            content_end
        };
        lines.push(LogicalLine { start, content_end });
        start = end;
    }
    if start < text.len() || text.is_empty() {
        lines.push(LogicalLine {
            start,
            content_end: text.len(),
        });
    }
    lines
}

fn first_word(trimmed: &str) -> &str {
    trimmed
        .split(|ch: char| ch.is_whitespace() || matches!(ch, '(' | ')' | ';'))
        .find(|word| !word.is_empty())
        .unwrap_or_default()
}

fn pre_dedent_for(first: &str) -> usize {
    match first.to_ascii_lowercase().as_str() {
        "endif" | "end" | "endsw" | "else" | "case" | "default" => 1,
        _ => 0,
    }
}

fn post_indent_delta(first: &str, trimmed: &str) -> usize {
    let lower = first.to_ascii_lowercase();
    match lower.as_str() {
        "if" if trimmed
            .split_whitespace()
            .any(|word| word.eq_ignore_ascii_case("then")) =>
        {
            1
        }
        "foreach" | "while" | "switch" | "else" | "case" | "default" => 1,
        _ => 0,
    }
}

fn line_continues(line: &str) -> bool {
    let trimmed_end = line.trim_end();
    if !trimmed_end.ends_with('\\') {
        return false;
    }
    let slash_count = trimmed_end
        .chars()
        .rev()
        .take_while(|ch| *ch == '\\')
        .count();
    slash_count % 2 == 1
}

#[cfg(test)]
mod tests {
    use super::*;

    fn options() -> FormattingOptions {
        FormattingOptions {
            tab_size: 2,
            insert_spaces: true,
            ..FormattingOptions::default()
        }
    }

    fn apply_edits(text: &str, edits: &[TextEdit]) -> String {
        let mut out = text.to_string();
        for edit in edits.iter().rev() {
            let start =
                crate::util::position_to_offset(&out, edit.range.start).expect("start offset");
            let end = crate::util::position_to_offset(&out, edit.range.end).expect("end offset");
            out.replace_range(start..end, &edit.new_text);
        }
        out
    }

    #[test]
    fn formatter_indents_blocks_and_is_idempotent() {
        let input = "if ( $foo ) then\n echo $foo\nelse\n  echo no\nendif\n";
        let once = apply_edits(input, &document_formatting_edits(input, &options()));
        assert_eq!(
            once,
            "if ( $foo ) then\n  echo $foo\nelse\n  echo no\nendif\n"
        );
        let twice = apply_edits(&once, &document_formatting_edits(&once, &options()));
        assert_eq!(once, twice);
    }

    #[test]
    fn formatter_respects_pragmas_and_continuations() {
        let input = "# tcsh-lsp format: off\nif ( $x ) then\n echo off\nendif\n# tcsh-lsp format: on\nset x = a \\\n    b\n";
        let formatted = apply_edits(input, &document_formatting_edits(input, &options()));
        assert_eq!(formatted, input);
    }
}
