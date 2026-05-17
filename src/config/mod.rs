use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum DialectMode {
    #[default]
    Auto,
    Tcsh,
    Csh,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceIndexingConfig {
    pub enabled: bool,
    pub max_files: usize,
}

impl Default for WorkspaceIndexingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            max_files: 2_000,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct CompletionConfig {
    pub scan_path: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TcshLspConfig {
    pub dialect: DialectMode,
    pub include_paths: Vec<String>,
    pub max_file_size_bytes: usize,
    pub workspace_indexing: WorkspaceIndexingConfig,
    pub completion: CompletionConfig,
    pub diagnostics_enabled: bool,
    pub formatting_enabled: bool,
    pub custom_builtins: Vec<String>,
    pub custom_commands: Vec<String>,
    pub user_dictionaries: Vec<String>,
    pub log_level: String,
}

impl Default for TcshLspConfig {
    fn default() -> Self {
        Self {
            dialect: DialectMode::Auto,
            include_paths: Vec::new(),
            max_file_size_bytes: 1_048_576,
            workspace_indexing: WorkspaceIndexingConfig::default(),
            completion: CompletionConfig::default(),
            diagnostics_enabled: true,
            formatting_enabled: true,
            custom_builtins: Vec::new(),
            custom_commands: Vec::new(),
            user_dictionaries: Vec::new(),
            log_level: "info".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_conservative() {
        let config = TcshLspConfig::default();
        assert_eq!(config.dialect, DialectMode::Auto);
        assert!(!config.workspace_indexing.enabled);
        assert!(!config.completion.scan_path);
        assert!(config.formatting_enabled);
        assert_eq!(config.max_file_size_bytes, 1_048_576);
    }
}
