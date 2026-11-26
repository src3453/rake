fn builtin_completion_item(
    block: &rake::BuiltinBlockDescription,
    label: &str,
    insert_name: &str,
) -> Value {
    let parameters = block
        .signature
        .split_once('(')
        .and_then(|(_, tail)| tail.strip_suffix(')'))
        .unwrap_or("");
    let arguments = parameters
        .split(", ")
        .enumerate()
        .filter_map(|(index, parameter)| {
            (!parameter.is_empty()).then(|| {
                format!(
                    "${{{}:{}}}",
                    index + 1,
                    parameter
                        .split_once(':')
                        .map(|(name, _)| name)
                        .unwrap_or(parameter)
                )
            })
        })
        .collect::<Vec<_>>()
        .join(", ");
    let extension_note = block.extension.map(|extension| {
        format!(
            " Scratch extension `{extension}`; available in {}.",
            block.runtimes.join(" and ")
        )
    }).unwrap_or_default();
    json!({
        "label": label,
        "kind": 3,
        "detail": format!("{}.{} — {}{}", block.namespace, block.signature, block.description, extension_note),
        "insertText": format!("{}({arguments})", insert_name),
        "insertTextFormat": 2,
        "sortText": format!("0-{label}")
    })
}

fn scratch_list_method_completion_items(
    descriptions: &[rake::BuiltinBlockDescription],
    prefix: &str,
) -> Vec<Value> {
    let mut items = vec![];
    for block in descriptions.iter().filter(|block| {
        block.namespace == "Scratch.Generics.List" && block.member.starts_with(prefix)
    }) {
        let parameters = block
            .signature
            .split_once('(')
            .and_then(|(_, tail)| tail.split_once(')').map(|(parameters, _)| parameters))
            .unwrap_or("");
        let arguments = parameters
            .split(", ")
            .skip(1)
            .enumerate()
            .map(|(index, parameter)| {
                let name = parameter
                    .split_once(':')
                    .map(|(name, _)| name)
                    .unwrap_or(parameter);
                format!("${{{}:{name}}}", index + 1)
            })
            .collect::<Vec<_>>()
            .join(", ");
        items.push(json!({
            "label": block.member,
            "kind": 3,
            "detail": format!("{} — {}", block.signature, block.description),
            "insertText": format!("{}({arguments})", block.member),
            "insertTextFormat": 2,
            "sortText": format!("0-{}", block.member)
        }));
    }
    items
}

fn scratch_namespace_completions(
    descriptions: &[rake::BuiltinBlockDescription],
    namespace: &str,
    prefix: &str,
) -> Vec<Value> {
    let mut child_namespaces = BTreeSet::new();
    let mut items = Vec::new();
    let child_prefix = format!("{namespace}.");

    for block in descriptions {
        if block.namespace == namespace {
            if block.member.starts_with(prefix) {
                items.push(builtin_completion_item(block, block.member, block.member));
            }
        } else if let Some(suffix) = block.namespace.strip_prefix(&child_prefix) {
            let child = suffix.split('.').next().unwrap_or_default();
            if child.starts_with(prefix) {
                child_namespaces.insert(child.to_string());
            }
        }
    }

    for child in child_namespaces {
        items.push(json!({
            "label": child,
            "kind": 9,
            "detail": "Scratch namespace",
            "insertText": child,
            "sortText": format!("0-{child}")
        }));
    }
    if namespace == "Scratch.Generics.List" && "new".starts_with(prefix) {
        items.push(json!({
            "label": "new",
            "kind": 3,
            "detail": "Create a named Scratch list in a module-scope variable declaration",
            "insertText": "new()",
            "sortText": "0-new"
        }));
    }
    items
}
