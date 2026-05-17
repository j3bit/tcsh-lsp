use super::CancellationRegistry;
use crate::config::TcshLspConfig;
use crate::documents::{DocumentStore, detect_language};
use crate::features::completion::completions_for_text;
use crate::features::diagnostics::diagnostics_for_text;
use crate::features::document_symbols::document_symbols_for_text;
use crate::features::edits::{
    conservative_code_actions, prepare_rename_for_position, rename_edits_for_text,
    rename_target_at_position, workspace_edit_from_changes,
};
use crate::features::navigation::{definition_for_position, hover_for_position};
use crate::features::read_only::{
    folding_ranges_for_text, highlights_for_position, reference_query_at_position,
    references_for_query, selection_range_for_position, semantic_token_legend,
    semantic_tokens_for_text, symbol_information_for_text,
};
use crate::format::{document_formatting_edits, range_formatting_edits};
use crate::semantics::AnalysisContext;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tower_lsp::jsonrpc::Result;
use tower_lsp::lsp_types::{
    CodeActionKind, CodeActionOptions, CodeActionParams, CodeActionProviderCapability,
    CodeActionResponse, CompletionOptions, CompletionParams, CompletionResponse,
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidChangeWorkspaceFoldersParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    DocumentFormattingParams, DocumentHighlight, DocumentHighlightParams,
    DocumentRangeFormattingParams, DocumentSymbolParams, DocumentSymbolResponse, FoldingRange,
    FoldingRangeParams, FoldingRangeProviderCapability, FormattingOptions, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, HoverProviderCapability, InitializeParams,
    InitializeResult, InitializedParams, Location, MessageType, OneOf, PrepareRenameResponse,
    ReferenceParams, RenameOptions, RenameParams, SaveOptions, SelectionRange,
    SelectionRangeParams, SelectionRangeProviderCapability, SemanticTokens,
    SemanticTokensFullOptions, SemanticTokensOptions, SemanticTokensParams, SemanticTokensResult,
    SemanticTokensServerCapabilities, ServerCapabilities, ServerInfo, SymbolInformation,
    TextDocumentPositionParams, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, TextEdit, WorkspaceEdit,
    WorkspaceSymbolParams,
};
use tower_lsp::{Client, LanguageServer, async_trait};
use tracing::{debug, error, info, warn};

#[derive(Debug)]
pub struct Backend {
    client: Client,
    cancellation: CancellationRegistry,
    config: TcshLspConfig,
    documents: Arc<RwLock<DocumentStore>>,
}

impl Backend {
    pub fn new(client: Client) -> Self {
        let config = TcshLspConfig::default();
        Self {
            client,
            cancellation: CancellationRegistry::default(),
            documents: Arc::new(RwLock::new(DocumentStore::new(&config))),
            config,
        }
    }

    fn analysis_context_for_uri(&self, uri: &tower_lsp::lsp_types::Url) -> AnalysisContext {
        let current_dir = uri
            .to_file_path()
            .ok()
            .and_then(|path| path.parent().map(std::path::Path::to_path_buf))
            .unwrap_or_else(|| {
                std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."))
            });
        AnalysisContext::new(current_dir, self.config.clone())
    }

    async fn publish_diagnostics_for_uri(&self, uri: &tower_lsp::lsp_types::Url) {
        let Some(doc) = self.documents.read().await.get(uri).cloned() else {
            return;
        };
        let context = self.analysis_context_for_uri(uri);
        let diagnostics = diagnostics_for_text(&doc.text, &context);
        self.client
            .publish_diagnostics(uri.clone(), diagnostics, Some(doc.version))
            .await;
    }

