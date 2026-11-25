use rake::{
    SourceToken, SourceTokenKind, SymbolOccurrence, analyze_symbols, compile_source, source_tokens,
};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::io::{self, BufRead, BufReader, BufWriter, Write};

const TOKEN_TYPES: &[&str] = &[
    "variable", "type", "keyword", "number", "operator", "function",
];
const TOKEN_MODIFIERS: &[&str] = &["declaration", "definition", "readonly", "modification"];

#[derive(Default)]
struct Document {
    text: String,
    version: Option<i64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Position {
    line: u32,
    character: u32,
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
                content_length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))?,
                );
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

fn position_from_params(params: &Value) -> Option<Position> {
    let position = params.get("position")?;
    Some(Position {
        line: position.get("line")?.as_u64()? as u32,
        character: position.get("character")?.as_u64()? as u32,
    })
}

fn source_position(token: &SourceToken) -> Position {
    Position {
        line: token.line,
        character: token.start_character,
    }
}

fn end_position(token: &SourceToken) -> Position {
    Position {
        line: token.line,
        character: token.start_character + token.length,
    }
}

fn occurrence_range(occurrence: &SymbolOccurrence) -> Value {
    json!({
        "start": {"line": occurrence.line, "character": occurrence.character},
        "end": {
            "line": occurrence.line,
            "character": occurrence.character + occurrence.length
        }
    })
}

fn location(uri: &str, occurrence: &SymbolOccurrence) -> Value {
    json!({"uri": uri, "range": occurrence_range(occurrence)})
}

fn occurrence_at<'a>(
    occurrences: &'a [SymbolOccurrence],
    position: Position,
) -> Option<&'a SymbolOccurrence> {
    occurrences.iter().find(|occurrence| {
        occurrence.line == position.line
            && position.character >= occurrence.character
            && position.character <= occurrence.character + occurrence.length
    })
}

fn scope_paths(tokens: &[SourceToken]) -> Vec<Vec<usize>> {
    let mut stack = vec![0];
    let mut paths = Vec::with_capacity(tokens.len());
    for (index, token) in tokens.iter().enumerate() {
        if token.text == "}" && stack.len() > 1 {
            stack.pop();
        }
        paths.push(stack.clone());
        if token.text == "{" {
            stack.push(index + 1);
        }
    }
    paths
}

fn scope_at_position(tokens: &[SourceToken], position: Position) -> Vec<usize> {
    let mut stack = vec![0];
    for (index, token) in tokens.iter().enumerate() {
        if source_position(token) >= position {
            break;
        }
        if token.text == "{" {
            stack.push(index + 1);
        } else if token.text == "}" && stack.len() > 1 {
            stack.pop();
        }
    }
    stack
}

fn function_parameter_body_open(tokens: &[SourceToken], parameter_index: usize) -> Option<usize> {
    for (index, token) in tokens.iter().enumerate() {
        if token.text != "fn"
            || tokens.get(index + 1).map(|token| token.kind) != Some(SourceTokenKind::Identifier)
            || tokens.get(index + 2).map(|token| token.text.as_str()) != Some("(")
        {
            continue;
        }
        let parameters_open = index + 2;
        let Some(parameters_close) =
            (parameters_open + 1..tokens.len()).find(|candidate| tokens[*candidate].text == ")")
        else {
            continue;
        };
        if parameter_index <= parameters_open || parameter_index >= parameters_close {
            continue;
        }
        return (parameters_close + 1..tokens.len())
            .find(|candidate| tokens[*candidate].text == "{");
    }
    None
}

fn visible_declarations<'a>(
    tokens: &[SourceToken],
    occurrences: &'a [SymbolOccurrence],
    position: Position,
) -> Vec<&'a SymbolOccurrence> {
    let paths = scope_paths(tokens);
    let current_scope = scope_at_position(tokens, position);
    let mut selected: HashMap<&str, (&SymbolOccurrence, usize)> = HashMap::new();

    for occurrence in occurrences
        .iter()
        .filter(|entry| entry.declaration && !entry.function)
    {
        let token_index = match tokens.iter().position(|token| {
            token.line == occurrence.line
                && token.start_character == occurrence.character
                && token.text == occurrence.name
        }) {
            Some(index) => index,
            None => continue,
        };
        if end_position(&tokens[token_index]) > position {
            continue;
        }
        let Some(mut declaration_scope) = paths.get(token_index).cloned() else {
            continue;
        };
        if let Some(body_open) = function_parameter_body_open(tokens, token_index) {
            if let Some(parent_scope) = paths.get(body_open) {
                declaration_scope = parent_scope.clone();
                declaration_scope.push(body_open + 1);
            }
        }
        if declaration_scope.len() > current_scope.len()
            || current_scope[..declaration_scope.len()] != declaration_scope[..]
        {
            continue;
        }
        let depth = declaration_scope.len();
        let key = occurrence.name.as_str();
        let replace = selected
            .get(key)
            .map(|(_, existing_depth)| depth >= *existing_depth)
            .unwrap_or(true);
        if replace {
            selected.insert(key, (occurrence, depth));
        }
    }
    selected.into_values().map(|(entry, _)| entry).collect()
}

