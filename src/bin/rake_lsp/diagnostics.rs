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

fn publish_diagnostics<W: Write>(writer: &mut W, uri: &str, document: &Document) -> io::Result<()> {
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
