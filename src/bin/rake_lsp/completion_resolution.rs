fn use_member_namespace_at(
    tokens: &[SourceToken],
    position: Position,
    prefix: &str,
) -> Option<String> {
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
    let use_index = preceding.iter().rposition(|token| token.text == "use")?;
    let open_index = preceding
        .iter()
        .enumerate()
        .skip(use_index + 1)
        .filter(|(_, token)| token.text == "{")
        .map(|(index, _)| index)
        .next_back()?;
    if preceding.get(open_index.checked_sub(1)?)?.text != "." {
        return None;
    }
    let identifier = preceding.get(open_index.checked_sub(2)?)?;
    let token_index = tokens.iter().position(|token| {
        token.line == identifier.line && token.start_character == identifier.start_character
    })?;
    let receiver = qualified_identifier_name(tokens, token_index)?;
    source_namespace_alias(tokens, &receiver).or(Some(receiver))
}

fn namespace_at_index(tokens: &[SourceToken], end: usize) -> String {
    let mut braces: Vec<Option<String>> = Vec::new();
    let mut pending_namespace = None;
    for index in 0..end.min(tokens.len()) {
        match tokens[index].text.as_str() {
            "ns" if tokens.get(index + 1).map(|token| token.kind)
                == Some(SourceTokenKind::Identifier) =>
            {
                pending_namespace = Some(tokens[index + 1].text.clone());
            }
            "{" => braces.push(pending_namespace.take()),
            "}" => {
                braces.pop();
            }
            _ => {}
        }
    }
    braces.into_iter().flatten().collect::<Vec<_>>().join(".")
}

fn namespace_at_position(tokens: &[SourceToken], position: Position) -> String {
    let end = tokens
        .iter()
        .position(|token| source_position(token) >= position)
        .unwrap_or(tokens.len());
    namespace_at_index(tokens, end)
}

fn source_declares_namespace(tokens: &[SourceToken], path: &str) -> bool {
    tokens.iter().enumerate().any(|(index, token)| {
        if token.text != "ns" {
            return false;
        }
        let Some(name) = tokens.get(index + 1) else {
            return false;
        };
        let parent = namespace_at_index(tokens, index);
        if parent.is_empty() {
            name.text == path
        } else {
            format!("{parent}.{}", name.text) == path
        }
    })
}

fn qualified_identifier_name(tokens: &[SourceToken], index: usize) -> Option<String> {
    let token = tokens.get(index)?;
    if token.kind != SourceTokenKind::Identifier {
        return None;
    }
    let mut parts = vec![token.text.clone()];
    let mut cursor = index;
    while cursor >= 2
        && tokens[cursor - 1].text == "."
        && tokens[cursor - 2].kind == SourceTokenKind::Identifier
    {
        parts.push(tokens[cursor - 2].text.clone());
        cursor -= 2;
    }
    parts.reverse();
    Some(parts.join("."))
}

fn source_namespace_alias(tokens: &[SourceToken], name: &str) -> Option<String> {
    for index in 0..tokens.len() {
        if tokens[index].text != "use" {
            continue;
        }
        let mut cursor = index + 1;
        let mut path = Vec::new();
        if tokens.get(cursor).map(|token| token.kind) != Some(SourceTokenKind::Identifier) {
            continue;
        }
        path.push(tokens[cursor].text.clone());
        cursor += 1;
        while tokens.get(cursor).map(|token| token.text.as_str()) == Some(".")
            && tokens.get(cursor + 1).map(|token| token.kind) == Some(SourceTokenKind::Identifier)
        {
            path.push(tokens[cursor + 1].text.clone());
            cursor += 2;
        }
        if tokens.get(cursor).map(|token| token.text.as_str()) == Some("as")
            && tokens.get(cursor + 1).map(|token| token.text.as_str()) == Some(name)
        {
            return Some(path.join("."));
        }
        if tokens.get(cursor).map(|token| token.text.as_str()) != Some("as")
            && path.last().map(String::as_str) == Some(name)
        {
            return Some(path.join("."));
        }
    }
    None
}