fn prefix_at(source: &str, position: Position) -> String {
    let Some(line) = source.split('\n').nth(position.line as usize) else {
        return String::new();
    };
    let mut before_cursor = String::new();
    let mut utf16_length = 0;
    for character in line.trim_end_matches('\r').chars() {
        let next_length = utf16_length + character.len_utf16() as u32;
        if next_length > position.character {
            break;
        }
        before_cursor.push(character);
        utf16_length = next_length;
    }
    before_cursor
        .chars()
        .rev()
        .take_while(|character| character.is_ascii_alphanumeric() || *character == '_')
        .collect::<String>()
        .chars()
        .rev()
        .collect()
}

fn is_type_context(tokens: &[SourceToken], position: Position, prefix: &str) -> bool {
    let prefix_start = Position {
        line: position.line,
        character: position
            .character
            .saturating_sub(prefix.encode_utf16().count() as u32),
    };
    tokens
        .iter()
        .filter(|token| end_position(token) <= prefix_start)
        .next_back()
        .map(|token| token.text == ":")
        .unwrap_or(false)
}

fn member_receiver_at(tokens: &[SourceToken], position: Position, prefix: &str) -> Option<String> {
    let prefix_start = Position {
        line: position.line,
        character: position
            .character
            .saturating_sub(prefix.encode_utf16().count() as u32),
    };
    let preceding: Vec<&SourceToken> = tokens
        .iter()
        .filter(|token| end_position(token) <= prefix_start)
        .collect();
    if preceding.last()?.text != "." {
        return None;
    }
    preceding
        .get(preceding.len().checked_sub(2)?)
        .filter(|token| token.kind == SourceTokenKind::Identifier)
        .map(|token| token.text.clone())
}

fn instance_class_name(tokens: &[SourceToken], receiver: &str) -> Option<String> {
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(token.text.as_str(), "var" | "const")
            || tokens.get(index + 1).map(|token| token.text.as_str()) != Some(receiver)
            || tokens.get(index + 2).map(|token| token.text.as_str()) != Some("=")
        {
            continue;
        }
        let class_name = tokens.get(index + 3)?;
        if class_name.kind == SourceTokenKind::Identifier
            && tokens.get(index + 4).map(|token| token.text.as_str()) == Some("(")
            && tokens.get(index + 5).map(|token| token.text.as_str()) == Some(")")
        {
            return Some(class_name.text.clone());
        }
    }
    None
}

fn class_fields(tokens: &[SourceToken], class_name: &str) -> Vec<(String, String, bool)> {
    let Some(class_index) = tokens.iter().enumerate().find_map(|(index, token)| {
        (token.text == "class"
            && tokens.get(index + 1).map(|name| name.text.as_str()) == Some(class_name))
        .then_some(index)
    }) else {
        return Vec::new();
    };
    let Some(open_brace) = (class_index + 2..tokens.len()).find(|index| tokens[*index].text == "{")
    else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    let mut index = open_brace + 1;
    while index + 3 < tokens.len() && tokens[index].text != "}" {
        if matches!(tokens[index].text.as_str(), "var" | "const")
            && tokens[index + 1].kind == SourceTokenKind::Identifier
            && tokens[index + 2].text == ":"
            && tokens[index + 3].kind == SourceTokenKind::Identifier
        {
            fields.push((
                tokens[index + 1].text.clone(),
                tokens[index + 3].text.clone(),
                tokens[index].text == "var",
            ));
            index += 4;
        } else {
            index += 1;
        }
    }
    fields
}

#[derive(Clone, Debug)]
struct FunctionCompletion {
    name: String,
    parameters: Vec<(String, String)>,
    return_type: Option<String>,
}

