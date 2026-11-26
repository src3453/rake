fn symbol_occurrences(document: &Document) -> Vec<SymbolOccurrence> {
    analyze_symbols(&document.text).unwrap_or_default()
}

fn macro_token_at(document: &Document, position: Position) -> Option<SourceToken> {
    source_tokens(&document.text)
        .ok()?
        .into_iter()
        .find(|token| {
            token.kind == SourceTokenKind::Identifier
                && token.line == position.line
                && position.character >= token.start_character
                && position.character < token.start_character + token.length
        })
}

fn macro_declaration(document: &Document, name: &str) -> Option<SourceToken> {
    let tokens = source_tokens(&document.text).ok()?;
    (0..tokens.len().saturating_sub(2))
        .find(|&index| {
            tokens[index].text == "macro"
                && tokens[index + 1].text == "!"
                && tokens[index + 2].text == name
        })
        .map(|index| tokens[index + 2].clone())
}

fn definition_result(uri: &str, document: &Document, position: Position) -> Value {
    if let Some(token) = macro_token_at(document, position) {
        if let Some(declaration) = macro_declaration(document, &token.text) {
            return json!({
                "uri": uri,
                "range": {
                    "start": {"line": declaration.line, "character": declaration.start_character},
                    "end": {"line": declaration.line, "character": declaration.start_character + declaration.length}
                }
            });
        }
    }
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
    let tokens = source_tokens(&document.text).unwrap_or_default();
    if let Some(token) = macro_token_at(document, position) {
        if macro_declaration(document, &token.text).is_some() {
            let signature = format!("macro! {}(...)", token.text);
            return json!({
                "contents": {"kind": "markdown", "value": format!("```rake\n{signature}\n```\n\nToken substitution macro, expanded before parsing and type checking.")},
                "range": {
                    "start": {"line": token.line, "character": token.start_character},
                    "end": {"line": token.line, "character": token.start_character + token.length}
                }
            });
        }
    }
    let analyzed_occurrences = analyze_symbols(&document.text).unwrap_or_default();
    if let Some((index, token)) = tokens.iter().enumerate().find(|(_, token)| {
        token.line == position.line
            && position.character >= token.start_character
            && position.character < token.start_character + token.length
    }) {
        if let Some(name) = builtin_name_at(&tokens, index)
            .or_else(|| scratch_list_method_name_at(&tokens, index, &analyzed_occurrences))
        {
            if let Some(block) = builtin_block_descriptions()
                .into_iter()
                .find(|block| block.name == name)
            {
                let signature = format!("{}.{}", block.namespace, block.signature);
                let extension_note = block.extension.map(|extension| {
                    format!("\n\nScratch extension `{extension}`; available in {}.", block.runtimes.join(" and "))
                }).unwrap_or_default();
                let detail = format!("```rake\n{signature}\n```\n\n{}{}", block.description, extension_note);
                return json!({
                    "contents": {"kind": "markdown", "value": detail},
                    "range": {
                        "start": {"line": token.line, "character": token.start_character},
                        "end": {"line": token.line, "character": token.start_character + token.length}
                    }
                });
            }
        }
        if scratch_list_constructor_at(&tokens, index) {
            return json!({
                "contents": {
                    "kind": "markdown",
                    "value": "```rake\nScratch.Generics.List.new() -> Scratch.Generics.List\n```\n\nCreate a named Stage-wide Scratch list in a module-scope variable declaration."
                },
                "range": {
                    "start": {"line": token.line, "character": token.start_character},
                    "end": {"line": token.line, "character": token.start_character + token.length}
                }
            });
        }
        if let Some(type_name) = imported_scratch_type_at(&tokens, index) {
            let description = if type_name == "Scratch.Generics.Value" {
                "A dynamically typed scalar value stored in a Scratch list."
            } else {
                "A handle to a named Scratch list resource."
            };
            return json!({
                "contents": {
                    "kind": "markdown",
                    "value": format!("```rake\n{type_name}\n```\n\n{description}")
                },
                "range": {
                    "start": {"line": token.line, "character": token.start_character},
                    "end": {"line": token.line, "character": token.start_character + token.length}
                }
            });
        }
        if let Some(function) = imported_function_at(&tokens, index) {
            let signature_arguments = function
                .parameters
                .iter()
                .map(|(name, ty)| format!("{name}: {ty}"))
                .collect::<Vec<_>>()
                .join(", ");
            let return_type = function
                .return_type
                .as_ref()
                .map(|ty| format!(" -> {ty}"))
                .unwrap_or_default();
            let function_keyword = if function.warp { "warp fn" } else { "fn" };
            let signature = format!(
                "{function_keyword} {}({signature_arguments}){return_type}",
                function.qualified_name
            );
            return json!({
                "contents": {"kind": "markdown", "value": format!("```rake\n{signature}\n```\n\nImported function.")},
                "range": {
                    "start": {"line": token.line, "character": token.start_character},
                    "end": {"line": token.line, "character": token.start_character + token.length}
                }
            });
        }
    }
    let occurrences = symbol_occurrences(document);
    let Some(occurrence) = occurrence_at(&occurrences, position) else {
        return Value::Null;
    };
    let (signature, description) = if occurrence.event_message {
        (
            occurrence.ty.clone(),
            "Custom Scratch broadcast message shared by its send statements and receiver scripts.",
        )
    } else if occurrence.event_handler {
        (
            occurrence.ty.clone(),
            "Scratch event handler; its body runs as an independent script.",
        )
    } else if occurrence.namespace_symbol {
        (
            format!("ns {}", occurrence.name),
            "Namespace containing declarations.",
        )
    } else if occurrence.class_symbol {
        (format!("class {}", occurrence.name), "Class declaration.")
    } else if occurrence.enum_variant {
        (
            occurrence.ty.clone(),
            "Enum variant. Its tag is resolved at compile time.",
        )
    } else if occurrence.enum_symbol {
        (occurrence.ty.clone(), "Enum declaration or type reference.")
    } else if occurrence.scratch_list {
        (
            format!("var {}: Scratch.Generics.List", occurrence.name),
            "Named Scratch list handle. Its methods are lowered to Scratch.Generics.List operations.",
        )
    } else if occurrence.function {
        (
            occurrence.ty.clone(),
            if occurrence.private_member {
                "Private method; callable only from methods of the declaring class."
            } else if occurrence.ty.contains(" -> ") {
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
    } else if occurrence.private_member && occurrence.mutable {
        (
            format!("private var {}: {}", occurrence.name, occurrence.ty),
            "Private mutable class field.",
        )
    } else if occurrence.private_member {
        (
            format!("private const {}: {}", occurrence.name, occurrence.ty),
            "Private constant class field.",
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
                "kind": if occurrence.enum_variant { 22 } else if occurrence.enum_symbol { 10 } else if occurrence.event_handler { 24 } else if occurrence.namespace_symbol { 3 } else if occurrence.class_symbol { 5 } else if occurrence.function { 12 } else if occurrence.mutable || occurrence.parameter || occurrence.scratch_list { 13 } else { 14 },
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
                | "enum"
                | "public"
                | "private"
                | "self"
                | "scratch"
                | "list"
                | "fn"
                | "warp"
                | "event"
                | "ns"
                | "use"
                | "as"
                | "if"
                | "else"
                | "while"
                | "true"
                | "false"
                | "int"
                | "float"
                | "bool"
                | "text"
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