fn resolved_namespace_alias(tokens: &[SourceToken], name: &str) -> Option<String> {
    if let Some(target) = source_namespace_alias(tokens, name) {
        return Some(target);
    }
    let parts: Vec<&str> = name.split('.').collect();
    for length in (1..parts.len()).rev() {
        let prefix = parts[..length].join(".");
        if let Some(target) = source_namespace_alias(tokens, &prefix) {
            return Some(format!("{target}.{}", parts[length..].join(".")));
        }
    }
    None
}

fn imported_builtin_members(tokens: &[SourceToken]) -> Vec<(String, String)> {
    let descriptions = builtin_block_descriptions();
    let mut imported = Vec::new();
    for start in 0..tokens.len() {
        if tokens[start].text != "use" {
            continue;
        }
        let mut cursor = start + 1;
        let Some(first) = tokens
            .get(cursor)
            .filter(|token| token.kind == SourceTokenKind::Identifier)
        else {
            continue;
        };
        let mut path = first.text.clone();
        cursor += 1;
        let mut members = Vec::new();
        let mut grouped = false;
        while tokens.get(cursor).map(|token| token.text.as_str()) == Some(".") {
            if tokens.get(cursor + 1).map(|token| token.text.as_str()) == Some("{") {
                cursor += 2;
                grouped = true;
                while let Some(member) = tokens.get(cursor) {
                    if member.text == "}" {
                        cursor += 1;
                        break;
                    }
                    if member.kind == SourceTokenKind::Identifier {
                        members.push(member.text.clone());
                    }
                    cursor += 1;
                }
                break;
            }
            let Some(segment) = tokens
                .get(cursor + 1)
                .filter(|token| token.kind == SourceTokenKind::Identifier)
            else {
                break;
            };
            path.push('.');
            path.push_str(&segment.text);
            cursor += 2;
        }
        let mut alias = None;
        if tokens.get(cursor).map(|token| token.text.as_str()) == Some("as") {
            alias = tokens.get(cursor + 1).map(|token| token.text.clone());
            cursor += 2;
        }
        if !grouped
            && tokens.get(cursor).map(|token| token.text.as_str()) == Some(".")
            && tokens.get(cursor + 1).map(|token| token.text.as_str()) == Some("{")
        {
            cursor += 2;
            grouped = true;
            while let Some(member) = tokens.get(cursor) {
                if member.text == "}" {
                    break;
                }
                if member.kind == SourceTokenKind::Identifier {
                    members.push(member.text.clone());
                }
                cursor += 1;
            }
            if tokens.get(cursor).map(|token| token.text.as_str()) == Some("}") {
                cursor += 1;
            }
        }
        if grouped
            && alias.is_none()
            && tokens.get(cursor).map(|token| token.text.as_str()) == Some("as")
        {
            alias = tokens.get(cursor + 1).map(|token| token.text.clone());
        }
        if grouped {
            for member in members {
                let canonical = format!("{path}.{member}");
                if descriptions.iter().any(|block| block.name == canonical) {
                    let local = alias
                        .as_ref()
                        .map(|alias| format!("{alias}.{member}"))
                        .unwrap_or(member);
                    imported.push((local, canonical));
                }
            }
        } else if let Some(alias) = alias {
            if descriptions.iter().any(|block| block.name == path) {
                imported.push((alias, path));
            } else {
                for block in descriptions
                    .iter()
                    .filter(|block| block.name.starts_with(&format!("{path}.")))
                {
                    imported.push((format!("{alias}.{}", block.member), block.name.to_string()));
                }
            }
        } else {
            if descriptions.iter().any(|block| block.name == path) {
                imported.push((path.rsplit('.').next().unwrap_or(&path).to_string(), path));
            } else {
                for block in descriptions
                    .iter()
                    .filter(|block| block.name.starts_with(&format!("{path}.")))
                {
                    let member = block.member;
                    let local = format!("{}.{member}", path.rsplit('.').next().unwrap_or(&path));
                    imported.push((local, block.name.to_string()));
                }
            }
        }
    }
    imported
}

