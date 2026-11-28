fn event_insert_text(name: &str, event: &rake::BuiltinEventDescription) -> String {
    let selector = match event.member {
        "keyPressed" => Some("${1:space}"),
        "backdropSwitchedTo" => Some("${1:backdrop1}"),
        _ => None,
    };
    match selector {
        Some(selector) => format!("{name}(\"{selector}\") {{\n\t$0\n}}"),
        None => format!("{name} {{\n\t$0\n}}"),
    }
}

fn declared_custom_messages(tokens: &[SourceToken]) -> Vec<String> {
    let mut builtin_aliases = std::collections::HashSet::new();
    for event in builtin_event_descriptions() {
        if source_namespace_alias(tokens, event.member).as_deref() == Some(event.name) {
            builtin_aliases.insert(event.member.to_string());
        }
        for (local, target) in source_imported_names(tokens) {
            if target == event.name {
                builtin_aliases.insert(local);
            }
        }
    }
    let mut messages = std::collections::BTreeSet::new();
    for index in 0..tokens.len() {
        if tokens[index].text != "event" {
            continue;
        }
        let Some(name) = tokens.get(index + 1) else {
            continue;
        };
        if name.kind != SourceTokenKind::Identifier
            || tokens.get(index + 2).map(|token| token.text.as_str()) != Some("{")
            || builtin_aliases.contains(&name.text)
        {
            continue;
        }
        messages.insert(name.text.clone());
    }
    messages.into_iter().collect()
}

