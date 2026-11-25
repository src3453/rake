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

    if event_type_context(&tokens, position, &prefix) {
        let event_types = [
            (
                "greenFlag",
                "Run when the green flag is clicked",
                "greenFlag() ${1:main} {\n\t$0\n}",
            ),
            (
                "keyPressed",
                "Run when a key is pressed",
                "keyPressed(\"${1:space}\") ${2:onSpace} {\n\t$0\n}",
            ),
            (
                "broadcastReceived",
                "Run when a broadcast is received",
                "broadcastReceived(\"${1:message}\") ${2:onMessage} {\n\t$0\n}",
            ),
            (
                "backdropSwitchedTo",
                "Run when the backdrop switches to a name",
                "backdropSwitchedTo(\"${1:backdrop1}\") ${2:onBackdrop} {\n\t$0\n}",
            ),
            (
                "spriteClicked",
                "Run when this sprite is clicked",
                "spriteClicked() ${1:onClick} {\n\t$0\n}",
            ),
            (
                "cloneStart",
                "Run when this clone starts",
                "cloneStart() ${1:onCloneStart} {\n\t$0\n}",
            ),
        ];
        return event_types
            .iter()
            .filter(|(name, _, _)| name.starts_with(&prefix))
            .map(|(name, detail, insert_text)| {
                json!({
                    "label": name,
                    "kind": 14,
                    "detail": detail,
                    "insertText": insert_text,
                    "insertTextFormat": 2,
                    "sortText": format!("0-{name}")
                })
            })
            .collect();
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
        return builtin_block_descriptions()
            .into_iter()
            .filter(|block| block.namespace == namespace && block.member.starts_with(&prefix))
            .map(|block| builtin_completion_item(&block, block.member, block.member))
            .collect();
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
            ("public", 14, "Public class member"),
            ("private", 14, "Private class member"),
            ("if", 14, "Conditional statement"),
            ("else", 14, "Alternative branch"),
            ("while", 14, "Loop statement"),
            ("fn", 14, "Function declaration"),
            ("warp", 14, "Declare a function that runs without screen refresh"),
            ("ns", 14, "Namespace declaration"),
            ("use", 14, "Import a namespace or declaration"),
            ("event", 14, "Declare a Scratch event handler"),
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
                    "event ${1:greenFlag}() ${2:main} {\n\t$0\n}"
                } else if label == "warp" {
                    "warp fn ${1:name}() {\n\t$0\n}"
                } else {
                    label
                },
                "insertTextFormat": if label == "event" || label == "warp" { 2 } else { 1 },
                "sortText": format!("0-{label}")
            }));
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
            let allow_private = class_at_position(&tokens, position).as_deref() == Some(class_name.as_str());
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
                "kind": if occurrence.class_symbol {
                    7
                } else if occurrence.mutable || occurrence.parameter {
                    6
                } else {
                    21
                },
                "detail": if occurrence.class_symbol {
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