fn source_imported_names(tokens: &[SourceToken]) -> Vec<(String, String)> {
    let mut imported = Vec::new();
    for start in 0..tokens.len() {
        if tokens[start].text != "use" {
            continue;
        }
        let mut cursor = start + 1;
        let Some(first) = tokens
            .get(cursor)
            .filter(|token| token.kind == SourceTokenKind::Identifier)
        else {
            continue;
        };
        let mut path = first.text.clone();
        cursor += 1;
        let mut members = Vec::new();
        let mut grouped = false;
        while tokens.get(cursor).map(|token| token.text.as_str()) == Some(".") {
            if tokens.get(cursor + 1).map(|token| token.text.as_str()) == Some("{") {
                cursor += 2;
                grouped = true;
                while let Some(member) = tokens.get(cursor) {
                    if member.text == "}" {
                        cursor += 1;
                        break;
                    }
                    if member.kind == SourceTokenKind::Identifier {
                        members.push(member.text.clone());
                    }
                    cursor += 1;
                }
                break;
            }
            let Some(segment) = tokens
                .get(cursor + 1)
                .filter(|token| token.kind == SourceTokenKind::Identifier)
            else {
                break;
            };
            path.push('.');
            path.push_str(&segment.text);
            cursor += 2;
        }
        let mut alias = None;
        if tokens.get(cursor).map(|token| token.text.as_str()) == Some("as") {
            alias = tokens.get(cursor + 1).map(|token| token.text.clone());
            cursor += 2;
        }
        if !grouped
            && tokens.get(cursor).map(|token| token.text.as_str()) == Some(".")
            && tokens.get(cursor + 1).map(|token| token.text.as_str()) == Some("{")
        {
            cursor += 2;
            grouped = true;
            while let Some(member) = tokens.get(cursor) {
                if member.text == "}" {
                    cursor += 1;
                    break;
                }
                if member.kind == SourceTokenKind::Identifier {
                    members.push(member.text.clone());
                }
                cursor += 1;
            }
        }
        if grouped
            && alias.is_none()
            && tokens.get(cursor).map(|token| token.text.as_str()) == Some("as")
        {
            alias = tokens.get(cursor + 1).map(|token| token.text.clone());
        }
        if grouped {
            for member in members {
                let local = alias
                    .as_ref()
                    .map(|alias| format!("{alias}.{member}"))
                    .unwrap_or(member.clone());
                imported.push((local, format!("{path}.{member}")));
            }
        } else if let Some(alias) = alias {
            imported.push((alias, path));
        }
    }
    imported
}

fn builtin_name_at(tokens: &[SourceToken], index: usize) -> Option<String> {
    let descriptions = builtin_block_descriptions();
    let name = qualified_identifier_name(tokens, index)?;
    if descriptions.iter().any(|block| block.name == name) {
        return Some(name);
    }
    if let Some((namespace, member)) = name.rsplit_once('.') {
        if let Some(target) = resolved_namespace_alias(tokens, namespace) {
            let canonical = format!("{target}.{member}");
            if descriptions.iter().any(|block| block.name == canonical) {
                return Some(canonical);
            }
        }
    }
    imported_builtin_members(tokens)
        .into_iter()
        .find_map(|(local, canonical)| (local == name).then_some(canonical))
}

fn imported_scratch_type_at(tokens: &[SourceToken], index: usize) -> Option<String> {
    let name = qualified_identifier_name(tokens, index)?;
    if matches!(
        name.as_str(),
        "Scratch.Generics.List" | "Scratch.Generics.Value"
    ) {
        return Some(name);
    }
    source_imported_names(tokens)
        .into_iter()
        .find_map(|(local, target)| {
            (local == name
                && matches!(
                    target.as_str(),
                    "Scratch.Generics.List" | "Scratch.Generics.Value"
                ))
            .then_some(target)
        })
}

fn source_declares_scratch_list(tokens: &[SourceToken], name: &str) -> bool {
    let imported_types = source_imported_names(tokens);
    for index in 0..tokens.len() {
        if !matches!(tokens[index].text.as_str(), "var" | "const")
            || tokens.get(index + 1).map(|token| token.text.as_str()) != Some(name)
        {
            continue;
        }
        let line = tokens[index].line;
        let end = (index + 2..tokens.len())
            .find(|cursor| tokens[*cursor].line != line)
            .unwrap_or(tokens.len());
        let Some(colon) = (index + 2..end).find(|cursor| tokens[*cursor].text == ":") else {
            continue;
        };
        let Some(equals) = (colon + 1..end).find(|cursor| tokens[*cursor].text == "=") else {
            continue;
        };
        let annotation = tokens[colon + 1..equals]
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        if annotation == "Scratch.Generics.List"
            || imported_types
                .iter()
                .any(|(local, target)| local == &annotation && target == "Scratch.Generics.List")
        {
            return true;
        }
    }
    false
}