fn completion_items(document: Option<&Document>, params: &Value) -> Vec<Value> {
    let position = position_from_params(params).unwrap_or(Position {
        line: 0,
        character: 0,
    });
    let source = document.map(|doc| doc.text.as_str()).unwrap_or("");
    if let Some(items) = document.and_then(|document| library_path_completion_items(document, position)) {
        return items;
    }
    let tokens = source_tokens(source).unwrap_or_default();
    let occurrences = document.map(analyze_document).unwrap_or_default();
    let prefix = prefix_at(source, position);
    let type_context = is_type_context(&tokens, position, &prefix);
    let member_receiver = member_receiver_at(&tokens, position, &prefix);

    if let Some(receiver) = qualified_receiver_at(&tokens, position, &prefix) {
        let variants = enum_variants_for_receiver(&tokens, &receiver);
        if !variants.is_empty() {
            return variants
                .into_iter()
                .filter(|(name, _)| name.starts_with(&prefix))
                .map(|(name, tag)| {
                    json!({
                        "label": name,
                        "kind": 20,
                        "detail": format!("Enum variant (compile-time tag {tag})"),
                        "insertText": name,
                        "sortText": format!("0-{name}")
                    })
                })
                .collect();
        }
    }

    if let Some(namespace) = event_member_namespace_at(&tokens, position, &prefix) {
        let resolved_namespace =
            resolved_namespace_alias(&tokens, &namespace).unwrap_or_else(|| namespace.clone());
        if resolved_namespace == "Scratch" {
            if "Events".starts_with(&prefix) {
                return vec![json!({
                    "label": "Events",
                    "kind": 9,
                    "detail": "Scratch standard events",
                    "insertText": "Events.",
                    "sortText": "0-Events"
                })];
            }
            return Vec::new();
        }
        if resolved_namespace == "Scratch.Events" {
            return builtin_event_descriptions()
                .iter()
                .filter(|event| event.member.starts_with(&prefix))
                .map(|event| {
                    let insert_text = event_insert_text(event.member, event);
                    json!({
                        "label": event.member,
                        "kind": 14,
                        "detail": format!("{} — {}", event.signature, event.description),
                        "insertText": insert_text,
                        "insertTextFormat": 2,
                        "sortText": format!("0-{}", event.member)
                    })
                })
                .collect();
        }
    }

    if event_type_context(&tokens, position, &prefix) {
        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for event in builtin_event_descriptions() {
            let mut names = vec![event.name.to_string()];
            if source_namespace_alias(&tokens, event.member).as_deref() == Some(event.name) {
                names.push(event.member.to_string());
            }
            for (local, target) in source_imported_names(&tokens) {
                if target == "Scratch.Events" && local.is_empty() {
                    names.push(event.member.to_string());
                } else if target == "Scratch.Events" {
                    names.push(format!("{local}.{}", event.member));
                } else if target == event.name {
                    names.push(local);
                }
            }
            if resolved_namespace_alias(&tokens, "Events").as_deref() == Some("Scratch.Events") {
                names.push(format!("Events.{}", event.member));
            }
            for name in names {
                if !name.starts_with(&prefix) || !seen.insert(name.clone()) {
                    continue;
                }
                items.push(json!({
                    "label": name,
                    "kind": 14,
                    "detail": format!("{} — {}", event.signature, event.description),
                    "insertText": event_insert_text(&name, event),
                    "insertTextFormat": 2,
                    "sortText": format!("0-{}", name)
                }));
            }
        }
        for name in declared_custom_messages(&tokens) {
            if name.starts_with(&prefix) && seen.insert(name.clone()) {
                items.push(json!({
                    "label": name,
                    "kind": 14,
                    "detail": format!("Custom Scratch message: {name}"),
                    "insertText": format!("{name} {{\n\t$0\n}}"),
                    "insertTextFormat": 2,
                    "sortText": format!("1-{name}")
                }));
            }
        }
        return items;
    }

    if send_message_context(&tokens, position, &prefix) {
        let mut items = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for name in declared_custom_messages(&tokens) {
            if name.starts_with(&prefix) && seen.insert(name.clone()) {
                items.push(json!({
                    "label": name,
                    "kind": 21,
                    "detail": "Scratch broadcast message",
                    "insertText": name,
                    "sortText": format!("0-{name}")
                }));
            }
        }
        return items;
    }

    if type_context {
        if let Some(receiver) = qualified_receiver_at(&tokens, position, &prefix) {
            let namespace =
                resolved_namespace_alias(&tokens, &receiver).unwrap_or_else(|| receiver.clone());
            let types: &[(&str, &str)] = match namespace.as_str() {
                "Scratch.Generics" => &[
                    ("Value", "Scratch scalar value type"),
                    ("List", "Scratch list handle type"),
                ],
                "Scratch" => &[("Generics", "Scratch generic data types")],
                _ => &[],
            };
            let items = types
                .iter()
                .filter(|(name, _)| name.starts_with(&prefix))
                .map(|(name, detail)| {
                    json!({
                        "label": name,
                        "kind": 7,
                        "detail": detail,
                        "insertText": name,
                        "sortText": format!("0-{name}")
                    })
                })
                .collect::<Vec<_>>();
            if !items.is_empty() {
                return items;
            }
        }
        let generic_types = [
            ("Scratch.Generics.Value", "Scratch scalar value type"),
            ("Scratch.Generics.List", "Scratch list handle type"),
        ];
        if prefix.starts_with("Scratch") {
            return generic_types
                .iter()
                .filter(|(name, _)| name.starts_with(&prefix))
                .map(|(name, detail)| {
                    json!({
                        "label": name,
                        "kind": 7,
                        "detail": detail,
                        "insertText": name,
                        "sortText": format!("0-{name}")
                    })
                })
                .collect();
        }
    }

    if let Some(namespace) = use_member_namespace_at(&tokens, position, &prefix) {
        let mut items = builtin_block_descriptions()
            .into_iter()
            .filter(|block| block.namespace == namespace && block.member.starts_with(&prefix))
            .map(|block| builtin_completion_item(&block, block.member, block.member))
            .collect::<Vec<_>>();
        if namespace == "Scratch.Events" {
            items.extend(
                builtin_event_descriptions()
                    .iter()
                    .filter(|event| event.member.starts_with(&prefix))
                    .map(|event| {
                        json!({
                            "label": event.member,
                            "kind": 14,
                            "detail": format!("{} — {}", event.signature, event.description),
                            "insertText": event.member,
                            "sortText": format!("0-{}", event.member)
                        })
                    }),
            );
        }
        return items;
    }

    if let Some(receiver) = qualified_receiver_at(&tokens, position, &prefix) {
        if occurrences
            .iter()
            .any(|occurrence| occurrence.name == receiver && occurrence.scratch_list)
            || source_declares_scratch_list(&tokens, &receiver)
        {
            return scratch_list_method_completion_items(&builtin_block_descriptions(), &prefix);
        }
        let imported_namespace = resolved_namespace_alias(&tokens, &receiver);
        let is_imported_namespace = imported_namespace.is_some();
        let known_namespace = receiver == "Scratch"
            || receiver.starts_with("Scratch.")
            || source_declares_namespace(&tokens, &receiver)
            || is_imported_namespace;
        let namespace = imported_namespace.unwrap_or_else(|| receiver.clone());
        let descriptions = builtin_block_descriptions();
        if known_namespace && namespace == "Scratch" {
            return scratch_namespace_completions(&descriptions, &namespace, &prefix);
        }
        if known_namespace && namespace.starts_with("Scratch.") {
            if is_imported_namespace {
                return scratch_namespace_completions(&descriptions, &namespace, &prefix);
            }
            return scratch_namespace_completions(&descriptions, &namespace, &prefix);
        }
        if known_namespace {
            return function_completions(&tokens)
            .into_iter()
            .filter(|function| {
                function.qualified_name.rsplit_once('.').map(|(parent, _)| parent) == Some(namespace.as_str())
                    && function.name.starts_with(&prefix)
            })
            .map(|function| {
                let signature = function.parameters.iter().map(|(name, ty)| format!("{name}: {ty}")).collect::<Vec<_>>().join(", ");
                let return_type = function.return_type.map(|ty| format!(" -> {ty}")).unwrap_or_default();
                let arguments = function.parameters.iter().enumerate().map(|(index, (name, _))| format!("${{{}:{name}}}", index + 1)).collect::<Vec<_>>().join(", ");
                json!({
                    "label": function.name,
                    "kind": 3,
                    "detail": format!("fn {}({signature}){return_type}", function.qualified_name),
                    "insertText": format!("{}({arguments})", function.name),
                    "insertTextFormat": 2,
                    "sortText": format!("0-{}", function.name)
                })
            })
            .collect();
        }
    }

    let mut items = Vec::new();
    if !type_context && member_receiver.is_none() {
        for index in 0..tokens.len().saturating_sub(2) {
            if tokens[index].text != "macro" || tokens[index + 1].text != "!" {
                continue;
            }
            let Some(name) = tokens
                .get(index + 2)
                .filter(|token| token.kind == SourceTokenKind::Identifier)
            else {
                continue;
            };
            if !name.text.starts_with(&prefix) {
                continue;
            }
            let mut parameters = Vec::new();
            let mut cursor = index + 3;
            if tokens.get(cursor).map(|token| token.text.as_str()) == Some("(") {
                cursor += 1;
                while let Some(token) = tokens.get(cursor) {
                    if token.text == ")" {
                        break;
                    }
                    if token.kind == SourceTokenKind::Identifier {
                        parameters.push(token.text.clone());
                    }
                    cursor += 1;
                }
            }
            let args = parameters
                .iter()
                .enumerate()
                .map(|(n, value)| format!("${{{}:{value}}}", n + 1))
                .collect::<Vec<_>>()
                .join(", ");
            items.push(json!({
                "label": name.text,
                "kind": 3,
                "detail": format!("macro! {}({})", name.text, parameters.join(", ")),
                "insertText": format!("{}!({args})", name.text),
                "insertTextFormat": 2,
                "sortText": format!("0-{}", name.text)
            }));
        }
    }
    let candidates: &[(&str, u32, &str)] = if type_context {
        &[
            ("int", 7, "Integer type"),
            ("float", 7, "Floating-point type"),
            ("bool", 7, "Boolean type"),
            ("text", 7, "Text type"),
            ("Scratch.Generics.Value", 7, "Scratch scalar value type"),
            ("Scratch.Generics.List", 7, "Scratch list handle type"),
        ]
    } else if member_receiver.is_some() {
        &[]
    } else {
        &[
            ("var", 14, "Mutable variable declaration"),
            ("const", 14, "Constant declaration"),
            ("class", 14, "Class declaration"),
            ("enum", 14, "Compile-time enumeration declaration"),
            ("macro", 14, "Token substitution macro declaration"),
            ("public", 14, "Public class member"),
            ("private", 14, "Private class member"),
            ("if", 14, "Conditional statement"),
            ("else", 14, "Alternative branch"),
            ("while", 14, "Loop statement"),
            ("match", 14, "Match an enum or scalar value"),
            ("fn", 14, "Function declaration"),
            (
                "warp",
                14,
                "Declare a function that runs without screen refresh",
            ),
            ("ns", 14, "Namespace declaration"),
            ("use", 14, "Import a namespace or declaration"),
            ("from", 14, "Import from a library file: from \"lib/path\" use ..."),
            ("event", 14, "Declare a Scratch event handler"),
            ("send", 14, "Broadcast a custom Scratch message"),
            ("Scratch", 9, "Scratch built-in namespaces"),
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
                "insertText": if label == "event" {
                    "event Scratch.Events.greenFlag {\n\t$0\n}"
                } else if label == "send" {
                    "send ${1:message}"
                } else if label == "warp" {
                    "warp fn ${1:name}() {\n\t$0\n}"
                } else if label == "enum" {
                    "enum ${1:Name} {\n\t${2:First},\n\t${3:Second},\n}"
                } else if label == "macro" {
                    "macro! ${1:name} (${2:argument}) {\n\t$0\n}"
                } else {
                    label
                },
                "insertTextFormat": if label == "event" || label == "send" || label == "warp" || label == "enum" || label == "macro" { 2 } else { 1 },
                "sortText": format!("0-{label}")
            }));
        }
    }

    if !type_context && member_receiver.is_none() {
        for conversion in conversion_descriptions() {
            if conversion.name.starts_with(&prefix) {
                items.push(json!({
                    "label": format!("{}()", conversion.name),
                    "filterText": conversion.name,
                    "kind": 3,
                    "detail": conversion.signature,
                    "documentation": conversion.description,
                    "insertText": format!("{}(${{1:value}})", conversion.name),
                    "insertTextFormat": 2,
                    "sortText": format!("1-{}", conversion.name)
                }));
            }
        }
    }

    if type_context {
        for class_name in declared_class_names(&tokens) {
            if class_name.starts_with(&prefix) {
                items.push(json!({
                    "label": class_name,
                    "kind": 7,
                    "detail": "Class type",
                    "insertText": class_name,
                    "sortText": format!("0-{class_name}")
                }));
            }
        }
        for enum_path in declared_enum_paths(&tokens) {
            let enum_name = enum_path.rsplit('.').next().unwrap_or(&enum_path);
            if enum_name.starts_with(&prefix) {
                items.push(json!({
                    "label": enum_name,
                    "kind": 13,
                    "detail": format!("Enum type: {enum_path}"),
                    "insertText": enum_name,
                    "sortText": format!("0-{enum_name}")
                }));
            }
        }
        for (local, target) in source_imported_names(&tokens) {
            if local.starts_with(&prefix)
                && declared_enum_paths(&tokens)
                    .iter()
                    .any(|path| path == &target)
            {
                items.push(json!({
                    "label": local,
                    "kind": 13,
                    "detail": format!("Enum type: {target}"),
                    "insertText": local,
                    "sortText": format!("0-{local}")
                }));
            }
        }
        for (local, target) in source_imported_names(&tokens) {
            if !local.contains('.')
                && local.starts_with(&prefix)
                && matches!(
                    target.as_str(),
                    "Scratch.Generics.List" | "Scratch.Generics.Value"
                )
            {
                items.push(json!({
                    "label": local,
                    "kind": 7,
                    "detail": target,
                    "insertText": local,
                    "sortText": format!("0-{local}")
                }));
            }
        }
    }

    if !type_context {
        for (local, canonical) in imported_builtin_members(&tokens) {
            if local.contains('.') || !local.starts_with(&prefix) {
                continue;
            }
            if let Some(block) = builtin_block_descriptions()
                .iter()
                .find(|block| block.name == canonical)
            {
                items.push(builtin_completion_item(block, &local, &local));
            }
        }
    }

    if let Some(receiver) = &member_receiver {
        let class_name = if receiver == "self" {
            class_at_position(&tokens, position)
        } else {
            instance_class_name(&tokens, receiver)
        };
        if let Some(class_name) = class_name {
            let allow_private =
                class_at_position(&tokens, position).as_deref() == Some(class_name.as_str());
            for (name, ty, mutable, private) in class_fields(&tokens, &class_name) {
                if private && !allow_private {
                    continue;
                }
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
            for (method, private) in class_methods(&tokens, &class_name) {
                if private && !allow_private || !method.name.starts_with(&prefix) {
                    continue;
                }
                let arguments = method
                    .parameters
                    .iter()
                    .enumerate()
                    .map(|(index, (name, _))| format!("${{{}:{name}}}", index + 1))
                    .collect::<Vec<_>>()
                    .join(", ");
                let signature = method
                    .parameters
                    .iter()
                    .map(|(name, ty)| format!("{name}: {ty}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                let return_type = method
                    .return_type
                    .as_ref()
                    .map(|ty| format!(" -> {ty}"))
                    .unwrap_or_default();
                items.push(json!({
                    "label": method.name,
                    "kind": 2,
                    "detail": format!("{} fn {}({signature}){return_type}", if method.warp { "warp" } else { "" }, method.qualified_name).trim(),
                    "insertText": format!("{}({arguments})", method.name),
                    "insertTextFormat": 2,
                    "sortText": format!("0-{}", method.name)
                }));
            }
        }
        return items;
    }

    if !type_context {
        let declaration_context = function_name_context(&tokens, position, &prefix);
        let active_namespace = namespace_at_position(&tokens, position);
        for function in function_completions(&tokens) {
            let visible_name = if function
                .qualified_name
                .rsplit_once('.')
                .map(|(parent, _)| parent)
                == Some(active_namespace.as_str())
            {
                function.name.clone()
            } else {
                function.qualified_name.clone()
            };
            if !visible_name.starts_with(&prefix) {
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
            let function_keyword = if function.warp { "warp fn" } else { "fn" };
            let detail = format!(
                "{function_keyword} {}({signature}){return_annotation}",
                function.qualified_name
            );
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
                format!("{}({arguments})", visible_name)
            };
            let mut item = json!({
                "label": visible_name,
                "kind": 3,
                "detail": detail,
                "insertText": insert_text,
                "sortText": format!("1-{}", visible_name)
            });
            if !declaration_context {
                item["insertTextFormat"] = json!(2);
            }
            items.push(item);
        }
        let function_definitions = function_completions(&tokens);
        for (local_name, qualified_name) in source_imported_names(&tokens) {
            if local_name.contains('.') || !local_name.starts_with(&prefix) {
                continue;
            }
            let Some(function) = function_definitions
                .iter()
                .find(|function| function.qualified_name == qualified_name)
            else {
                continue;
            };
            let signature = function
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
            let arguments = function
                .parameters
                .iter()
                .enumerate()
                .map(|(index, (name, _))| format!("${{{}:{name}}}", index + 1))
                .collect::<Vec<_>>()
                .join(", ");
            let function_keyword = if function.warp { "warp fn" } else { "fn" };
            items.push(json!({
                "label": local_name,
                "kind": 3,
                "detail": format!("{function_keyword} {}({signature}){return_type}", function.qualified_name),
                "insertText": format!("{}({arguments})", local_name),
                "insertTextFormat": 2,
                "sortText": format!("0-{local_name}")
            }));
        }
    }

    if !type_context {
        for occurrence in visible_declarations(&tokens, &occurrences, position) {
            if !occurrence.name.starts_with(&prefix) {
                continue;
            }
            items.push(json!({
                "label": occurrence.name,
                "kind": if occurrence.enum_variant {
                    20
                } else if occurrence.enum_symbol {
                    13
                } else if occurrence.class_symbol {
                    7
                } else if occurrence.mutable || occurrence.parameter {
                    6
                } else {
                    21
                },
                "detail": if occurrence.enum_variant {
                    occurrence.ty.clone()
                } else if occurrence.enum_symbol {
                    occurrence.ty.clone()
                } else if occurrence.class_symbol {
                    "class declaration".to_string()
                } else if occurrence.scratch_list {
                    "Scratch.Generics.List handle".to_string()
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
