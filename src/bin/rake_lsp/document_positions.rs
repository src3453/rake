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