    fn capabilities() -> ServerCapabilities {
        ServerCapabilities {
            definition_provider: Some(OneOf::Left(true)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            references_provider: Some(OneOf::Left(true)),
            document_highlight_provider: Some(OneOf::Left(true)),
            folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
            selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
            workspace_symbol_provider: Some(OneOf::Left(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
            document_range_formatting_provider: Some(OneOf::Left(true)),
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: Default::default(),
            })),
            code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
                code_action_kinds: Some(vec![CodeActionKind::QUICKFIX, CodeActionKind::SOURCE]),
                work_done_progress_options: Default::default(),
                resolve_provider: Some(false),
            })),
            semantic_tokens_provider: Some(
                SemanticTokensServerCapabilities::SemanticTokensOptions(SemanticTokensOptions {
                    work_done_progress_options: Default::default(),
                    legend: semantic_token_legend(),
                    range: Some(false),
                    full: Some(SemanticTokensFullOptions::Bool(true)),
                }),
            ),
            completion_provider: Some(CompletionOptions {
                resolve_provider: Some(false),
                trigger_characters: Some(vec!["$".to_string(), "@".to_string(), "/".to_string()]),
                all_commit_characters: None,
                work_done_progress_options: Default::default(),
                completion_item: None,
            }),
            document_symbol_provider: Some(OneOf::Left(true)),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    will_save: Some(false),
                    will_save_wait_until: Some(false),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                        include_text: Some(false),
                    })),
                },
            )),
            ..ServerCapabilities::default()
        }
    }
}

#[async_trait]
impl LanguageServer for Backend {
    async fn initialize(&self, params: InitializeParams) -> Result<InitializeResult> {
        info!(
            client_name = ?params.client_info.as_ref().map(|c| &c.name),
            max_file_size_bytes = self.config.max_file_size_bytes,
            "initialize"
        );
        Ok(InitializeResult {
            capabilities: Self::capabilities(),
            server_info: Some(ServerInfo {
                name: "tcsh-lsp".to_string(),
                version: Some(env!("CARGO_PKG_VERSION").to_string()),
            }),
        })
    }

    async fn initialized(&self, _params: InitializedParams) {
        debug!("initialized");
        self.cancellation.mark_cancelled("__m0_smoke__");
        let _ = self.cancellation.is_cancelled("__m0_smoke__");
        self.cancellation.clear("__m0_smoke__");
        self.client
            .log_message(MessageType::INFO, "tcsh-lsp initialized")
            .await;
    }

    async fn shutdown(&self) -> Result<()> {
        info!("shutdown");
        Ok(())
    }

    async fn did_open(&self, params: DidOpenTextDocumentParams) {
        let doc = params.text_document;
        let language = detect_language(&doc.uri, &doc.language_id, &doc.text);
        let mut documents = self.documents.write().await;
        let result = documents.open(doc.uri.clone(), doc.version, doc.language_id, doc.text);
        let open_document_count = documents.len();
        drop(documents);
        match result {
            Ok(()) => {
                debug!(uri = %doc.uri, ?language, open_document_count, "document opened");
                self.publish_diagnostics_for_uri(&doc.uri).await;
            }
            Err(err) => {
                warn!(uri = %doc.uri, error = %err, "document open rejected");
                self.client
                    .log_message(
                        MessageType::WARNING,
                        format!("tcsh-lsp didOpen ignored: {err}"),
                    )
                    .await;
            }
        }
    }

    async fn did_change(&self, params: DidChangeTextDocumentParams) {
        let uri = params.text_document.uri;
        let version = params.text_document.version;
        let mut store = self.documents.write().await;
        for change in params.content_changes {
            let result = if let Some(range) = change.range {
                store.apply_range_change(&uri, version, range, &change.text)
            } else {
                store.apply_full_change(&uri, version, change.text)
            };
            if let Err(err) = result {
                error!(uri = %uri, error = %err, "document change failed");
                self.client
                    .log_message(
                        MessageType::ERROR,
                        format!("tcsh-lsp didChange failed: {err}"),
                    )
                    .await;
                return;
            }
        }
        drop(store);
        self.publish_diagnostics_for_uri(&uri).await;
        debug!(uri = %uri, version, "document changed");
    }

    async fn did_save(&self, params: DidSaveTextDocumentParams) {
        let uri = params.text_document.uri;
        let is_open = self.documents.read().await.get(&uri).is_some();
        debug!(uri = %uri, is_open, "document saved");
    }

