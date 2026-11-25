use rake::compile_source;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, BufWriter, Write};

#[derive(Default)]
struct Document {
    text: String,
    version: Option<i64>,
}

fn read_message<R: BufRead>(reader: &mut R) -> io::Result<Option<Value>> {
    let mut content_length = None;
    loop {
        let mut line = String::new();
        if reader.read_line(&mut line)? == 0 {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("Content-Length") {
                content_length = Some(value.trim().parse::<usize>().map_err(|error| {
                    io::Error::new(io::ErrorKind::InvalidData, error)
                })?);
            }
        }
    }

    let length = content_length.ok_or_else(|| {
        io::Error::new(io::ErrorKind::InvalidData, "missing Content-Length header")
    })?;
    let mut body = vec![0; length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map(Some)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

fn send_message<W: Write>(writer: &mut W, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

fn diagnostic_for(source: &str, message: &str) -> Value {
    let mut parts = message.splitn(3, ':');
    let line = parts
        .next()
        .and_then(|part| part.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let column = parts
        .next()
        .and_then(|part| part.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let description = parts.next().unwrap_or(message).trim();

    // The compiler counts columns in Unicode scalar values, while LSP uses UTF-16.
    let line_text = source.split('\n').nth(line - 1).unwrap_or("");
    let chars: Vec<char> = line_text.trim_end_matches('\r').chars().collect();
    let char_index = (column - 1).min(chars.len());
    let start_character = chars[..char_index]
        .iter()
        .map(|character| character.len_utf16())
        .sum::<usize>();
    let end_character = start_character
        + chars
            .get(char_index)
            .map(|character| character.len_utf16())
            .unwrap_or(0);

    json!({
        "range": {
            "start": {"line": line - 1, "character": start_character},
            "end": {"line": line - 1, "character": end_character}
        },
        "severity": 1,
        "source": "rake",
        "message": description
    })
}

fn publish_diagnostics<W: Write>(
    writer: &mut W,
    uri: &str,
    document: &Document,
) -> io::Result<()> {
    let diagnostics = match compile_source(&document.text) {
        Ok(_) => Vec::new(),
        Err(message) => vec![diagnostic_for(&document.text, &message)],
    };
    let mut params = json!({"uri": uri, "diagnostics": diagnostics});
    if let Some(version) = document.version {
        params["version"] = json!(version);
    }
    send_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "method": "textDocument/publishDiagnostics",
            "params": params
        }),
    )
}

fn completion_items() -> Vec<Value> {
    [
        ("var", 14, "Mutable variable declaration"),
        ("const", 14, "Constant declaration"),
        ("if", 14, "Conditional statement"),
        ("else", 14, "Alternative branch"),
        ("while", 14, "Loop statement"),
        ("true", 14, "Boolean literal"),
        ("false", 14, "Boolean literal"),
        ("int", 7, "Integer type"),
        ("float", 7, "Floating-point type"),
        ("bool", 7, "Boolean type"),
    ]
    .into_iter()
    .map(|(label, kind, detail)| {
        json!({"label": label, "kind": kind, "detail": detail, "insertText": label})
    })
    .collect()
}

fn process_message<W: Write>(
    writer: &mut W,
    message: Value,
    documents: &mut HashMap<String, Document>,
) -> io::Result<bool> {
    let method = message.get("method").and_then(Value::as_str).unwrap_or("");
    let id = message.get("id").cloned();
    let params = message.get("params").cloned().unwrap_or(Value::Null);

    match method {
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
                                "completionProvider": {"resolveProvider": false}
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
            if let Some(uri) = params
                .get("textDocument")
                .and_then(|document| document.get("uri"))
                .and_then(Value::as_str)
            {
                documents.remove(uri);
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
                send_message(
                    writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {"isIncomplete": false, "items": completion_items()}
                    }),
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
