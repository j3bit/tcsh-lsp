use crate::config::TcshLspConfig;
use std::collections::HashMap;
use tower_lsp::lsp_types::{Position, Range, Url};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DocumentLanguage {
    Tcsh,
    Csh,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Document {
    pub uri: Url,
    pub version: i32,
    pub language_id: String,
    pub text: String,
    pub language: DocumentLanguage,
    line_index: LineIndex,
}

impl Document {
    pub fn new(uri: Url, version: i32, language_id: String, text: String) -> Self {
        let language = detect_language(&uri, &language_id, &text);
        let line_index = LineIndex::new(&text);
        Self {
            uri,
            version,
            language_id,
            text,
            language,
            line_index,
        }
    }

    pub fn replace_text(&mut self, version: i32, text: String) {
        self.version = version;
        self.language = detect_language(&self.uri, &self.language_id, &text);
        self.line_index = LineIndex::new(&text);
        self.text = text;
    }

    pub fn apply_range_edit(
        &mut self,
        version: i32,
        range: Range,
        replacement: &str,
    ) -> Result<(), DocumentError> {
        let start = self
            .line_index
            .position_to_offset(&self.text, range.start)?;
        let end = self.line_index.position_to_offset(&self.text, range.end)?;
        if start > end {
            return Err(DocumentError::InvalidRange("range start is after end"));
        }
        if !self.text.is_char_boundary(start) || !self.text.is_char_boundary(end) {
            return Err(DocumentError::InvalidRange(
                "range is not on UTF-8 boundary",
            ));
        }
        self.text.replace_range(start..end, replacement);
        self.version = version;
        self.language = detect_language(&self.uri, &self.language_id, &self.text);
        self.line_index = LineIndex::new(&self.text);
        Ok(())
    }
}

#[derive(Debug, Default)]
pub struct DocumentStore {
    docs: HashMap<Url, Document>,
    max_file_size_bytes: usize,
}

impl DocumentStore {
    pub fn new(config: &TcshLspConfig) -> Self {
        Self {
            docs: HashMap::new(),
            max_file_size_bytes: config.max_file_size_bytes,
        }
    }

    pub fn open(
        &mut self,
        uri: Url,
        version: i32,
        language_id: String,
        text: String,
    ) -> Result<(), DocumentError> {
        self.check_size(&text)?;
        let doc = Document::new(uri.clone(), version, language_id, text);
        self.docs.insert(uri, doc);
        Ok(())
    }

    pub fn apply_full_change(
        &mut self,
        uri: &Url,
        version: i32,
        text: String,
    ) -> Result<(), DocumentError> {
        self.check_size(&text)?;
        let doc = self.docs.get_mut(uri).ok_or(DocumentError::NotOpen)?;
        doc.replace_text(version, text);
        Ok(())
    }

    pub fn apply_range_change(
        &mut self,
        uri: &Url,
        version: i32,
        range: Range,
        text: &str,
    ) -> Result<(), DocumentError> {
        let doc = self.docs.get_mut(uri).ok_or(DocumentError::NotOpen)?;
        doc.apply_range_edit(version, range, text)?;
        if doc.text.len() > self.max_file_size_bytes {
            return Err(DocumentError::FileTooLarge {
                size: doc.text.len(),
                max: self.max_file_size_bytes,
            });
        }
        Ok(())
    }

    pub fn close(&mut self, uri: &Url) {
        self.docs.remove(uri);
    }

    pub fn get(&self, uri: &Url) -> Option<&Document> {
        self.docs.get(uri)
    }

    pub fn len(&self) -> usize {
        self.docs.len()
    }

    pub fn is_empty(&self) -> bool {
        self.docs.is_empty()
    }

    pub fn documents(&self) -> impl Iterator<Item = &Document> {
        self.docs.values()
    }

    fn check_size(&self, text: &str) -> Result<(), DocumentError> {
        if text.len() > self.max_file_size_bytes {
            return Err(DocumentError::FileTooLarge {
                size: text.len(),
                max: self.max_file_size_bytes,
            });
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DocumentError {
    NotOpen,
    FileTooLarge { size: usize, max: usize },
    InvalidPosition(&'static str),
    InvalidRange(&'static str),
}

impl std::fmt::Display for DocumentError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotOpen => write!(f, "document is not open"),
            Self::FileTooLarge { size, max } => {
                write!(f, "document is too large: {size} bytes exceeds {max} bytes")
            }
            Self::InvalidPosition(message) => write!(f, "invalid position: {message}"),
            Self::InvalidRange(message) => write!(f, "invalid range: {message}"),
        }
    }
}

impl std::error::Error for DocumentError {}

#[derive(Debug, Clone, PartialEq, Eq)]
struct LineIndex {
    line_starts: Vec<usize>,
}

impl LineIndex {
    fn new(text: &str) -> Self {
        let mut line_starts = vec![0];
        for (idx, ch) in text.char_indices() {
            if ch == '\n' {
                line_starts.push(idx + ch.len_utf8());
            }
        }
        Self { line_starts }
    }

    fn position_to_offset(&self, text: &str, position: Position) -> Result<usize, DocumentError> {
        let line = position.line as usize;
        let Some(&line_start) = self.line_starts.get(line) else {
            return Err(DocumentError::InvalidPosition("line out of bounds"));
        };
        let line_end = self
            .line_starts
            .get(line + 1)
            .copied()
            .unwrap_or(text.len());
        let line_text = &text[line_start..line_end];
        let target_utf16 = position.character as usize;
        let mut seen_utf16 = 0;
        for (relative, ch) in line_text.char_indices() {
            if seen_utf16 == target_utf16 {
                return Ok(line_start + relative);
            }
            seen_utf16 += ch.len_utf16();
            if seen_utf16 > target_utf16 {
                return Err(DocumentError::InvalidPosition(
                    "character splits a UTF-16 surrogate pair",
                ));
            }
        }
        if seen_utf16 == target_utf16 {
            Ok(line_end)
        } else {
            Err(DocumentError::InvalidPosition("character out of bounds"))
        }
    }
}

pub fn detect_language(uri: &Url, language_id: &str, text: &str) -> DocumentLanguage {
    match language_id.to_ascii_lowercase().as_str() {
        "tcsh" => return DocumentLanguage::Tcsh,
        "csh" => return DocumentLanguage::Csh,
        _ => {}
    }

    if let Ok(path) = uri.to_file_path() {
        if let Some(name) = path.file_name().and_then(|name| name.to_str()) {
            match name {
                ".tcshrc" => return DocumentLanguage::Tcsh,
                ".cshrc" => return DocumentLanguage::Csh,
                _ => {}
            }
        }
        if let Some(extension) = path.extension().and_then(|extension| extension.to_str()) {
            match extension {
                "tcsh" => return DocumentLanguage::Tcsh,
                "csh" => return DocumentLanguage::Csh,
                _ => {}
            }
        }
    }

    let first_line = text.lines().next().unwrap_or_default().to_ascii_lowercase();
    if first_line.starts_with("#!") {
        if first_line.contains("tcsh") {
            return DocumentLanguage::Tcsh;
        }
        if first_line.contains("csh") {
            return DocumentLanguage::Csh;
        }
    }

    DocumentLanguage::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;
    use tower_lsp::lsp_types::{Position, Range};

    fn uri(path: &str) -> Url {
        Url::parse(&format!("file:///tmp/{path}")).unwrap()
    }

    #[test]
    fn detects_extensions_rc_names_language_id_and_shebangs() {
        assert_eq!(
            detect_language(&uri("x.tcsh"), "", ""),
            DocumentLanguage::Tcsh
        );
        assert_eq!(
            detect_language(&uri("x.csh"), "", ""),
            DocumentLanguage::Csh
        );
        assert_eq!(
            detect_language(&uri(".tcshrc"), "", ""),
            DocumentLanguage::Tcsh
        );
        assert_eq!(
            detect_language(&uri(".cshrc"), "", ""),
            DocumentLanguage::Csh
        );
        assert_eq!(
            detect_language(&uri("x"), "tcsh", ""),
            DocumentLanguage::Tcsh
        );
        assert_eq!(detect_language(&uri("x"), "csh", ""), DocumentLanguage::Csh);
        assert_eq!(
            detect_language(&uri("x"), "", "#!/usr/bin/env tcsh\necho ok\n"),
            DocumentLanguage::Tcsh
        );
        assert_eq!(
            detect_language(&uri("x"), "", "#!/bin/csh -f\necho ok\n"),
            DocumentLanguage::Csh
        );
    }

    #[test]
    fn applies_utf16_range_edits_for_surrogates_combining_and_crlf() {
        let mut doc = Document::new(
            uri("x.tcsh"),
            1,
            "tcsh".to_string(),
            "a😀e\u{301}\r\nsecond".to_string(),
        );
        doc.apply_range_edit(
            2,
            Range {
                start: Position {
                    line: 0,
                    character: 1,
                },
                end: Position {
                    line: 0,
                    character: 3,
                },
            },
            "Z",
        )
        .unwrap();
        assert_eq!(doc.text, "aZe\u{301}\r\nsecond");

        doc.apply_range_edit(
            3,
            Range {
                start: Position {
                    line: 0,
                    character: 2,
                },
                end: Position {
                    line: 1,
                    character: 6,
                },
            },
            "X",
        )
        .unwrap();
        assert_eq!(doc.text, "aZX");
    }

    #[test]
    fn rejects_invalid_utf16_position_inside_surrogate() {
        let mut doc = Document::new(uri("x.tcsh"), 1, "tcsh".to_string(), "a😀b".to_string());
        let err = doc
            .apply_range_edit(
                2,
                Range {
                    start: Position {
                        line: 0,
                        character: 2,
                    },
                    end: Position {
                        line: 0,
                        character: 3,
                    },
                },
                "x",
            )
            .unwrap_err();
        assert!(matches!(err, DocumentError::InvalidPosition(_)));
    }

    #[test]
    fn store_applies_full_and_range_changes_and_enforces_size() {
        let config = TcshLspConfig {
            max_file_size_bytes: 8,
            ..TcshLspConfig::default()
        };
        let mut store = DocumentStore::new(&config);
        let uri = uri("x.csh");
        store
            .open(uri.clone(), 1, "csh".to_string(), "echo".to_string())
            .unwrap();
        assert_eq!(store.len(), 1);
        store
            .apply_range_change(
                &uri,
                2,
                Range {
                    start: Position {
                        line: 0,
                        character: 4,
                    },
                    end: Position {
                        line: 0,
                        character: 4,
                    },
                },
                " ok",
            )
            .unwrap();
        assert_eq!(store.get(&uri).unwrap().text, "echo ok");
        let err = store
            .apply_full_change(&uri, 3, "012345678".to_string())
            .unwrap_err();
        assert!(matches!(err, DocumentError::FileTooLarge { .. }));
        store.close(&uri);
        assert!(store.is_empty());
    }
}
