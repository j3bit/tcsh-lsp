use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub start: usize,
    pub end: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
    pub text: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TokenKind {
    Comment,
    Whitespace,
    Newline,
    Separator,
    SingleQuotedString,
    DoubleQuotedString,
    BacktickString,
    Escape,
    Glob,
    VariableExpansion,
    CommandSubstitution,
    Redirection,
    Pipe,
    LeftParen,
    RightParen,
    Label,
    Word,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LexError {
    pub code: LexErrorCode,
    pub span: Span,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LexErrorCode {
    UnterminatedSingleQuote,
    UnterminatedDoubleQuote,
    UnterminatedBacktick,
    MalformedVariableExpansion,
    UnbalancedCommandSubstitution,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LexResult {
    pub tokens: Vec<Token>,
    pub errors: Vec<LexError>,
}

pub fn lex(input: &str) -> LexResult {
    let mut lexer = Lexer {
        input,
        cursor: 0,
        tokens: Vec::new(),
        errors: Vec::new(),
        at_command_start: true,
    };
    lexer.run();
    LexResult {
        tokens: lexer.tokens,
        errors: lexer.errors,
    }
}

struct Lexer<'a> {
    input: &'a str,
    cursor: usize,
    tokens: Vec<Token>,
    errors: Vec<LexError>,
    at_command_start: bool,
}

impl Lexer<'_> {
    fn run(&mut self) {
        while !self.is_eof() {
            let start = self.cursor;
            let Some(ch) = self.peek_char() else { break };
            match ch {
                '\n' => {
                    self.bump_char();
                    self.push(TokenKind::Newline, start, self.cursor);
                    self.at_command_start = true;
                }
                ' ' | '\t' | '\r' => {
                    self.consume_while(start, TokenKind::Whitespace, |c| {
                        matches!(c, ' ' | '\t' | '\r')
                    });
                }
                '#' => self.consume_comment(start),
                ';' | '&' => {
                    self.bump_char();
                    if ch == '&' && self.peek_char() == Some('&') {
                        self.bump_char();
                    }
                    self.push(TokenKind::Separator, start, self.cursor);
                    self.at_command_start = true;
                }
                '|' => self.consume_pipe(start),
                '<' | '>' => self.consume_redirection(start),
                '(' => {
                    self.bump_char();
                    self.push(TokenKind::LeftParen, start, self.cursor);
                    self.at_command_start = false;
                }
                ')' => {
                    self.bump_char();
                    self.push(TokenKind::RightParen, start, self.cursor);
                    self.at_command_start = false;
                }
                '\\' => self.consume_escape(start),
                '\'' => self.consume_single_quote(start),
                '"' => self.consume_double_quote(start),
                '`' => self.consume_backtick(start),
                '$' => self.consume_dollar(start),
                '!' => self.consume_history_expansion(start),
                '*' | '?' | '[' | ']' | '{' | '}' => {
                    self.bump_char();
                    self.push(TokenKind::Glob, start, self.cursor);
                    self.at_command_start = false;
                }
                _ => self.consume_word_or_label(start),
            }
        }
    }

    fn consume_comment(&mut self, start: usize) {
        while let Some(ch) = self.peek_char() {
            if ch == '\n' {
                break;
            }
            self.bump_char();
        }
        self.push(TokenKind::Comment, start, self.cursor);
    }

    fn consume_pipe(&mut self, start: usize) {
        self.bump_char();
        if self.peek_char() == Some('&') {
            self.bump_char();
        }
        self.push(TokenKind::Pipe, start, self.cursor);
        self.at_command_start = true;
    }

    fn consume_redirection(&mut self, start: usize) {
        self.bump_char();
        while matches!(self.peek_char(), Some('>' | '<' | '&' | '!')) {
            self.bump_char();
        }
        self.push(TokenKind::Redirection, start, self.cursor);
        self.at_command_start = false;
    }

    fn consume_escape(&mut self, start: usize) {
        self.bump_char();
        if !self.is_eof() {
            self.bump_char();
        }
        self.push(TokenKind::Escape, start, self.cursor);
        self.at_command_start = false;
    }

    fn consume_single_quote(&mut self, start: usize) {
        self.bump_char();
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            if ch == '\'' {
                self.push(TokenKind::SingleQuotedString, start, self.cursor);
                self.at_command_start = false;
                return;
            }
        }
        self.push_error_token(
            TokenKind::Error,
            start,
            LexErrorCode::UnterminatedSingleQuote,
            "unterminated single quote",
        );
    }

    fn consume_double_quote(&mut self, start: usize) {
        self.bump_char();
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            if ch == '\\' && !self.is_eof() {
                self.bump_char();
                continue;
            }
            if ch == '"' {
                self.push(TokenKind::DoubleQuotedString, start, self.cursor);
                self.at_command_start = false;
                return;
            }
        }
        self.push_error_token(
            TokenKind::Error,
            start,
            LexErrorCode::UnterminatedDoubleQuote,
            "unterminated double quote",
        );
    }

    fn consume_backtick(&mut self, start: usize) {
        self.bump_char();
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            if ch == '\\' && !self.is_eof() {
                self.bump_char();
                continue;
            }
            if ch == '`' {
                self.push(TokenKind::BacktickString, start, self.cursor);
                self.at_command_start = false;
                return;
            }
        }
        self.push_error_token(
            TokenKind::Error,
            start,
            LexErrorCode::UnterminatedBacktick,
            "unterminated backtick command substitution",
        );
    }

    fn consume_history_expansion(&mut self, start: usize) {
        self.bump_char();
        if self.peek_char() == Some('$') && self.dollar_starts_expansion_after_bang() {
            self.push(TokenKind::Word, start, self.cursor);
            self.at_command_start = false;
            return;
        }
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace()
                || matches!(
                    ch,
                    '#' | ';' | '&' | '|' | '<' | '>' | '(' | ')' | '\'' | '"' | '`' | '\\'
                )
            {
                break;
            }
            self.bump_char();
        }
        self.push(TokenKind::Word, start, self.cursor);
        self.at_command_start = false;
    }

    fn dollar_starts_expansion_after_bang(&self) -> bool {
        let after_dollar = self.cursor + '$'.len_utf8();
        let Some(ch) = self.input[after_dollar..].chars().next() else {
            return false;
        };
        matches!(ch, '{' | '(') || is_var_start(ch) || matches!(ch, '?' | '#' | '$' | '<')
    }

    fn consume_dollar(&mut self, start: usize) {
        self.bump_char();
        match self.peek_char() {
            Some('(') => self.consume_command_substitution(start),
            Some('{') => self.consume_braced_variable(start),
            Some(ch) if is_var_start(ch) || matches!(ch, '?' | '#' | '$' | '<') => {
                self.bump_char();
                while self.peek_char().is_some_and(is_var_continue) {
                    self.bump_char();
                }
                self.push(TokenKind::VariableExpansion, start, self.cursor);
                self.at_command_start = false;
            }
            _ => self.push_error_token(
                TokenKind::Error,
                start,
                LexErrorCode::MalformedVariableExpansion,
                "malformed variable expansion",
            ),
        }
    }

    fn consume_braced_variable(&mut self, start: usize) {
        self.bump_char();
        let name_start = self.cursor;
        while self.peek_char().is_some_and(is_var_continue) {
            self.bump_char();
        }
        if self.cursor == name_start || self.peek_char() != Some('}') {
            while let Some(ch) = self.peek_char() {
                if ch == '\n' {
                    break;
                }
                self.bump_char();
                if ch == '}' {
                    break;
                }
            }
            self.push_error_token(
                TokenKind::Error,
                start,
                LexErrorCode::MalformedVariableExpansion,
                "malformed braced variable expansion",
            );
            return;
        }
        self.bump_char();
        self.push(TokenKind::VariableExpansion, start, self.cursor);
        self.at_command_start = false;
    }

    fn consume_command_substitution(&mut self, start: usize) {
        self.bump_char();
        let mut depth = 1usize;
        while let Some(ch) = self.peek_char() {
            self.bump_char();
            match ch {
                '(' => depth += 1,
                ')' => {
                    depth -= 1;
                    if depth == 0 {
                        self.push(TokenKind::CommandSubstitution, start, self.cursor);
                        self.at_command_start = false;
                        return;
                    }
                }
                '\\' if !self.is_eof() => {
                    self.bump_char();
                }
                _ => {}
            }
        }
        self.push_error_token(
            TokenKind::Error,
            start,
            LexErrorCode::UnbalancedCommandSubstitution,
            "unbalanced command substitution",
        );
    }

    fn consume_word_or_label(&mut self, start: usize) {
        while let Some(ch) = self.peek_char() {
            if ch.is_whitespace()
                || matches!(
                    ch,
                    '#' | ';'
                        | '&'
                        | '|'
                        | '<'
                        | '>'
                        | '('
                        | ')'
                        | '\''
                        | '"'
                        | '`'
                        | '$'
                        | '\\'
                        | '*'
                        | '?'
                        | '['
                        | ']'
                )
            {
                break;
            }
            self.bump_char();
        }
        if self.cursor == start {
            self.bump_char();
        }
        if self.at_command_start
            && (self.peek_char() == Some(':') || self.input[start..self.cursor].ends_with(':'))
        {
            if self.peek_char() == Some(':') {
                self.bump_char();
            }
            self.push(TokenKind::Label, start, self.cursor);
        } else {
            self.push(TokenKind::Word, start, self.cursor);
        }
        self.at_command_start = false;
    }

    fn consume_while(&mut self, start: usize, kind: TokenKind, mut pred: impl FnMut(char) -> bool) {
        while self.peek_char().is_some_and(&mut pred) {
            self.bump_char();
        }
        self.push(kind, start, self.cursor);
    }

    fn push(&mut self, kind: TokenKind, start: usize, end: usize) {
        self.tokens.push(Token {
            kind,
            span: Span { start, end },
            text: self.input[start..end].to_string(),
        });
    }

    fn push_error_token(
        &mut self,
        kind: TokenKind,
        start: usize,
        code: LexErrorCode,
        message: &'static str,
    ) {
        self.push(kind, start, self.cursor);
        self.errors.push(LexError {
            code,
            span: Span {
                start,
                end: self.cursor,
            },
            message: message.to_string(),
        });
        self.at_command_start = false;
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.cursor..].chars().next()
    }

    fn bump_char(&mut self) -> Option<char> {
        let ch = self.peek_char()?;
        self.cursor += ch.len_utf8();
        Some(ch)
    }

    fn is_eof(&self) -> bool {
        self.cursor >= self.input.len()
    }
}

