// Library modules in the editor: `from "lib/path" use ...` imports resolve against the
// project root (the nearest directory with rake-ide.json), like the compiler.

/// Symbols of a `.rk` document, with its `from` libraries loaded from disk.
fn analyze_document(document: &Document) -> Vec<SymbolOccurrence> {
    match document.path.as_deref().filter(|path| path.is_absolute()) {
        Some(path) => {
            let root = rake::find_project_root(path);
            let context = rake::ModuleContext {
                entry_path: path,
                root: &root,
                sources: &rake::FileSystemSources,
            };
            rake::analyze_symbols_in_project(&document.text, Some(&context)).unwrap_or_default()
        }
        None => analyze_symbols(&document.text).unwrap_or_default(),
    }
}

/// The `from "..."` path string under `position`, if any.
fn library_path_token_at(document: &Document, position: Position) -> Option<SourceToken> {
    let tokens = source_tokens(&document.text).ok()?;
    let index = tokens.iter().position(|token| {
        token.kind == SourceTokenKind::String
            && token.line == position.line
            && position.character >= token.start_character
            && position.character < token.start_character + token.length
    })?;
    (index > 0 && tokens[index - 1].text == "from").then(|| tokens[index].clone())
}

/// The library file a `from` path string points to.
fn library_file_at(document: &Document, position: Position) -> Option<(SourceToken, PathBuf)> {
    let token = library_path_token_at(document, position)?;
    let path = document.path.as_deref()?;
    let root = rake::find_project_root(path);
    let file = rake::library_file_path(&root, &token.text).ok()?;
    Some((token, file))
}

fn library_definition(document: &Document, position: Position) -> Option<Value> {
    let (_, file) = library_file_at(document, position)?;
    file.is_file().then(|| {
        json!({
            "uri": path_to_uri(&file),
            "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}
        })
    })
}

fn library_hover(document: &Document, position: Position) -> Option<Value> {
    let (token, file) = library_file_at(document, position)?;
    let status = if file.is_file() { "" } else { " (not found)" };
    Some(json!({
        "contents": {
            "kind": "markdown",
            "value": format!(
                "Library module `{}`{status}\n\nPaths are relative to the project root (the folder with `rake-ide.json`).",
                file.display()
            )
        },
        "range": {
            "start": {"line": token.line, "character": token.start_character},
            "end": {"line": token.line, "character": token.start_character + token.length}
        }
    }))
}

/// `.rk` files under `root`, as `from` paths (root-relative, without the extension).
fn library_path_candidates(root: &Path, exclude: Option<&Path>) -> Vec<String> {
    let mut found = Vec::new();
    let mut pending = vec![root.to_path_buf()];
    while let Some(directory) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let name = entry.file_name().to_string_lossy().into_owned();
            if path.is_dir() {
                if !name.starts_with('.') && !matches!(name.as_str(), "node_modules" | "target" | "build") {
                    pending.push(path);
                }
            } else if path.extension().is_some_and(|extension| extension == "rk")
                && exclude.is_none_or(|exclude| !same_path(exclude, &path))
            {
                if let Ok(relative) = path.strip_prefix(root) {
                    let relative = relative
                        .components()
                        .map(|component| component.as_os_str().to_string_lossy().into_owned())
                        .collect::<Vec<_>>()
                        .join("/");
                    found.push(relative.trim_end_matches(".rk").to_string());
                }
            }
            if found.len() >= 300 {
                break;
            }
        }
    }
    found.sort();
    found
}

/// Completions for the path inside `from "...`.
fn library_path_completion_items(document: &Document, position: Position) -> Option<Vec<Value>> {
    let before = line_prefix(&document.text, position);
    let quote = before.rfind('"')?;
    if !before[..quote].trim_end().ends_with("from") || before[..quote].matches('"').count() % 2 != 0 {
        return None;
    }
    let typed = &before[quote + 1..];
    let start_character = position.character - typed.encode_utf16().count() as u32;
    let Some(path) = document.path.as_deref() else {
        return Some(Vec::new());
    };
    let root = rake::find_project_root(path);
    Some(
        library_path_candidates(&root, Some(path))
            .into_iter()
            .filter(|candidate| candidate.starts_with(typed))
            .map(|candidate| {
                json!({
                    "label": candidate,
                    "kind": 17,
                    "detail": "Rake library module",
                    "textEdit": {
                        "range": {
                            "start": {"line": position.line, "character": start_character},
                            "end": {"line": position.line, "character": position.character}
                        },
                        "newText": candidate
                    }
                })
            })
            .collect(),
    )
}
