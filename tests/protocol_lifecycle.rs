use anyhow::{Context, Result, bail};
use serde_json::{Value, json};
use std::fs;
use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::time::{Duration, Instant};

fn frame(payload: &Value) -> Vec<u8> {
    let body = serde_json::to_vec(payload).expect("json payload");
    let mut bytes = format!("Content-Length: {}\r\n\r\n", body.len()).into_bytes();
    bytes.extend(body);
    bytes
}

fn spawn_stdout_reader(stdout: impl Read + Send + 'static) -> Receiver<Result<Value, String>> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        loop {
            let mut content_length = None;
            loop {
                let mut line = String::new();
                match reader.read_line(&mut line) {
                    Ok(0) => return,
                    Ok(_) => {
                        let trimmed = line.trim_end_matches(['\r', '\n']);
                        if trimmed.is_empty() {
                            break;
                        }
                        if let Some(value) = trimmed.strip_prefix("Content-Length: ") {
                            match value.parse::<usize>() {
                                Ok(len) => content_length = Some(len),
                                Err(err) => {
                                    let _ = tx.send(Err(format!("bad Content-Length: {err}")));
                                    return;
                                }
                            }
                        }
                    }
                    Err(err) => {
                        let _ = tx.send(Err(format!("stdout read failed: {err}")));
                        return;
                    }
                }
            }

            let Some(len) = content_length else {
                let _ = tx.send(Err("missing Content-Length".to_string()));
                return;
            };
            let mut body = vec![0_u8; len];
            if let Err(err) = reader.read_exact(&mut body) {
                let _ = tx.send(Err(format!("body read failed: {err}")));
                return;
            }
            match serde_json::from_slice::<Value>(&body) {
                Ok(value) => {
                    if tx.send(Ok(value)).is_err() {
                        return;
                    }
                }
                Err(err) => {
                    let _ = tx.send(Err(format!("json decode failed: {err}")));
                    return;
                }
            }
        }
    });
    rx
}

fn recv_matching<F>(
    rx: &Receiver<Result<Value, String>>,
    timeout: Duration,
    mut pred: F,
) -> Result<Value>
where
    F: FnMut(&Value) -> bool,
{
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        let remaining = deadline.saturating_duration_since(Instant::now());
        match rx.recv_timeout(remaining.min(Duration::from_millis(200))) {
            Ok(Ok(message)) if pred(&message) => return Ok(message),
            Ok(Ok(_notification_or_other_response)) => continue,
            Ok(Err(err)) => bail!(err),
            Err(mpsc::RecvTimeoutError::Timeout) => continue,
            Err(mpsc::RecvTimeoutError::Disconnected) => bail!("server stdout disconnected"),
        }
    }
    bail!("timed out waiting for matching LSP message")
}

