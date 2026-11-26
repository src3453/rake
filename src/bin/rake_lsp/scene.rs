// Language features for `.rks` Rake Scene files.

fn is_scene_uri(uri: &str) -> bool {
    uri.to_ascii_lowercase().ends_with(".rks")
}

fn percent_decode(text: &str) -> String {
    let bytes = text.as_bytes();
    let mut output = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            if let Ok(byte) = u8::from_str_radix(&text[index + 1..index + 3], 16) {
                output.push(byte);
                index += 3;
                continue;
            }
        }
        output.push(bytes[index]);
        index += 1;
    }
    String::from_utf8_lossy(&output).into_owned()
}

fn uri_to_path(uri: &str) -> Option<PathBuf> {
    let rest = uri.strip_prefix("file://")?;
    let decoded = percent_decode(rest);
    // `file:///C:/x` becomes `/C:/x`; drop the slash before a Windows drive letter.
    let bytes = decoded.as_bytes();
    if bytes.len() > 2 && bytes[0] == b'/' && bytes[2] == b':' {
        return Some(PathBuf::from(&decoded[1..]));
    }
    Some(PathBuf::from(decoded))
}

fn path_to_uri(path: &Path) -> String {
    let text = path.to_string_lossy().replace('\\', "/");
    let mut encoded = String::new();
    for byte in text.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'/' | b'-' | b'_' | b'.' | b'~' | b':' => {
                encoded.push(byte as char)
            }
            _ => encoded.push_str(&format!("%{byte:02X}")),
        }
    }
    if encoded.starts_with('/') {
        format!("file://{encoded}")
    } else {
        format!("file:///{encoded}")
    }
}

fn scene_diagnostics(uri: &str, text: &str) -> Vec<Value> {
    let path = uri_to_path(uri);
    match parse_scene(text, path.as_deref()) {
        Ok(_) => Vec::new(),
        Err(message) => {
            let mut diagnostic = diagnostic_for(text, &message);
            diagnostic["source"] = json!("rake-scene");
            // Widen the range to the whole token when it is a path or name.
            if let Ok(tokens) = scene_source_tokens(text) {
                let start = &diagnostic["range"]["start"];
                let (line, character) = (start["line"].as_u64().unwrap_or(0) as u32, start["character"].as_u64().unwrap_or(0) as u32);
                if let Some(token) = tokens.iter().find(|token| token.line == line && token.start_character == character) {
                    diagnostic["range"]["end"] = json!({"line": line, "character": character + token.length});
                }
            }
            vec![diagnostic]
        }
    }
}

/// Which `.rks` block the cursor is in, from the braces before it.
#[derive(Debug, PartialEq, Eq)]
enum SceneContext {
    TopLevel,
    Scene,
    Stage,
    Sprite,
    Value,
}

fn scene_token_index_before(tokens: &[SourceToken], position: Position) -> usize {
    tokens
        .iter()
        .take_while(|token| source_position(token) < position)
        .count()
}

fn scene_context(tokens: &[SourceToken], position: Position) -> (SceneContext, Option<String>) {
    let before = &tokens[..scene_token_index_before(tokens, position)];
    let mut stack: Vec<&str> = Vec::new();
    for (index, token) in before.iter().enumerate() {
        if token.kind != SourceTokenKind::Symbol {
            continue;
        }
        match token.text.as_str() {
            "{" => {
                let owner = before[..index]
                    .iter()
                    .rev()
                    .find(|candidate| matches!(candidate.text.as_str(), "scene" | "stage" | "sprite"))
                    .map(|candidate| candidate.text.as_str())
                    .unwrap_or("scene");
                stack.push(owner);
            }
            "}" => {
                stack.pop();
            }
            _ => {}
        }
    }
    let block = match stack.last() {
        None => return (SceneContext::TopLevel, None),
        Some(&"scene") if stack.len() == 1 => return (SceneContext::Scene, None),
        Some(&"stage") => SceneContext::Stage,
        _ => SceneContext::Sprite,
    };
    // After `name:` (and before the next property) the cursor is in a value.
    let mut depth = 0;
    for (index, token) in before.iter().enumerate().rev() {
        match token.text.as_str() {
            ")" | "]" if token.kind == SourceTokenKind::Symbol => depth += 1,
            "(" | "[" if token.kind == SourceTokenKind::Symbol => depth -= 1,
            "{" | "}" if token.kind == SourceTokenKind::Symbol => break,
            ":" if token.kind == SourceTokenKind::Symbol && depth <= 0 => {
                let property = index.checked_sub(1).map(|previous| before[previous].text.clone());
                let complete_value = index + 1 < before.len() && depth == 0 && before.last().is_some_and(|last| {
                    last.kind != SourceTokenKind::Symbol || matches!(last.text.as_str(), ")" | "]")
                });
                if complete_value && !is_scene_position_inside_list(before, index) {
                    return (block, None);
                }
                return (SceneContext::Value, property);
            }
            _ => {}
        }
    }
    (block, None)
}