    async fn did_close(&self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.write().await.close(&uri);
        self.client
            .publish_diagnostics(uri.clone(), Vec::new(), None)
            .await;
        debug!(uri = %uri, "document closed");
    }

    async fn goto_definition(
        &self,
        params: GotoDefinitionParams,
    ) -> Result<Option<GotoDefinitionResponse>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let store = self.documents.read().await;
        let Some(doc) = store.get(&uri) else {
            return Ok(None);
        };
        let context = self.analysis_context_for_uri(&uri);
        Ok(definition_for_position(&doc.text, &uri, position, &context)
            .map(GotoDefinitionResponse::Scalar))
    }

    async fn hover(&self, params: HoverParams) -> Result<Option<Hover>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let store = self.documents.read().await;
        let Some(doc) = store.get(&uri) else {
            return Ok(None);
        };
        let context = self.analysis_context_for_uri(&uri);
        Ok(hover_for_position(&doc.text, position, &context))
    }

    async fn completion(&self, params: CompletionParams) -> Result<Option<CompletionResponse>> {
        let uri = params.text_document_position.text_document.uri;
        let store = self.documents.read().await;
        let Some(doc) = store.get(&uri) else {
            return Ok(None);
        };
        let context = self.analysis_context_for_uri(&uri);
        Ok(Some(CompletionResponse::Array(completions_for_text(
            &doc.text, &context,
        ))))
    }

    async fn references(&self, params: ReferenceParams) -> Result<Option<Vec<Location>>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let store = self.documents.read().await;
        let Some(doc) = store.get(&uri) else {
            return Ok(None);
        };
        let context = self.analysis_context_for_uri(&uri);
        let Some(query) = reference_query_at_position(&doc.text, position, &context) else {
            return Ok(Some(Vec::new()));
        };
        let mut locations = Vec::new();
        for indexed_doc in store.documents() {
            if self.cancellation.is_cancelled("textDocument/references") {
                break;
            }
            let indexed_context = self.analysis_context_for_uri(&indexed_doc.uri);
            locations.extend(references_for_query(
                &indexed_doc.text,
                &indexed_doc.uri,
                &indexed_context,
                &query,
            ));
        }
        Ok(Some(locations))
    }

    async fn document_highlight(
        &self,
        params: DocumentHighlightParams,
    ) -> Result<Option<Vec<DocumentHighlight>>> {
        let uri = params.text_document_position_params.text_document.uri;
        let position = params.text_document_position_params.position;
        let store = self.documents.read().await;
        let Some(doc) = store.get(&uri) else {
            return Ok(None);
        };
        let context = self.analysis_context_for_uri(&uri);
        Ok(Some(highlights_for_position(&doc.text, position, &context)))
    }

    async fn folding_range(&self, params: FoldingRangeParams) -> Result<Option<Vec<FoldingRange>>> {
        let store = self.documents.read().await;
        let Some(doc) = store.get(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(folding_ranges_for_text(&doc.text)))
    }

    async fn selection_range(
        &self,
        params: SelectionRangeParams,
    ) -> Result<Option<Vec<SelectionRange>>> {
        let store = self.documents.read().await;
        let Some(doc) = store.get(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(
            params
                .positions
                .into_iter()
                .filter_map(|position| selection_range_for_position(&doc.text, position))
                .collect(),
        ))
    }

    async fn semantic_tokens_full(
        &self,
        params: SemanticTokensParams,
    ) -> Result<Option<SemanticTokensResult>> {
        let store = self.documents.read().await;
        let Some(doc) = store.get(&params.text_document.uri) else {
            return Ok(None);
        };
        let tokens: SemanticTokens = semantic_tokens_for_text(&doc.text);
        Ok(Some(SemanticTokensResult::Tokens(tokens)))
    }

    async fn symbol(
        &self,
        params: WorkspaceSymbolParams,
    ) -> Result<Option<Vec<SymbolInformation>>> {
        let query = params.query.to_ascii_lowercase();
        let store = self.documents.read().await;
        let mut symbols = Vec::new();
        for doc in store.documents() {
            if self.cancellation.is_cancelled("workspace/symbol") {
                break;
            }
            let context = self.analysis_context_for_uri(&doc.uri);
            symbols.extend(
                symbol_information_for_text(&doc.text, &doc.uri, &context)
                    .into_iter()
                    .filter(|symbol| symbol.name.to_ascii_lowercase().contains(&query)),
            );
        }
        Ok(Some(symbols))
    }

    async fn formatting(&self, params: DocumentFormattingParams) -> Result<Option<Vec<TextEdit>>> {
        if !self.config.formatting_enabled {
            return Ok(Some(Vec::new()));
        }
        let store = self.documents.read().await;
        let Some(doc) = store.get(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(document_formatting_edits(&doc.text, &params.options)))
    }

    async fn range_formatting(
        &self,
        params: DocumentRangeFormattingParams,
    ) -> Result<Option<Vec<TextEdit>>> {
        if !self.config.formatting_enabled {
            return Ok(Some(Vec::new()));
        }
        let store = self.documents.read().await;
        let Some(doc) = store.get(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(range_formatting_edits(
            &doc.text,
            &params.options,
            params.range,
        )))
    }

    async fn prepare_rename(
        &self,
        params: TextDocumentPositionParams,
    ) -> Result<Option<PrepareRenameResponse>> {
        let uri = params.text_document.uri;
        let position = params.position;
        let store = self.documents.read().await;
        let Some(doc) = store.get(&uri) else {
            return Ok(None);
        };
        let context = self.analysis_context_for_uri(&uri);
        Ok(prepare_rename_for_position(&doc.text, position, &context))
    }

    async fn rename(&self, params: RenameParams) -> Result<Option<WorkspaceEdit>> {
        let uri = params.text_document_position.text_document.uri;
        let position = params.text_document_position.position;
        let store = self.documents.read().await;
        let Some(seed_doc) = store.get(&uri) else {
            return Ok(None);
        };
        let seed_context = self.analysis_context_for_uri(&uri);
        let Some(target) = rename_target_at_position(&seed_doc.text, position, &seed_context)
        else {
            return Ok(None);
        };
        let mut changes = HashMap::new();
        for doc in store.documents() {
            let context = self.analysis_context_for_uri(&doc.uri);
            if let Some(edits) =
                rename_edits_for_text(&doc.text, &target, &params.new_name, &context)
            {
                if !edits.is_empty() {
                    changes.insert(doc.uri.clone(), edits);
                }
            }
        }
        Ok(workspace_edit_from_changes(changes))
    }

    async fn code_action(&self, params: CodeActionParams) -> Result<Option<CodeActionResponse>> {
        let store = self.documents.read().await;
        let Some(doc) = store.get(&params.text_document.uri) else {
            return Ok(None);
        };
        let options = FormattingOptions {
            tab_size: 4,
            insert_spaces: true,
            ..Default::default()
        };
        Ok(Some(conservative_code_actions(
            &params.text_document.uri,
            &doc.text,
            &params.context.diagnostics,
            &options,
        )))
    }

    async fn document_symbol(
        &self,
        params: DocumentSymbolParams,
    ) -> Result<Option<DocumentSymbolResponse>> {
        let store = self.documents.read().await;
        let Some(doc) = store.get(&params.text_document.uri) else {
            return Ok(None);
        };
        Ok(Some(DocumentSymbolResponse::Nested(
            document_symbols_for_text(&doc.text),
        )))
    }

    async fn did_change_configuration(&self, _params: DidChangeConfigurationParams) {
        debug!("configuration changed; dynamic settings merge lands after schema stabilization");
    }

    async fn did_change_workspace_folders(&self, _params: DidChangeWorkspaceFoldersParams) {
        debug!("workspace folders changed; workspace index lands in a later milestone");
    }
}
