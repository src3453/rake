fn process_message<W: Write>(
    writer: &mut W,
    message: Value,
    documents: &mut HashMap<String, Document>,
) -> io::Result<bool> {
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let id = message.get("id").cloned();
    let params = message.get("params").cloned().unwrap_or(Value::Null);
    let uri = params
        .get("textDocument")
        .and_then(|document| document.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();

    match method {
        "rake/compile" => {
            if let Some(id) = id {
                let result = match params.get("source").and_then(Value::as_str) {
                    Some(source) => match compile_source(source) {
                        Ok((project, _assets, block_count)) => {
                            json!({"ok": true, "project": project, "blockCount": block_count})
                        }
                        Err(error) => json!({"ok": false, "error": error}),
                    },
                    None => json!({"ok": false, "error": "missing source text"}),
                };
                send_message(
                    writer,
                    &json!({"jsonrpc": "2.0", "id": id, "result": result}),
                )?;
            }
        }
        "initialize" => {
            if let Some(id) = id {
                send_message(
                    writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "capabilities": {
                                "textDocumentSync": 1,
                                "completionProvider": {"resolveProvider": false},
                                "semanticTokensProvider": semantic_token_capabilities(),
                                "declarationProvider": true,
                                "definitionProvider": true,
                                "implementationProvider": true,
                                "referencesProvider": true,
                                "renameProvider": {"prepareProvider": true},
                                "hoverProvider": true,
                                "documentSymbolProvider": true
                            },
                            "serverInfo": {"name": "rake-lsp", "version": env!("CARGO_PKG_VERSION")}
                        }
                    }),
                )?;
            }
        }
        "textDocument/didOpen" => {
            if let Some(document) = params.get("textDocument") {
                if let (Some(uri), Some(text)) = (
                    document.get("uri").and_then(Value::as_str),
                    document.get("text").and_then(Value::as_str),
                ) {
                    let state = Document {
                        text: text.to_string(),
                        version: document.get("version").and_then(Value::as_i64),
                    };
                    publish_diagnostics(writer, uri, &state)?;
                    documents.insert(uri.to_string(), state);
                }
            }
        }
        "textDocument/didChange" => {
            if let Some(document) = params.get("textDocument") {
                if let Some(uri) = document.get("uri").and_then(Value::as_str) {
                    if let Some(text) = params
                        .get("contentChanges")
                        .and_then(Value::as_array)
                        .and_then(|changes| changes.last())
                        .and_then(|change| change.get("text"))
                        .and_then(Value::as_str)
                    {
                        let state = Document {
                            text: text.to_string(),
                            version: document.get("version").and_then(Value::as_i64),
                        };
                        publish_diagnostics(writer, uri, &state)?;
                        documents.insert(uri.to_string(), state);
                    }
                }
            }
        }
        "textDocument/didClose" => {
            if !uri.is_empty() {
                documents.remove(&uri);
                send_message(
                    writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "textDocument/publishDiagnostics",
                        "params": {"uri": uri, "diagnostics": []}
                    }),
                )?;
            }
        }
        "textDocument/completion" => {
            if let Some(id) = id {
                let items = completion_items(documents.get(&uri), &params);
                send_message(
                    writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"isIncomplete": false, "items": items}
                    }),
                )?;
            }
        }
        "textDocument/semanticTokens/full" => {
            if let Some(id) = id {
                let data = documents
                    .get(&uri)
                    .map(|document| semantic_token_data(&document.text))
                    .unwrap_or_default();
                send_message(
                    writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"data": data}
                    }),
                )?;
            }
        }
        "textDocument/definition" | "textDocument/declaration" | "textDocument/implementation" => {
            if let Some(id) = id {
                let result = documents
                    .get(&uri)
                    .and_then(|document| {
                        position_from_params(&params)
                            .map(|position| definition_result(&uri, document, position))
                    })
                    .unwrap_or(Value::Null);
                send_message(
                    writer,
                    &json!({"jsonrpc": "2.0", "id": id, "result": result}),
                )?;
            }
        }
        "textDocument/references" => {
            if let Some(id) = id {
                let result = documents
                    .get(&uri)
                    .and_then(|document| {
                        position_from_params(&params).map(|position| {
                            let include_declaration = params
                                .get("context")
                                .and_then(|context| context.get("includeDeclaration"))
                                .and_then(Value::as_bool)
                                .unwrap_or(false);
                            references_result(&uri, document, position, include_declaration)
                        })
                    })
                    .unwrap_or_else(|| json!([]));
                send_message(
                    writer,
                    &json!({"jsonrpc": "2.0", "id": id, "result": result}),
                )?;
            }
        }
        "textDocument/hover" => {
            if let Some(id) = id {
                let result = documents
                    .get(&uri)
                    .and_then(|document| {
                        position_from_params(&params)
                            .map(|position| hover_result(document, position))
                    })
                    .unwrap_or(Value::Null);
                send_message(
                    writer,
                    &json!({"jsonrpc": "2.0", "id": id, "result": result}),
                )?;
            }
        }
        "textDocument/documentSymbol" => {
            if let Some(id) = id {
                let result = documents
                    .get(&uri)
                    .map(document_symbols)
                    .unwrap_or_else(|| json!([]));
                send_message(
                    writer,
                    &json!({"jsonrpc": "2.0", "id": id, "result": result}),
                )?;
            }
        }
        "textDocument/prepareRename" => {
            if let Some(id) = id {
                let result = documents.get(&uri).and_then(|document| {
                    let position = position_from_params(&params)?;
                    let occurrences = symbol_occurrences(document);
                    let occurrence = occurrence_at(&occurrences, position)?;
                    Some(json!({
                        "range": occurrence_range(occurrence),
                        "placeholder": occurrence.name
                    }))
                });
                send_message(
                    writer,
                    &json!({"jsonrpc": "2.0", "id": id, "result": result}),
                )?;
            }
        }
        "textDocument/rename" => {
            if let Some(id) = id {
                let result = documents
                    .get(&uri)
                    .and_then(|document| {
                        let position = position_from_params(&params)?;
                        let new_name = params.get("newName")?.as_str()?;
                        Some(rename_result(&uri, document, position, new_name))
                    })
                    .unwrap_or(Value::Null);
                send_message(
                    writer,
                    &json!({"jsonrpc": "2.0", "id": id, "result": result}),
                )?;
            }
        }
        "shutdown" => {
            if let Some(id) = id {
                send_message(writer, &json!({"jsonrpc": "2.0", "id": id, "result": null}))?;
            }
        }
        "exit" => return Ok(false),
        _ => {
            if let Some(id) = id {
                send_message(
                    writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "error": {"code": -32601, "message": format!("method not found: {method}")}
                    }),
                )?;
            }
        }
    }

    Ok(true)
}

fn main() -> io::Result<()> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = BufWriter::new(stdout.lock());
    let mut documents = HashMap::new();

    while let Some(message) = read_message(&mut reader)? {
        if !process_message(&mut writer, message, &mut documents)? {
            break;
        }
    }
    Ok(())
}