fn is_scene_position_inside_list(before: &[SourceToken], colon: usize) -> bool {
    let mut depth = 0;
    for token in &before[colon + 1..] {
        match token.text.as_str() {
            "(" | "[" => depth += 1,
            ")" | "]" => depth -= 1,
            _ => {}
        }
    }
    depth > 0
}

fn line_prefix(source: &str, position: Position) -> String {
    let Some(line) = source.split('\n').nth(position.line as usize) else {
        return String::new();
    };
    let mut prefix = String::new();
    let mut length = 0;
    for character in line.trim_end_matches('\r').chars() {
        length += character.len_utf16() as u32;
        if length > position.character {
            break;
        }
        prefix.push(character);
    }
    prefix
}

/// Files under the project (the scene directory's parent) with one of `extensions`, relative to the scene.
fn scene_file_candidates(scene_path: &Path, extensions: &[&str]) -> Vec<String> {
    let Some(scene_directory) = scene_path.parent() else {
        return Vec::new();
    };
    let root = scene_directory.parent().unwrap_or(scene_directory);
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
                if !name.starts_with('.') && name != "node_modules" && name != "target" && name != "build" {
                    pending.push(path);
                }
            } else if path
                .extension()
                .map(|extension| extensions.contains(&extension.to_string_lossy().to_ascii_lowercase().as_str()))
                .unwrap_or(false)
            {
                found.push(relative_scene_path(scene_directory, &path));
            }
            if found.len() >= 300 {
                return found;
            }
        }
    }
    found.sort();
    found
}

fn relative_scene_path(from: &Path, to: &Path) -> String {
    let from = from.components().collect::<Vec<_>>();
    let to = to.components().collect::<Vec<_>>();
    let common = from.iter().zip(&to).take_while(|(left, right)| left == right).count();
    let mut parts = vec!["..".to_string(); from.len() - common];
    parts.extend(to[common..].iter().map(|part| part.as_os_str().to_string_lossy().into_owned()));
    parts.join("/")
}

