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

    for (token_index, token) in tokens.iter().enumerate() {
        let token_type = match token.kind {
            SourceTokenKind::Identifier
                if matches!(token.text.as_str(), "int" | "float" | "bool" | "text") =>
            {
                Some(types["type"])
            }
            SourceTokenKind::Identifier
                if imported_scratch_type_at(&tokens, token_index).is_some()
                    && is_type_context(
                        &tokens,
                        Position {
                            line: token.line,
                            character: token.start_character + token.length,
                        },
                        &token.text,
                    ) =>
            {
                Some(types["type"])
            }
            SourceTokenKind::Identifier if class_type_at(&tokens, token_index) => {
                Some(types["type"])
            }
            SourceTokenKind::Identifier
                if matches!(
                    token.text.as_str(),
                    "var"
                        | "const"
                        | "class"
                        | "public"
                        | "private"
                        | "self"
                        | "fn"
                        | "warp"
                        | "event"
                        | "send"
                        | "ns"
                        | "use"
                        | "as"
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
                if token.text == "Scratch"
                    || (token_index > 0 && tokens[token_index - 1].text == "ns")
                    || qualified_identifier_name(&tokens, token_index).is_some_and(|name| {
                        builtin_block_descriptions().iter().any(|block| {
                            block.namespace == name
                                || block.namespace.starts_with(&format!("{name}."))
                        })
                    }) =>
            {
                Some(types["namespace"])
            }
            SourceTokenKind::Identifier
                if builtin_name_at(&tokens, token_index).is_some()
                    || scratch_list_method_name_at(&tokens, token_index, &occurrences)
                        .is_some()
                    || scratch_list_constructor_at(&tokens, token_index) =>
            {
                Some(types["function"])
            }
            SourceTokenKind::Identifier if imported_function_at(&tokens, token_index).is_some() => {
                Some(types["function"])
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
                    .map(|occurrence| occurrence.function || occurrence.event_handler)
                    .unwrap_or(false) =>
            {
                Some(types["function"])
            }
            SourceTokenKind::Identifier => Some(types["variable"]),
            SourceTokenKind::Number => Some(types["number"]),
            SourceTokenKind::String => Some(types["string"]),
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