fn function_completions(tokens: &[SourceToken]) -> Vec<FunctionCompletion> {
    let mut functions = Vec::new();
    for (index, token) in tokens.iter().enumerate() {
        if token.text != "fn"
            || tokens.get(index + 1).map(|token| token.kind) != Some(SourceTokenKind::Identifier)
            || tokens.get(index + 2).map(|token| token.text.as_str()) != Some("(")
        {
            continue;
        }
        let name = tokens[index + 1].text.clone();
        let mut cursor = index + 3;
        let mut parameters = Vec::new();
        let mut valid = true;
        while cursor < tokens.len() && tokens[cursor].text != ")" {
            if tokens[cursor].text == "," {
                cursor += 1;
                continue;
            }
            if tokens.get(cursor).map(|token| token.kind) != Some(SourceTokenKind::Identifier)
                || tokens.get(cursor + 1).map(|token| token.text.as_str()) != Some(":")
                || tokens.get(cursor + 2).map(|token| token.kind)
                    != Some(SourceTokenKind::Identifier)
            {
                valid = false;
                break;
            }
            parameters.push((tokens[cursor].text.clone(), tokens[cursor + 2].text.clone()));
            cursor += 3;
        }
        if valid && tokens.get(cursor).map(|token| token.text.as_str()) == Some(")") {
            let return_type =
                if tokens.get(cursor + 1).map(|token| token.text.as_str()) == Some("->") {
                    tokens.get(cursor + 2).map(|token| token.text.clone())
                } else {
                    None
                };
            functions.push(FunctionCompletion {
                name,
                parameters,
                return_type,
            });
        }
    }
    functions
}

fn function_name_context(tokens: &[SourceToken], position: Position, prefix: &str) -> bool {
    let prefix_start = Position {
        line: position.line,
        character: position
            .character
            .saturating_sub(prefix.encode_utf16().count() as u32),
    };
    tokens
        .iter()
        .filter(|token| end_position(token) <= prefix_start)
        .next_back()
        .map(|token| token.text == "fn")
        .unwrap_or(false)
}

