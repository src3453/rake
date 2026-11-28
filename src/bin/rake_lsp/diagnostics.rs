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

/// Library modules for diagnostics: open editor buffers win over the files on disk,
/// so errors follow unsaved library edits.
struct OpenBufferSources<'a> {
    documents: &'a HashMap<String, Document>,
}

impl rake::SourceProvider for OpenBufferSources<'_> {
    fn read_module(&self, root: &Path, relative: &str) -> Result<String, String> {
        let wanted = root.join(relative);
        for (uri, document) in self.documents {
            if uri_to_path(uri).is_some_and(|path| same_path(&path, &wanted)) {
                return Ok(document.text.clone());
            }
        }
        rake::FileSystemSources.read_module(root, relative)
    }
}

fn same_path(left: &Path, right: &Path) -> bool {
    let normalize = |path: &Path| {
        path.components()
            .filter(|component| !matches!(component, std::path::Component::CurDir))
            .map(|component| component.as_os_str().to_string_lossy().replace('\\', "/"))
            .collect::<Vec<_>>()
    };
    let (left, right) = (normalize(left), normalize(right));
    // Editors may report a Windows drive letter in either case.
    left.len() == right.len()
        && left.iter().zip(&right).enumerate().all(|(index, (left, right))| {
            left == right || (index == 0 && left.eq_ignore_ascii_case(right))
        })
}

/// Compile a `.rk` buffer with its `from` libraries resolved from the project root.
fn source_diagnostics(uri: &str, text: &str, documents: &HashMap<String, Document>) -> Vec<Value> {
    let result = match uri_to_path(uri).filter(|path| path.is_absolute()) {
        Some(path) => {
            let root = rake::find_project_root(&path);
            let sources = OpenBufferSources { documents };
            let context = rake::ModuleContext {
                entry_path: &path,
                root: &root,
                sources: &sources,
            };
            rake::compile_source_in_project(text, &context, rake::Runtime::default()).map(|_| ())
        }
        None => compile_source(text).map(|_| ()).map_err(|message| rake::SourceError {
            file: None,
            message,
        }),
    };
    match result {
        Ok(()) => Vec::new(),
        Err(rake::SourceError { file: None, message }) => vec![diagnostic_for(text, &message)],
        Err(rake::SourceError { file: Some(file), message }) => {
            // The error is inside a library: show it on this file's first `from` import.
            let line = text
                .lines()
                .position(|line| line.trim_start().starts_with("from \""))
                .unwrap_or(0);
            let (location, description) = message
                .split_once(": ")
                .map(|(location, description)| (location.to_string(), description.to_string()))
                .unwrap_or_default();
            vec![diagnostic_for(
                text,
                &format!(
                    "{}:1: error in library {}:{location}: {description}",
                    line + 1,
                    file.display()
                ),
            )]
        }
    }
}

#[cfg(test)]
fn publish_diagnostics<W: Write>(writer: &mut W, uri: &str, document: &Document) -> io::Result<()> {
    publish_diagnostics_with(writer, uri, document, &HashMap::new())
}

fn publish_diagnostics_with<W: Write>(
    writer: &mut W,
    uri: &str,
    document: &Document,
    documents: &HashMap<String, Document>,
) -> io::Result<()> {
    let diagnostics = if is_scene_uri(uri) {
        scene_diagnostics(uri, &document.text)
    } else {
        source_diagnostics(uri, &document.text, documents)
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

/// Re-check `uri` and every other open `.rk` file, since any of them may import it.
fn publish_project_diagnostics<W: Write>(
    writer: &mut W,
    uri: &str,
    documents: &HashMap<String, Document>,
) -> io::Result<()> {
    if let Some(document) = documents.get(uri) {
        publish_diagnostics_with(writer, uri, document, documents)?;
    }
    if is_scene_uri(uri) {
        return Ok(());
    }
    let mut others = documents
        .keys()
        .filter(|other| other.as_str() != uri && !is_scene_uri(other))
        .cloned()
        .collect::<Vec<_>>();
    others.sort();
    for other in others {
        publish_diagnostics_with(writer, &other, &documents[&other], documents)?;
    }
    Ok(())
}