fn is_var_start(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphabetic()
}
fn is_var_continue(ch: char) -> bool {
    ch == '_' || ch.is_ascii_alphanumeric()
}

pub fn format_tokens_for_golden(result: &LexResult) -> String {
    let mut out = String::new();
    for token in &result.tokens {
        out.push_str(&format!(
            "{:?} {}..{} {:?}\n",
            token.kind, token.span.start, token.span.end, token.text
        ));
    }
    if !result.errors.is_empty() {
        out.push_str("errors:\n");
        for error in &result.errors {
            out.push_str(&format!(
                "{:?} {}..{} {}\n",
                error.code, error.span.start, error.span.end, error.message
            ));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bang_before_variable_preserves_variable_expansion_token() {
        let result = lex("if ( !$?prompt ) then
echo !$var
");
        assert!(
            result.errors.is_empty(),
            "bang-variable syntax should not produce lex errors: {:#?}",
            result.errors
        );
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::VariableExpansion && token.text == "$?prompt"),
            "! before $?prompt should not swallow the variable expansion: {:#?}",
            result.tokens
        );
        assert!(
            result
                .tokens
                .iter()
                .any(|token| token.kind == TokenKind::VariableExpansion && token.text == "$var"),
            "! before $var should not swallow the variable expansion: {:#?}",
            result.tokens
        );
    }

    #[test]
    fn history_expansions_do_not_emit_malformed_variable_errors() {
        let result = lex("echo !$ !! !:1 !-2 %1\n");
        assert!(
            result.errors.is_empty(),
            "history/job syntax should not produce lex errors: {:#?}",
            result.errors
        );
        assert!(result.tokens.iter().any(|token| token.text == "!$"));
        assert!(result.tokens.iter().any(|token| token.text == "!!"));
        assert!(result.tokens.iter().any(|token| token.text == "!:1"));
    }
}