fn scene_completion_items(uri: &str, document: Option<&Document>, params: &Value) -> Vec<Value> {
    let Some(document) = document else {
        return Vec::new();
    };
    let Some(position) = position_from_params(params) else {
        return Vec::new();
    };
    let prefix = line_prefix(&document.text, position);
    // Inside an unterminated string: complete project file paths.
    if prefix.matches('"').count() % 2 == 1 {
        let start = prefix.rfind('"').map(|index| prefix[..=index].encode_utf16().count() as u32).unwrap_or(position.character);
        let text_before_cursor = document.text_until(position);
        let property_prefix = text_before_cursor
            .rsplit_once('"')
            .map(|(before, _)| before)
            .unwrap_or(&text_before_cursor);
        let property = property_prefix
            .rsplit_once(':')
            .and_then(|(prefix, _)| prefix.trim_end().split_whitespace().last())
            .unwrap_or_default();
        if property == "runtime" {
            return ["scratch", "turbowarp"]
                .iter()
                .map(|runtime| json!({
                    "label": format!("\"{runtime}\""),
                    "kind": 12,
                    "textEdit": {"range": {"start": {"line": position.line, "character": start}, "end": {"line": position.line, "character": position.character}}, "newText": runtime}
                }))
                .collect();
        }
        let extensions: &[&str] = match property {
            "scripts" => &["rk"],
            "sounds" => &["wav", "mp3"],
            "rotation_style" => {
                return ["all around", "left-right", "don't rotate"]
                    .iter()
                    .map(|style| json!({"label": style, "kind": 12, "textEdit": {"range": {"start": {"line": position.line, "character": start}, "end": {"line": position.line, "character": position.character}}, "newText": style}}))
                    .collect();
            }
            _ => &["svg", "png", "jpg", "jpeg", "gif", "bmp"],
        };
        let Some(path) = uri_to_path(uri) else {
            return Vec::new();
        };
        return scene_file_candidates(&path, extensions)
            .into_iter()
            .map(|candidate| {
                json!({
                    "label": candidate,
                    "kind": 17,
                    "textEdit": {"range": {"start": {"line": position.line, "character": start}, "end": {"line": position.line, "character": position.character}}, "newText": candidate}
                })
            })
            .collect();
    }

    let tokens = scene_source_tokens(&document.text_until(position)).unwrap_or_default();
    let (context, property) = scene_context(&tokens, position);
    match context {
        SceneContext::TopLevel => vec![json!({
            "label": "scene",
            "kind": 14,
            "insertTextFormat": 2,
            "insertText": "scene ${1:Main} {\n\tstage {\n\t\tbackdrop: \"$2\"\n\t}\n\n\tsprite ${3:Player} {\n\t\tcostume: \"$4\"\n\t\tposition: (0, 0)\n\t\tscripts: [\"$5\"]\n\t}\n}",
            "detail": "Declare the scene in this file"
        })],
        SceneContext::Scene => vec![
            json!({"label": "runtime", "kind": 10, "detail": "runtime: \"scratch\" | \"turbowarp\"", "documentation": {"kind": "markdown", "value": "Choose the runtime profile used to validate extension blocks. Defaults to Scratch."}, "insertText": "runtime: \"${1|scratch,turbowarp|}\""}),
            json!({"label": "stage", "kind": 14, "insertTextFormat": 2, "insertText": "stage {\n\t$0\n}", "detail": "The Stage: backdrops, sounds, and Stage scripts"}),
            json!({"label": "sprite", "kind": 14, "insertTextFormat": 2, "insertText": "sprite ${1:Name} {\n\tcostume: \"$2\"\n\tposition: (0, 0)\n\tscripts: [\"$3\"]\n}", "detail": "A Sprite with costumes, initial state, and scripts"}),
        ],
        SceneContext::Stage | SceneContext::Sprite => {
            let stage = context == SceneContext::Stage;
            scene_property_descriptions()
                .iter()
                .filter(|description| if stage { description.stage } else { description.sprite })
                .map(|description| {
                    json!({
                        "label": description.name,
                        "kind": 10,
                        "detail": description.detail,
                        "documentation": {"kind": "markdown", "value": description.documentation},
                        "insertText": format!("{}: ", description.name)
                    })
                })
                .collect()
        }
        SceneContext::Value => match property.as_deref() {
            Some("visible") | Some("draggable") => ["true", "false"]
                .iter()
                .map(|value| json!({"label": value, "kind": 14}))
                .collect(),
            Some("runtime") => ["scratch", "turbowarp"]
                .iter()
                .map(|runtime| json!({"label": format!("\"{runtime}\""), "kind": 12}))
                .collect(),
            Some("rotation_style") => ["all around", "left-right", "don't rotate"]
                .iter()
                .map(|style| json!({"label": format!("\"{style}\""), "kind": 12}))
                .collect(),
            Some("position") => vec![json!({"label": "(0, 0)", "kind": 12, "insertTextFormat": 2, "insertText": "(${1:0}, ${2:0})"})],
            _ => Vec::new(),
        },
    }
}

fn scene_token_at(text: &str, position: Position) -> Option<SourceToken> {
    scene_source_tokens(text).ok()?.into_iter().find(|token| {
        token.line == position.line
            && token.start_character <= position.character
            && position.character <= token.start_character + token.length
    })
}

fn scene_hover(uri: &str, document: &Document, position: Position) -> Value {
    let Some(token) = scene_token_at(&document.text, position) else {
        return Value::Null;
    };
    let range = json!({
        "start": {"line": token.line, "character": token.start_character},
        "end": {"line": token.line, "character": token.start_character + token.length}
    });
    let markdown = match token.kind {
        SourceTokenKind::Identifier => {
            if let Some(description) = scene_property_descriptions().iter().find(|description| description.name == token.text) {
                format!("```rks\n{}\n```\n\n{}", description.detail, description.documentation)
            } else {
                match token.text.as_str() {
                    "scene" => "**scene** declares the one Scratch project described by this file.".to_string(),
                    "stage" => "**stage** configures the Scratch Stage: backdrops, sounds, and Stage scripts.".to_string(),
                    "sprite" => "**sprite** declares a Scratch Sprite with its costumes, initial state, and scripts.".to_string(),
                    "runtime" => "**runtime** selects `scratch` or `turbowarp` for extension availability checks. The default is `scratch`; built-in Pen and Music blocks are available in both.".to_string(),
                    _ => return Value::Null,
                }
            }
        }
        SourceTokenKind::String => {
            let Some(scene_path) = uri_to_path(uri) else {
                return Value::Null;
            };
            let resolved = scene_path.parent().unwrap_or(Path::new("")).join(&token.text);
            let state = if resolved.is_file() { "exists" } else { "**missing**" };
            format!("`{}` ({state})", resolved.display())
        }
        _ => return Value::Null,
    };
    json!({"contents": {"kind": "markdown", "value": markdown}, "range": range})
}