fn scratch_list_method_name_at(
    tokens: &[SourceToken],
    index: usize,
    occurrences: &[SymbolOccurrence],
) -> Option<String> {
    let name = qualified_identifier_name(tokens, index)?;
    let (receiver, member) = name.rsplit_once('.')?;
    let is_list_handle = occurrences
        .iter()
        .any(|occurrence| occurrence.name == receiver && occurrence.scratch_list);
    if is_list_handle
        && builtin_block_descriptions()
            .iter()
            .any(|block| block.name == format!("Scratch.Generics.List.{member}"))
    {
        Some(format!("Scratch.Generics.List.{member}"))
    } else {
        None
    }
}

fn scratch_list_constructor_at(tokens: &[SourceToken], index: usize) -> bool {
    let Some(name) = qualified_identifier_name(tokens, index) else {
        return false;
    };
    let Some((receiver, member)) = name.rsplit_once('.') else {
        return false;
    };
    member == "new"
        && resolved_namespace_alias(tokens, receiver).as_deref() == Some("Scratch.Generics.List")
}

fn imported_function_at(tokens: &[SourceToken], index: usize) -> Option<FunctionCompletion> {
    let local_name = qualified_identifier_name(tokens, index)?;
    let canonical = source_imported_names(tokens)
        .into_iter()
        .find_map(|(local, canonical)| (local == local_name).then_some(canonical))?;
    function_completions(tokens)
        .into_iter()
        .find(|function| function.qualified_name == canonical)
}

fn instance_class_name(tokens: &[SourceToken], receiver: &str) -> Option<String> {
    for (index, token) in tokens.iter().enumerate() {
        if !matches!(token.text.as_str(), "var" | "const")
            || tokens.get(index + 1).map(|token| token.text.as_str()) != Some(receiver)
        {
            continue;
        }
        let mut cursor = index + 2;
        if tokens.get(cursor).map(|token| token.text.as_str()) == Some(":") {
            cursor += 1;
            let mut annotated = Vec::new();
            while let Some(token) = tokens.get(cursor) {
                if token.kind != SourceTokenKind::Identifier {
                    break;
                }
                annotated.push(token.text.clone());
                cursor += 1;
                if tokens.get(cursor).map(|token| token.text.as_str()) != Some(".") {
                    break;
                }
                cursor += 1;
            }
            if tokens.get(cursor).map(|token| token.text.as_str()) != Some("=") {
                continue;
            }
            return Some(annotated.join("."));
        }
        if tokens.get(cursor).map(|token| token.text.as_str()) != Some("=") {
            continue;
        }
        cursor += 1;
        if let Some(class_name) = class_name_from_initializer(tokens, cursor) {
            return Some(class_name);
        }
    }
    None
}

fn class_name_from_initializer(tokens: &[SourceToken], index: usize) -> Option<String> {
    let first = tokens.get(index)?;
    if first.kind != SourceTokenKind::Identifier {
        return None;
    }
    let mut name = first.text.clone();
    let mut cursor = index + 1;
    loop {
        match tokens.get(cursor).map(|token| token.text.as_str()) {
            Some("(") => return Some(name),
            Some(".") => {
                let segment = tokens.get(cursor + 1)?;
                if segment.kind != SourceTokenKind::Identifier {
                    return None;
                }
                if segment.text == "new"
                    && tokens.get(cursor + 2).map(|token| token.text.as_str()) == Some("(")
                {
                    return Some(name);
                }
                name.push('.');
                name.push_str(&segment.text);
                cursor += 2;
            }
            _ => return None,
        }
    }
}

fn class_declaration_matches(tokens: &[SourceToken], class_index: usize, name: &str) -> bool {
    let Some(class_token) = tokens.get(class_index + 1) else { return false };
    let namespace = namespace_at_index(tokens, class_index);
    let qualified = if namespace.is_empty() {
        class_token.text.clone()
    } else {
        format!("{namespace}.{}", class_token.text)
    };
    if name.contains('.') {
        qualified == name
    } else {
        qualified == name || class_token.text == name
    }
}