#[test]
fn initialize_shutdown_exit_lifecycle() -> Result<()> {
    let server = env!("CARGO_BIN_EXE_tcsh-lsp");
    let mut child = Command::new(server)
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn tcsh-lsp")?;

    let mut stdin = child.stdin.take().context("child stdin")?;
    let stdout = child.stdout.take().context("child stdout")?;
    let rx = spawn_stdout_reader(stdout);

    stdin.write_all(&frame(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": null,
            "rootUri": null,
            "capabilities": {},
            "clientInfo": {"name": "tcsh-lsp-test", "version": "0"}
        }
    })))?;
    stdin.flush()?;

    let init = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(1))
    })?;
    assert_eq!(init["jsonrpc"], "2.0");
    assert_eq!(init["result"]["serverInfo"]["name"], "tcsh-lsp");
    assert!(init["result"]["capabilities"].is_object());
    assert_eq!(
        init["result"]["capabilities"]["textDocumentSync"]["change"],
        2
    );
    assert_eq!(
        init["result"]["capabilities"]["documentSymbolProvider"],
        true
    );
    assert_eq!(init["result"]["capabilities"]["referencesProvider"], true);
    assert_eq!(
        init["result"]["capabilities"]["documentHighlightProvider"],
        true
    );
    assert_eq!(init["result"]["capabilities"]["foldingRangeProvider"], true);
    assert_eq!(
        init["result"]["capabilities"]["selectionRangeProvider"],
        true
    );
    assert_eq!(
        init["result"]["capabilities"]["workspaceSymbolProvider"],
        true
    );
    assert_eq!(
        init["result"]["capabilities"]["semanticTokensProvider"]["full"],
        true
    );
    assert_eq!(
        init["result"]["capabilities"]["documentFormattingProvider"],
        true
    );
    assert_eq!(
        init["result"]["capabilities"]["documentRangeFormattingProvider"],
        true
    );
    assert_eq!(
        init["result"]["capabilities"]["renameProvider"]["prepareProvider"],
        true
    );
    assert!(init["result"]["capabilities"]["codeActionProvider"].is_object());

    let doc_uri = "file:///tmp/example.tcsh";
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "method":"textDocument/didOpen",
        "params": {
            "textDocument": {
                "uri": doc_uri,
                "languageId": "tcsh",
                "version": 1,
                "text": "set foo = bar\necho $foo\n"
            }
        }
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "method":"textDocument/didChange",
        "params": {
            "textDocument": {"uri": doc_uri, "version": 2},
            "contentChanges": [{
                "range": {
                    "start": {"line": 0, "character": 10},
                    "end": {"line": 0, "character": 13}
                },
                "rangeLength": 3,
                "text": "baz"
            }]
        }
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 3,
        "method":"textDocument/documentSymbol",
        "params": {"textDocument": {"uri": doc_uri}}
    })))?;
    stdin.flush()?;
    let symbols = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(3))
    })?;
    assert!(symbols["result"].as_array().is_some());

    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 4,
        "method":"textDocument/definition",
        "params": {"textDocument": {"uri": doc_uri}, "position": {"line": 1, "character": 7}}
    })))?;
    stdin.flush()?;
    let definition = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(4))
    })?;
    assert_eq!(definition["result"]["range"]["start"]["line"], 0);

    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 5,
        "method":"textDocument/hover",
        "params": {"textDocument": {"uri": doc_uri}, "position": {"line": 1, "character": 7}}
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 6,
        "method":"textDocument/completion",
        "params": {"textDocument": {"uri": doc_uri}, "position": {"line": 1, "character": 9}}
    })))?;
    stdin.flush()?;
    let hover = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(5))
    })?;
    assert!(hover["result"].is_object());
    let completion = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(6))
    })?;
    assert!(completion["result"].as_array().is_some());

    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 7,
        "method":"textDocument/references",
        "params": {
            "textDocument": {"uri": doc_uri},
            "position": {"line": 1, "character": 7},
            "context": {"includeDeclaration": true}
        }
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 8,
        "method":"textDocument/documentHighlight",
        "params": {"textDocument": {"uri": doc_uri}, "position": {"line": 1, "character": 7}}
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 9,
        "method":"textDocument/foldingRange",
        "params": {"textDocument": {"uri": doc_uri}}
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 10,
        "method":"textDocument/selectionRange",
        "params": {"textDocument": {"uri": doc_uri}, "positions": [{"line": 1, "character": 7}]}
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 11,
        "method":"workspace/symbol",
        "params": {"query": "foo"}
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 12,
        "method":"textDocument/semanticTokens/full",
        "params": {"textDocument": {"uri": doc_uri}}
    })))?;
    stdin.flush()?;
    let references = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(7))
    })?;
    assert!(
        references["result"]
            .as_array()
            .is_some_and(|items| items.len() >= 2)
    );
    let highlights = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(8))
    })?;
    assert!(highlights["result"].as_array().is_some());
    let folding = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(9))
    })?;
    assert!(folding["result"].as_array().is_some());
    let selection = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(10))
    })?;
    assert!(selection["result"].as_array().is_some());
    let workspace_symbols = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(11))
    })?;
    assert!(workspace_symbols["result"].as_array().is_some());
    let semantic_tokens = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(12))
    })?;
    assert!(semantic_tokens["result"]["data"].as_array().is_some());

    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 13,
        "method":"textDocument/formatting",
        "params": {"textDocument": {"uri": doc_uri}, "options": {"tabSize": 2, "insertSpaces": true}}
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 14,
        "method":"textDocument/rangeFormatting",
        "params": {
            "textDocument": {"uri": doc_uri},
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 1, "character": 9}},
            "options": {"tabSize": 2, "insertSpaces": true}
        }
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 15,
        "method":"textDocument/prepareRename",
        "params": {"textDocument": {"uri": doc_uri}, "position": {"line": 1, "character": 7}}
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 16,
        "method":"textDocument/rename",
        "params": {
            "textDocument": {"uri": doc_uri},
            "position": {"line": 1, "character": 7},
            "newName": "renamed"
        }
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "id": 17,
        "method":"textDocument/codeAction",
        "params": {
            "textDocument": {"uri": doc_uri},
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 1, "character": 9}},
            "context": {"diagnostics": []}
        }
    })))?;
    stdin.flush()?;
    let formatting = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(13))
    })?;
    assert!(formatting["result"].as_array().is_some());
    let range_formatting = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(14))
    })?;
    assert!(range_formatting["result"].as_array().is_some());
    let prepare_rename = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(15))
    })?;
    assert!(prepare_rename["result"].is_object());
    let rename = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(16))
    })?;
    assert!(rename["result"]["changes"].is_object());
    let code_action = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(17))
    })?;
    assert!(code_action["result"].as_array().is_some());

    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "method":"textDocument/didSave",
        "params": {"textDocument": {"uri": doc_uri}}
    })))?;
    stdin.write_all(&frame(&json!({
        "jsonrpc":"2.0",
        "method":"textDocument/didClose",
        "params": {"textDocument": {"uri": doc_uri}}
    })))?;

    stdin.write_all(&frame(
        &json!({"jsonrpc":"2.0","method":"initialized","params":{}}),
    ))?;
    stdin.write_all(&frame(
        &json!({"jsonrpc":"2.0","id":2,"method":"shutdown","params":null}),
    ))?;
    stdin.flush()?;

    let shutdown = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(2))
    })?;
    assert_eq!(shutdown["result"], Value::Null);

    stdin.write_all(&frame(&json!({"jsonrpc":"2.0","method":"exit"})))?;
    stdin.flush()?;
    drop(stdin);

    let status = child
        .wait_timeout(Duration::from_secs(5))?
        .context("server did not exit")?;
    assert!(status.success(), "server exit status: {status}");
    Ok(())
}