fn scene_definition(uri: &str, document: &Document, position: Position) -> Value {
    let Some(token) = scene_token_at(&document.text, position) else {
        return Value::Null;
    };
    if token.kind != SourceTokenKind::String {
        return Value::Null;
    }
    let Some(scene_path) = uri_to_path(uri) else {
        return Value::Null;
    };
    let resolved = scene_path.parent().unwrap_or(Path::new("")).join(&token.text);
    if !resolved.is_file() {
        return Value::Null;
    }
    let resolved = resolved.canonicalize().unwrap_or(resolved);
    let resolved = PathBuf::from(resolved.to_string_lossy().trim_start_matches(r"\\?\"));
    json!({
        "uri": path_to_uri(&resolved),
        "range": {"start": {"line": 0, "character": 0}, "end": {"line": 0, "character": 0}}
    })
}

fn scene_semantic_tokens(text: &str) -> Vec<u32> {
    let Ok(tokens) = scene_source_tokens(text) else {
        return Vec::new();
    };
    let types: HashMap<&str, u32> = TOKEN_TYPES
        .iter()
        .enumerate()
        .map(|(index, token_type)| (*token_type, index as u32))
        .collect();
    let mut encoded = Vec::new();
    let (mut previous_line, mut previous_start) = (0, 0);
    for (index, token) in tokens.iter().enumerate() {
        let previous = index.checked_sub(1).map(|previous| tokens[previous].text.as_str());
        let next = tokens.get(index + 1).map(|next| next.text.as_str());
        let (token_type, modifiers) = match token.kind {
            SourceTokenKind::Identifier if previous == Some("scene") => (types["namespace"], 0b11),
            SourceTokenKind::Identifier if previous == Some("sprite") => (types["type"], 0b11),
            SourceTokenKind::Identifier if SCENE_KEYWORDS.contains(&token.text.as_str()) => (types["keyword"], 0),
            SourceTokenKind::Identifier if next == Some(":") => (types["variable"], 0),
            SourceTokenKind::Number => (types["number"], 0),
            SourceTokenKind::String => (types["string"], 0),
            SourceTokenKind::Symbol if token.text == "-" => (types["operator"], 0),
            _ => continue,
        };
        let delta_line = token.line - previous_line;
        let delta_start = if delta_line == 0 { token.start_character - previous_start } else { token.start_character };
        encoded.extend_from_slice(&[delta_line, delta_start, token.length, token_type, modifiers]);
        previous_line = token.line;
        previous_start = token.start_character;
    }
    encoded
}

fn scene_document_symbols(document: &Document) -> Value {
    let Ok(scene) = parse_scene(&document.text, None) else {
        return json!([]);
    };
    let range_of = |span: &rake::SceneSpan| {
        let line = span.line.saturating_sub(1) as u32;
        let line_text = document.text.split('\n').nth(line as usize).unwrap_or("");
        let start: u32 = line_text.chars().take(span.column.saturating_sub(1)).map(|character| character.len_utf16() as u32).sum();
        json!({"start": {"line": line, "character": start}, "end": {"line": line, "character": start + span.length as u32}})
    };
    let mut children = Vec::new();
    if scene.has_stage {
        let range = range_of(&scene.stage.span);
        children.push(json!({"name": "stage", "detail": format!("{} backdrop(s)", scene.stage.backdrops.len()), "kind": 19, "range": range, "selectionRange": range}));
    }
    for sprite in &scene.sprites {
        let range = range_of(&sprite.span);
        children.push(json!({
            "name": sprite.name,
            "detail": format!("sprite at ({}, {})", sprite.x, sprite.y),
            "kind": 5,
            "range": range,
            "selectionRange": range
        }));
    }
    let range = range_of(&scene.span);
    json!([{"name": scene.name, "detail": "scene", "kind": 3, "range": range, "selectionRange": range, "children": children}])
}

impl Document {
    /// The document text before `position`, used to classify partially typed scenes.
    fn text_until(&self, position: Position) -> String {
        let mut output = String::new();
        for (index, line) in self.text.split('\n').enumerate() {
            if index < position.line as usize {
                output.push_str(line);
                output.push('\n');
            } else {
                let mut length = 0;
                for character in line.chars() {
                    length += character.len_utf16() as u32;
                    if length > position.character {
                        break;
                    }
                    output.push(character);
                }
                break;
            }
        }
        output
    }
}