fn declared_class_names(tokens: &[SourceToken]) -> Vec<String> {
    tokens
        .windows(2)
        .filter(|pair| {
            pair[0].text == "class" && pair[1].kind == SourceTokenKind::Identifier
        })
        .map(|pair| pair[1].text.clone())
        .collect()
}

fn declared_class_paths(tokens: &[SourceToken]) -> Vec<String> {
    tokens
        .iter()
        .enumerate()
        .filter_map(|(index, token)| {
            if token.text != "class" {
                return None;
            }
            let class_name = tokens.get(index + 1)?;
            if class_name.kind != SourceTokenKind::Identifier {
                return None;
            }
            let namespace = namespace_at_index(tokens, index);
            Some(if namespace.is_empty() {
                class_name.text.clone()
            } else {
                format!("{namespace}.{}", class_name.text)
            })
        })
        .collect()
}

fn class_type_at(tokens: &[SourceToken], index: usize) -> bool {
    let Some(token) = tokens.get(index) else { return false };
    if token.kind != SourceTokenKind::Identifier {
        return false;
    }
    let mut start = index;
    while start >= 2
        && tokens[start - 1].text == "."
        && tokens[start - 2].kind == SourceTokenKind::Identifier
    {
        start -= 2;
    }
    if start == 0 || !matches!(tokens[start - 1].text.as_str(), ":" | "->") {
        return false;
    }
    let path = tokens[start..=index]
        .iter()
        .map(|token| token.text.as_str())
        .collect::<Vec<_>>()
        .join("");
    let classes = declared_class_paths(tokens);
    classes.iter().any(|class_name| class_name == &path)
        || classes
            .iter()
            .filter(|class_name| class_name.rsplit('.').next() == Some(token.text.as_str()))
            .count()
            == 1
}

fn class_body_range(tokens: &[SourceToken], class_index: usize) -> Option<(usize, usize)> {
    let open = (class_index + 2..tokens.len()).find(|index| tokens[*index].text == "{")?;
    let mut depth = 1usize;
    for index in open + 1..tokens.len() {
        match tokens[index].text.as_str() {
            "{" => depth += 1,
            "}" => {
                depth -= 1;
                if depth == 0 {
                    return Some((open, index));
                }
            }
            _ => {}
        }
    }
    None
}

fn class_at_position(tokens: &[SourceToken], position: Position) -> Option<String> {
    let cursor = tokens.iter().position(|token| {
        token.line > position.line
            || (token.line == position.line && token.start_character >= position.character)
    }).unwrap_or(tokens.len());
    tokens.iter().enumerate().rev().find_map(|(index, token)| {
        if token.text != "class" {
            return None;
        }
        let (open, close) = class_body_range(tokens, index)?;
        (cursor > open && cursor < close).then(|| {
            let namespace = namespace_at_index(tokens, index);
            if namespace.is_empty() {
                tokens[index + 1].text.clone()
            } else {
                format!("{namespace}.{}", tokens[index + 1].text)
            }
        })
    })
}

fn class_fields(tokens: &[SourceToken], class_name: &str) -> Vec<(String, String, bool, bool)> {
    let Some(class_index) = tokens.iter().enumerate().find_map(|(index, token)| {
        (token.text == "class" && class_declaration_matches(tokens, index, class_name))
        .then_some(index)
    }) else {
        return Vec::new();
    };
    let Some((open, close)) = class_body_range(tokens, class_index) else {
        return Vec::new();
    };
    let mut fields = Vec::new();
    let mut index = open + 1;
    let mut depth = 1usize;
    while index < close {
        match tokens[index].text.as_str() {
            "{" => {
                depth += 1;
                index += 1;
                continue;
            }
            "}" => {
                depth = depth.saturating_sub(1);
                index += 1;
                continue;
            }
            _ => {}
        }
        if depth != 1 {
            index += 1;
            continue;
        }
        let private = if matches!(tokens[index].text.as_str(), "public" | "private") {
            let private = tokens[index].text == "private";
            index += 1;
            private
        } else {
            false
        };
        if index + 3 < close
            && matches!(tokens[index].text.as_str(), "var" | "const")
            && tokens[index + 1].kind == SourceTokenKind::Identifier
            && tokens[index + 2].text == ":"
            && tokens[index + 3].kind == SourceTokenKind::Identifier
        {
            fields.push((
                tokens[index + 1].text.clone(),
                tokens[index + 3].text.clone(),
                tokens[index].text == "var",
                private,
            ));
            index += 4;
        } else {
            index += 1;
        }
    }
    fields
}