#[test]
fn dogfood_examples_publish_empty_diagnostics_and_answer_read_only_requests() -> Result<()> {
    let server = env!("CARGO_BIN_EXE_tcsh-lsp");
    let mut child = Command::new(server)
        .arg("--stdio")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .context("spawn tcsh-lsp")?;

    let mut stdin = child.stdin.take().context("child stdin")?;
    let stdout = child.stdout.take().context("child stdout")?;
    let rx = spawn_stdout_reader(stdout);

    stdin.write_all(&frame(&json!({
        "jsonrpc": "2.0",
        "id": 1,
        "method": "initialize",
        "params": {
            "processId": null,
            "rootUri": null,
            "capabilities": {},
            "clientInfo": {"name": "tcsh-lsp-dogfood-test", "version": "0"}
        }
    })))?;
    stdin.flush()?;
    let _init = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(1))
    })?;

    stdin.write_all(&frame(&json!({
        "jsonrpc": "2.0",
        "method": "initialized",
        "params": {}
    })))?;

    let fixtures = [
        "fixtures/corpus/parser/valid/tree_sitter_sample.tcsh",
        "fixtures/corpus/parser/valid/tree_sitter_showcase.tcsh",
    ];

    let mut next_id = 10;
    for path in fixtures {
        let text = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
        let uri = format!("file://{}", std::env::current_dir()?.join(path).display());
        stdin.write_all(&frame(&json!({
            "jsonrpc":"2.0",
            "method":"textDocument/didOpen",
            "params": {
                "textDocument": {
                    "uri": uri,
                    "languageId": "tcsh",
                    "version": 1,
                    "text": text
                }
            }
        })))?;
        stdin.flush()?;

        let diagnostics = recv_matching(&rx, Duration::from_secs(10), |message| {
            message.get("method") == Some(&json!("textDocument/publishDiagnostics"))
                && message["params"]["uri"] == json!(uri)
        })?;
        assert_eq!(
            diagnostics["params"]["diagnostics"]
                .as_array()
                .map(Vec::len),
            Some(0),
            "{path} should publish empty diagnostics"
        );

        next_id += 1;
        let document_symbol_id = next_id;
        next_id += 1;
        let folding_id = next_id;
        next_id += 1;
        let semantic_tokens_id = next_id;

        stdin.write_all(&frame(&json!({
            "jsonrpc":"2.0",
            "id": document_symbol_id,
            "method":"textDocument/documentSymbol",
            "params": {"textDocument": {"uri": uri}}
        })))?;
        stdin.write_all(&frame(&json!({
            "jsonrpc":"2.0",
            "id": folding_id,
            "method":"textDocument/foldingRange",
            "params": {"textDocument": {"uri": uri}}
        })))?;
        stdin.write_all(&frame(&json!({
            "jsonrpc":"2.0",
            "id": semantic_tokens_id,
            "method":"textDocument/semanticTokens/full",
            "params": {"textDocument": {"uri": uri}}
        })))?;
        stdin.flush()?;

        let symbols = recv_matching(&rx, Duration::from_secs(10), |message| {
            message.get("id") == Some(&json!(document_symbol_id))
        })?;
        assert!(symbols["result"].as_array().is_some());
        let folding = recv_matching(&rx, Duration::from_secs(10), |message| {
            message.get("id") == Some(&json!(folding_id))
        })?;
        assert!(folding["result"].as_array().is_some());
        let semantic_tokens = recv_matching(&rx, Duration::from_secs(10), |message| {
            message.get("id") == Some(&json!(semantic_tokens_id))
        })?;
        assert!(semantic_tokens["result"]["data"].as_array().is_some());
    }

    stdin.write_all(&frame(
        &json!({"jsonrpc":"2.0","id":99,"method":"shutdown"}),
    ))?;
    stdin.flush()?;
    let shutdown = recv_matching(&rx, Duration::from_secs(10), |message| {
        message.get("id") == Some(&json!(99))
    })?;
    assert_eq!(shutdown["result"], Value::Null);
    stdin.write_all(&frame(&json!({"jsonrpc":"2.0","method":"exit"})))?;
    stdin.flush()?;
    drop(stdin);

    let status = child
        .wait_timeout(Duration::from_secs(5))?
        .context("server did not exit")?;
    assert!(status.success(), "server exit status: {status}");
    Ok(())
}

trait WaitTimeout {
    fn wait_timeout(&mut self, timeout: Duration) -> Result<Option<std::process::ExitStatus>>;
}

impl WaitTimeout for std::process::Child {
    fn wait_timeout(&mut self, timeout: Duration) -> Result<Option<std::process::ExitStatus>> {
        let start = Instant::now();
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(Some(status));
            }
            if start.elapsed() >= timeout {
                let _ = self.kill();
                return Ok(None);
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}