fn completion_items(document: Option<&Document>, params: &Value) -> Vec<Value> {
    let position = position_from_params(params).unwrap_or(Position {
        line: 0,
        character: 0,
    });
    let source = document.map(|doc| doc.text.as_str()).unwrap_or("");
    let tokens = source_tokens(source).unwrap_or_default();
    let occurrences = analyze_symbols(source).unwrap_or_default();
    let prefix = prefix_at(source, position);
    let type_context = is_type_context(&tokens, position, &prefix);
    let member_receiver = member_receiver_at(&tokens, position, &prefix);

    let mut items = Vec::new();
    let candidates: &[(&str, u32, &str)] = if type_context {
        &[
            ("int", 7, "Integer type"),
            ("float", 7, "Floating-point type"),
            ("bool", 7, "Boolean type"),
        ]
    } else if member_receiver.is_some() {
        &[]
    } else {
        &[
            ("var", 14, "Mutable variable declaration"),
            ("const", 14, "Constant declaration"),
            ("class", 14, "Class declaration"),
            ("if", 14, "Conditional statement"),
            ("else", 14, "Alternative branch"),
            ("while", 14, "Loop statement"),
            ("fn", 14, "Function declaration"),
            ("return", 14, "Return a value from a function"),
            ("true", 14, "Boolean literal"),
            ("false", 14, "Boolean literal"),
        ]
    };
    for (label, kind, detail) in candidates.iter().copied() {
        if label.starts_with(&prefix) {
            items.push(json!({
                "label": label,
                "kind": kind,
                "detail": detail,
                "insertText": label,
                "sortText": format!("0-{label}")
            }));
        }
    }

    if let Some(receiver) = &member_receiver {
        if let Some(class_name) = instance_class_name(&tokens, receiver) {
            for (name, ty, mutable) in class_fields(&tokens, &class_name) {
                if name.starts_with(&prefix) {
                    items.push(json!({
                        "label": name,
                        "kind": 5,
                        "detail": format!("{} field: {ty}", if mutable { "var" } else { "const" }),
                        "insertText": name,
                        "sortText": format!("0-{name}")
                    }));
                }
            }
        }
        return items;
    }

    if !type_context {
        let declaration_context = function_name_context(&tokens, position, &prefix);
        for function in function_completions(&tokens) {
            if !function.name.starts_with(&prefix) {
                continue;
            }
            let signature = function
                .parameters
                .iter()
                .map(|(name, ty)| format!("{name}: {ty}"))
                .collect::<Vec<_>>()
                .join(", ");
            let return_annotation = function
                .return_type
                .as_ref()
                .map(|ty| format!(" -> {ty}"))
                .unwrap_or_default();
            let detail = format!("fn {}({signature}){return_annotation}", function.name);
            let insert_text = if declaration_context {
                function.name.clone()
            } else {
                let arguments = function
                    .parameters
                    .iter()
                    .enumerate()
                    .map(|(index, (name, _))| format!("${{{}:{name}}}", index + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!("{}({arguments})", function.name)
            };
            let mut item = json!({
                "label": function.name,
                "kind": 3,
                "detail": detail,
                "insertText": insert_text,
                "sortText": format!("1-{}", function.name)
            });
            if !declaration_context {
                item["insertTextFormat"] = json!(2);
            }
            items.push(item);
        }
    }

    if !type_context {
        for occurrence in visible_declarations(&tokens, &occurrences, position) {
            if !occurrence.name.starts_with(&prefix) {
                continue;
            }
            items.push(json!({
                "label": occurrence.name,
                "kind": if occurrence.class_symbol {
                    7
                } else if occurrence.mutable || occurrence.parameter {
                    6
                } else {
                    21
                },
                "detail": if occurrence.class_symbol {
                    "class declaration".to_string()
                } else {
                    format!(
                        "{} {}",
                        if occurrence.parameter {
                            "parameter"
                        } else if occurrence.mutable {
                            "variable"
                        } else {
                            "constant"
                        },
                        occurrence.ty
                    )
                },
                "insertText": occurrence.name,
                "sortText": format!("2-{}", occurrence.name)
            }));
        }
    }
    items
}

fn semantic_token_data(source: &str) -> Vec<u32> {
    let Ok(tokens) = source_tokens(source) else {
        return Vec::new();
    };
    let occurrences = analyze_symbols(source).unwrap_or_default();
    let occurrence_by_position: HashMap<(u32, u32), &SymbolOccurrence> = occurrences
        .iter()
        .map(|entry| ((entry.line, entry.character), entry))
        .collect();
    let types: HashMap<&str, u32> = TOKEN_TYPES
        .iter()
        .enumerate()
        .map(|(index, token_type)| (*token_type, index as u32))
        .collect();
    let mut encoded = Vec::new();
    let mut previous_line = 0;
    let mut previous_start = 0;

    for token in tokens {
        let token_type = match token.kind {
            SourceTokenKind::Identifier
                if matches!(token.text.as_str(), "int" | "float" | "bool") =>
            {
                Some(types["type"])
            }
            SourceTokenKind::Identifier
                if matches!(
                    token.text.as_str(),
                    "var"
                        | "const"
                        | "class"
                        | "fn"
                        | "if"
                        | "else"
                        | "while"
                        | "return"
                        | "true"
                        | "false"
                ) =>
            {
                Some(types["keyword"])
            }
            SourceTokenKind::Identifier
                if occurrence_by_position
                    .get(&(token.line, token.start_character))
                    .map(|occurrence| occurrence.class_symbol)
                    .unwrap_or(false) =>
            {
                Some(types["type"])
            }
            SourceTokenKind::Identifier
                if occurrence_by_position
                    .get(&(token.line, token.start_character))
                    .map(|occurrence| occurrence.function)
                    .unwrap_or(false) =>
            {
                Some(types["function"])
            }
            SourceTokenKind::Identifier => Some(types["variable"]),
            SourceTokenKind::Number => Some(types["number"]),
            SourceTokenKind::Symbol
                if matches!(
                    token.text.as_str(),
                    "=" | "->"
                        | "+="
                        | "-="
                        | "+"
                        | "-"
                        | "*"
                        | "/"
                        | "<"
                        | "<="
                        | ">"
                        | ">="
                        | "=="
                        | "!="
                        | "&&"
                        | "||"
                        | "!"
                ) =>
            {
                Some(types["operator"])
            }
            SourceTokenKind::Symbol => None,
        };
        let Some(token_type) = token_type else {
            continue;
        };

        let modifiers = occurrence_by_position
            .get(&(token.line, token.start_character))
            .map(|occurrence| {
                let mut bits = 0;
                if occurrence.declaration {
                    bits |= 1 << 0;
                    bits |= 1 << 1;
                }
                if !occurrence.mutable {
                    bits |= 1 << 2;
                }
                if occurrence.modification {
                    bits |= 1 << 3;
                }
                bits
            })
            .unwrap_or(0);
        let delta_line = token.line - previous_line;
        let delta_start = if delta_line == 0 {
            token.start_character - previous_start
        } else {
            token.start_character
        };
        encoded.extend_from_slice(&[delta_line, delta_start, token.length, token_type, modifiers]);
        previous_line = token.line;
        previous_start = token.start_character;
    }
    encoded
}

fn semantic_token_capabilities() -> Value {
    json!({
        "legend": {
            "tokenTypes": TOKEN_TYPES,
            "tokenModifiers": TOKEN_MODIFIERS
        },
        "full": true
    })
}

fn symbol_occurrences(document: &Document) -> Vec<SymbolOccurrence> {
    analyze_symbols(&document.text).unwrap_or_default()
}

fn definition_result(uri: &str, document: &Document, position: Position) -> Value {
    let occurrences = symbol_occurrences(document);
    let Some(selected) = occurrence_at(&occurrences, position) else {
        return Value::Null;
    };
    let Some(declaration) = occurrences
        .iter()
        .find(|entry| entry.symbol_id == selected.symbol_id && entry.declaration)
    else {
        return Value::Null;
    };
    location(uri, declaration)
}

fn references_result(
    uri: &str,
    document: &Document,
    position: Position,
    include_declaration: bool,
) -> Value {
    let occurrences = symbol_occurrences(document);
    let Some(selected) = occurrence_at(&occurrences, position) else {
        return json!([]);
    };
    let references: Vec<Value> = occurrences
        .iter()
        .filter(|entry| {
            entry.symbol_id == selected.symbol_id && (include_declaration || !entry.declaration)
        })
        .map(|entry| location(uri, entry))
        .collect();
    json!(references)
}

fn hover_result(document: &Document, position: Position) -> Value {
    let occurrences = symbol_occurrences(document);
    let Some(occurrence) = occurrence_at(&occurrences, position) else {
        return Value::Null;
    };
    let (signature, description) = if occurrence.class_symbol {
        (format!("class {}", occurrence.name), "Class declaration.")
    } else if occurrence.function {
        (
            occurrence.ty.clone(),
            if occurrence.ty.contains(" -> ") {
                "Function returning a value."
            } else {
                "Function with no return value."
            },
        )
    } else if occurrence.parameter {
        (
            format!("{}: {}", occurrence.name, occurrence.ty),
            "Function parameter; read-only inside the function.",
        )
    } else if occurrence.mutable {
        (
            format!("var {}: {}", occurrence.name, occurrence.ty),
            "Mutable variable.",
        )
    } else {
        (
            format!("const {}: {}", occurrence.name, occurrence.ty),
            "Constant value.",
        )
    };
    let detail = format!("```rake\n{signature}\n```\n\n{description}");
    json!({
        "contents": {
            "kind": "markdown",
            "value": detail
        },
        "range": occurrence_range(occurrence)
    })
}

fn document_symbols(document: &Document) -> Value {
    let symbols: Vec<Value> = symbol_occurrences(document)
        .iter()
        .filter(|occurrence| occurrence.declaration)
        .map(|occurrence| {
            let range = occurrence_range(occurrence);
            json!({
                "name": occurrence.name,
                "detail": occurrence.ty,
                "kind": if occurrence.class_symbol { 5 } else if occurrence.function { 12 } else if occurrence.mutable || occurrence.parameter { 13 } else { 14 },
                "range": range,
                "selectionRange": range
            })
        })
        .collect();
    json!(symbols)
}

fn valid_identifier(name: &str) -> bool {
    let mut characters = name.chars();
    let Some(first) = characters.next() else {
        return false;
    };
    (first.is_ascii_alphabetic() || first == '_')
        && characters.all(|character| character.is_ascii_alphanumeric() || character == '_')
        && !matches!(
            name,
            "var"
                | "const"
                | "class"
                | "fn"
                | "if"
                | "else"
                | "while"
                | "true"
                | "false"
                | "int"
                | "float"
                | "bool"
        )
}

fn rename_result(uri: &str, document: &Document, position: Position, new_name: &str) -> Value {
    if !valid_identifier(new_name) {
        return Value::Null;
    }
    let occurrences = symbol_occurrences(document);
    let Some(selected) = occurrence_at(&occurrences, position) else {
        return Value::Null;
    };
    let edits: Vec<Value> = occurrences
        .iter()
        .filter(|entry| entry.symbol_id == selected.symbol_id)
        .map(|entry| {
            json!({
                "range": occurrence_range(entry),
                "newText": new_name
            })
        })
        .collect();
    let mut changes = HashMap::new();
    changes.insert(uri.to_string(), json!(edits));
    json!({"changes": changes})
}

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