fn class_methods(tokens: &[SourceToken], class_name: &str) -> Vec<(FunctionCompletion, bool)> {
    let Some(class_index) = tokens.iter().enumerate().find_map(|(index, token)| {
        (token.text == "class" && class_declaration_matches(tokens, index, class_name))
        .then_some(index)
    }) else {
        return Vec::new();
    };
    let Some((open, close)) = class_body_range(tokens, class_index) else {
        return Vec::new();
    };
    let mut methods = Vec::new();
    let mut index = open + 1;
    while index < close {
        if tokens[index].text == "{" {
            let mut depth = 1usize;
            index += 1;
            while index < close && depth > 0 {
                match tokens[index].text.as_str() {
                    "{" => depth += 1,
                    "}" => depth -= 1,
                    _ => {}
                }
                index += 1;
            }
            continue;
        }
        let private = if matches!(tokens[index].text.as_str(), "public" | "private") {
            let private = tokens[index].text == "private";
            index += 1;
            private
        } else {
            false
        };
        let mut cursor = index;
        let mut warp = false;
        while matches!(tokens.get(cursor).map(|token| token.text.as_str()), Some("const" | "warp")) {
            warp |= tokens[cursor].text == "warp";
            cursor += 1;
        }
        if tokens.get(cursor).map(|token| token.text.as_str()) != Some("fn") {
            index += 1;
            continue;
        }
        let Some(name) = tokens.get(cursor + 1) else { break };
        if tokens.get(cursor + 2).map(|token| token.text.as_str()) != Some("(") {
            index += 1;
            continue;
        }
        let mut parameter_cursor = cursor + 3;
        let mut parameters = Vec::new();
        let mut valid = true;
        while parameter_cursor < close && tokens[parameter_cursor].text != ")" {
            if tokens[parameter_cursor].text == "," {
                parameter_cursor += 1;
                continue;
            }
            if tokens.get(parameter_cursor + 1).map(|token| token.text.as_str()) != Some(":")
                || tokens.get(parameter_cursor + 2).map(|token| token.kind)
                    != Some(SourceTokenKind::Identifier)
            {
                valid = false;
                break;
            }
            parameters.push((tokens[parameter_cursor].text.clone(), tokens[parameter_cursor + 2].text.clone()));
            parameter_cursor += 3;
        }
        if valid && tokens.get(parameter_cursor).map(|token| token.text.as_str()) == Some(")") {
            let return_type = (tokens.get(parameter_cursor + 1).map(|token| token.text.as_str()) == Some("->"))
                .then(|| tokens.get(parameter_cursor + 2).map(|token| token.text.clone()))
                .flatten();
            methods.push((FunctionCompletion {
                name: name.text.clone(),
                qualified_name: format!("{class_name}.{}", name.text),
                warp,
                parameters,
                return_type,
            }, private));
            index = parameter_cursor + 1;
            while index < close && tokens[index].text != "{" {
                index += 1;
            }
        } else {
            index = cursor + 1;
        }
    }
    methods
}

#[derive(Clone, Debug)]
struct FunctionCompletion {
    name: String,
    qualified_name: String,
    warp: bool,
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
        if tokens.iter().enumerate().any(|(class_index, class_token)| {
            class_token.text == "class"
                && class_body_range(tokens, class_index)
                    .is_some_and(|(open, close)| index > open && index < close)
        }) {
            continue;
        }
        let name = tokens[index + 1].text.clone();
        let namespace = namespace_at_index(tokens, index);
        let qualified_name = if namespace.is_empty() {
            name.clone()
        } else {
            format!("{namespace}.{name}")
        };
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
                qualified_name,
                warp: tokens[..index]
                    .iter()
                    .rev()
                    .take_while(|token| matches!(token.text.as_str(), "const" | "warp"))
                    .any(|token| token.text == "warp"),
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
