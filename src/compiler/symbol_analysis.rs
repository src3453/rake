fn symbol_occurrence(
    source: &str,
    token: &Token,
    symbol: &Symbol,
    declaration: bool,
    modification: bool,
) -> SymbolOccurrence {
    let line_text = source.split('\n').nth(token.line - 1).unwrap_or("");
    let character = line_text
        .trim_end_matches('\r')
        .chars()
        .take(token.column - 1)
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    SymbolOccurrence {
        symbol_id: symbol.semantic_id.clone(),
        name: symbol.source_name.clone(),
        ty: symbol.ty.name().to_string(),
        mutable: symbol.mutable,
        parameter: symbol.argument_id.is_some(),
        function: false,
        event_handler: false,
        event_message: false,
        class_symbol: false,
        enum_symbol: false,
        enum_variant: false,
        enum_type_id: match symbol.ty { Type::Enum(id) => Some(id), _ => None },
        namespace_symbol: false,
        scratch_list: symbol.ty == Type::ScratchList,
        private_member: symbol.visibility == Visibility::Private,
        line: (token.line - 1) as u32,
        character,
        length: token.text.encode_utf16().count() as u32,
        declaration,
        modification,
    }
}

fn event_symbol_occurrence(
    source: &str,
    token: &Token,
    kind: &EventKind,
) -> SymbolOccurrence {
    let line_text = source.split('\n').nth(token.line - 1).unwrap_or("");
    let character = line_text
        .trim_end_matches('\r')
        .chars()
        .take(token.column - 1)
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    let (symbol_id, name, signature, event_message) = match kind {
        EventKind::GreenFlag => (
            format!("scratch-event:{}:{}", token.line, token.column),
            "greenFlag".to_string(),
            "event Scratch.Events.greenFlag".to_string(),
            false,
        ),
        EventKind::KeyPressed(key) => (
            format!("scratch-event:{}:{}", token.line, token.column),
            "keyPressed".to_string(),
            format!("event Scratch.Events.keyPressed({key:?})"),
            false,
        ),
        EventKind::Message(message) => (
            format!("message:{message}"),
            message.clone(),
            format!("event {message}"),
            true,
        ),
        EventKind::SpriteClicked => (
            format!("scratch-event:{}:{}", token.line, token.column),
            "spriteClicked".to_string(),
            "event Scratch.Events.spriteClicked".to_string(),
            false,
        ),
        EventKind::CloneStart => (
            format!("scratch-event:{}:{}", token.line, token.column),
            "cloneStart".to_string(),
            "event Scratch.Events.cloneStart".to_string(),
            false,
        ),
        EventKind::BackdropSwitchedTo(backdrop) => (
            format!("scratch-event:{}:{}", token.line, token.column),
            "backdropSwitchedTo".to_string(),
            format!("event Scratch.Events.backdropSwitchedTo({backdrop:?})"),
            false,
        ),
    };
    SymbolOccurrence {
        symbol_id,
        name,
        ty: signature,
        mutable: false,
        parameter: false,
        function: false,
        event_handler: true,
        event_message,
        class_symbol: false,
        enum_symbol: false,
        enum_variant: false,
        enum_type_id: None,
        namespace_symbol: false,
        scratch_list: false,
        private_member: false,
        line: (token.line - 1) as u32,
        character,
        length: token.text.encode_utf16().count() as u32,
        declaration: true,
        modification: false,
    }
}

fn message_reference_occurrence(
    source: &str,
    token: &Token,
    message: &str,
) -> SymbolOccurrence {
    let line_text = source.split('\n').nth(token.line - 1).unwrap_or("");
    let character = line_text
        .trim_end_matches('\r')
        .chars()
        .take(token.column - 1)
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    SymbolOccurrence {
        symbol_id: format!("message:{message}"),
        name: message.to_string(),
        ty: format!("event {message}"),
        mutable: false,
        parameter: false,
        function: false,
        event_handler: true,
        event_message: true,
        class_symbol: false,
        enum_symbol: false,
        enum_variant: false,
        enum_type_id: None,
        namespace_symbol: false,
        scratch_list: false,
        private_member: false,
        line: (token.line - 1) as u32,
        character,
        length: token.text.encode_utf16().count() as u32,
        declaration: false,
        modification: false,
    }
}

fn field_symbol_occurrence(
    source: &str,
    token: &Token,
    symbol: &Symbol,
    declaration: bool,
    modification: bool,
) -> SymbolOccurrence {
    let mut occurrence = symbol_occurrence(source, token, symbol, declaration, modification);
    occurrence.name = token.text.clone();
    occurrence
}

fn class_symbol_occurrence(
    source: &str,
    token: &Token,
    name: &str,
    class_id: u32,
    declaration: bool,
) -> SymbolOccurrence {
    let symbol = Symbol {
        source_name: name.to_string(),
        scratch_name: name.to_string(),
        ty: Type::Class(class_id),
        mutable: false,
        visibility: Visibility::Public,
        id: format!("class:{name}"),
        semantic_id: format!("class:{name}"),
        argument_id: None,
        storage: StorageClass::ObjectBinding,
        initial_value: Value::Null,
    };
    let mut occurrence = symbol_occurrence(source, token, &symbol, declaration, false);
    occurrence.class_symbol = true;
    occurrence
}

fn enum_symbol_occurrence(
    source: &str,
    token: &Token,
    name: &str,
    enum_id: u32,
    declaration: bool,
) -> SymbolOccurrence {
    let symbol = Symbol {
        source_name: name.to_string(),
        scratch_name: name.to_string(),
        ty: Type::Enum(enum_id),
        mutable: false,
        visibility: Visibility::Public,
        id: format!("enum:{name}"),
        semantic_id: format!("enum:{name}"),
        argument_id: None,
        storage: StorageClass::ObjectBinding,
        initial_value: Value::Null,
    };
    let mut occurrence = symbol_occurrence(source, token, &symbol, declaration, false);
    occurrence.name = name.rsplit('.').next().unwrap_or(name).to_string();
    occurrence.ty = format!("enum {name}");
    occurrence.enum_symbol = true;
    occurrence.enum_type_id = Some(enum_id);
    occurrence
}

fn enum_variant_occurrence(
    source: &str,
    token: &Token,
    enum_name: &str,
    enum_id: u32,
    variant_name: &str,
    value: i64,
    declaration: bool,
) -> SymbolOccurrence {
    let line_text = source.split('\n').nth(token.line - 1).unwrap_or("");
    let character = line_text
        .trim_end_matches('\r')
        .chars()
        .take(token.column - 1)
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    SymbolOccurrence {
        symbol_id: format!("enum:{enum_name}:variant:{variant_name}"),
        name: variant_name.to_string(),
        ty: format!("{enum_name}::{variant_name} = {value}"),
        mutable: false,
        parameter: false,
        function: false,
        event_handler: false,
        event_message: false,
        class_symbol: false,
        enum_symbol: false,
        enum_variant: true,
        enum_type_id: Some(enum_id),
        namespace_symbol: false,
        scratch_list: false,
        private_member: false,
        line: (token.line - 1) as u32,
        character,
        length: token.text.encode_utf16().count() as u32,
        declaration,
        modification: false,
    }
}

fn namespace_symbol_occurrence(source: &str, token: &Token, path: &str) -> SymbolOccurrence {
    let line_text = source.split('\n').nth(token.line - 1).unwrap_or("");
    let character = line_text
        .trim_end_matches('\r')
        .chars()
        .take(token.column - 1)
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    SymbolOccurrence {
        symbol_id: format!("namespace:{path}"),
        name: path.to_string(),
        ty: "namespace".to_string(),
        mutable: false,
        parameter: false,
        function: false,
        event_handler: false,
        event_message: false,
        class_symbol: false,
        enum_symbol: false,
        enum_variant: false,
        enum_type_id: None,
        namespace_symbol: true,
        scratch_list: false,
        private_member: false,
        line: (token.line - 1) as u32,
        character,
        length: token.text.encode_utf16().count() as u32,
        declaration: true,
        modification: false,
    }
}

fn function_symbol_occurrence(
    source: &str,
    token: &Token,
    function: &FunctionInfo,
    declaration: bool,
) -> SymbolOccurrence {
    let line_text = source.split('\n').nth(token.line - 1).unwrap_or("");
    let character = line_text
        .trim_end_matches('\r')
        .chars()
        .take(token.column - 1)
        .map(char::len_utf16)
        .sum::<usize>() as u32;
    SymbolOccurrence {
        symbol_id: function.symbol_id.clone(),
        name: function.name.rsplit('.').next().unwrap_or(&function.name).to_string(),
        ty: function.signature.clone(),
        mutable: false,
        parameter: false,
        function: true,
        event_handler: false,
        event_message: false,
        class_symbol: false,
        enum_symbol: false,
        enum_variant: false,
        enum_type_id: None,
        namespace_symbol: false,
        scratch_list: false,
        private_member: function.visibility == Visibility::Private,
        line: (token.line - 1) as u32,
        character,
        length: token.text.encode_utf16().count() as u32,
        declaration,
        modification: false,
    }
}

fn collect_expression_symbols(source: &str, expression: &Expr, output: &mut Vec<SymbolOccurrence>) {
    match &expression.kind {
        ExprKind::Name {
            symbol: Some(symbol),
            value,
        } => {
            if value != "self" {
                output.push(symbol_occurrence(
                    source,
                    &expression.token,
                    symbol,
                    false,
                    false,
                ));
            }
        }
        ExprKind::Unary { operand, .. } => collect_expression_symbols(source, operand, output),
        ExprKind::ObjectInit {
            class_id,
            class_name,
            class_token,
            constructor,
            arguments,
        } => {
            output.push(class_symbol_occurrence(
                source,
                class_token,
                class_name,
                *class_id,
                false,
            ));
            if let Some(function) = constructor {
                output.push(function_symbol_occurrence(
                    source,
                    &expression.token,
                    function,
                    false,
                ));
            }
            for argument in arguments {
                collect_expression_symbols(source, argument, output);
            }
        }
        ExprKind::EnumValue { enum_id, enum_name, variant_name, value } => {
            output.push(enum_variant_occurrence(
                source,
                &expression.token,
                enum_name,
                *enum_id,
                variant_name,
                *value,
                false,
            ));
        }
        ExprKind::Member {
            symbol: Some(symbol),
            ..
        } => output.push(field_symbol_occurrence(
            source,
            &expression.token,
            symbol,
            false,
            false,
        )),
        ExprKind::Member { object, .. } => collect_expression_symbols(source, object, output),
        ExprKind::Call {
            receiver,
            arguments,
            info,
            ..
        } => {
            if let Some(function) = info {
                output.push(function_symbol_occurrence(
                    source,
                    &expression.token,
                    function,
                    false,
                ));
            }
            if let Some(receiver) = receiver {
                collect_expression_symbols(source, receiver, output);
            }
            for argument in arguments {
                collect_expression_symbols(source, argument, output);
            }
        }
        ExprKind::Binary { left, right, .. } => {
            collect_expression_symbols(source, left, output);
            collect_expression_symbols(source, right, output);
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_expression_symbols(source, scrutinee, output);
            for arm in arms {
                if let MatchPattern::Value(pattern) = &arm.pattern {
                    collect_expression_symbols(source, pattern, output);
                }
                match &arm.body {
                    MatchArmBody::Expression(expression) => {
                        collect_expression_symbols(source, expression, output)
                    }
                    MatchArmBody::Block(body) => collect_statement_symbols(source, body, output),
                }
            }
        }
        ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::Text(_)
        | ExprKind::ScratchListInit
        | ExprKind::Name { symbol: None, .. } => {}
    }
}

fn collect_statement_symbols(
    source: &str,
    statements: &[Stmt],
    output: &mut Vec<SymbolOccurrence>,
) {
    for statement in statements {
        match statement {
            Stmt::Use { .. } => {}
            Stmt::Namespace {
                name_token,
                path,
                body,
                ..
            } => {
                output.push(namespace_symbol_occurrence(source, name_token, path));
                collect_statement_symbols(source, body, output);
            }
            Stmt::Enum { name_token, name, id, variants } => {
                output.push(enum_symbol_occurrence(source, name_token, name, *id, true));
                for variant in variants {
                    output.push(enum_variant_occurrence(
                        source,
                        &variant.token,
                        name,
                        *id,
                        &variant.name,
                        variant.value,
                        true,
                    ));
                }
            }
            Stmt::Class {
                name_token,
                name,
                fields,
                methods,
            } => {
                output.push(class_symbol_occurrence(source, name_token, name, 0, true));
                for field in fields {
                    let semantic_id = format!("class:{name}:field:{}", field.name);
                    let symbol = Symbol {
                        source_name: field.name.clone(),
                        scratch_name: field.name.clone(),
                        ty: field.ty,
                        mutable: field.mutable,
                        visibility: field.visibility,
                        id: semantic_id.clone(),
                        semantic_id,
                        argument_id: None,
                        storage: StorageClass::ObjectBinding,
                        initial_value: Value::Null,
                    };
                    output.push(field_symbol_occurrence(
                        source,
                        &field.token,
                        &symbol,
                        true,
                        false,
                    ));
                }
                for method in methods {
                    let Stmt::Function {
                        name_token,
                        parameters,
                        info: Some(function),
                        ..
                    } = method
                    else {
                        continue;
                    };
                    output.push(function_symbol_occurrence(
                        source,
                        name_token,
                        function,
                        true,
                    ));
                    for (parameter, argument) in parameters.iter().zip(&function.arguments) {
                        let symbol = Symbol {
                            source_name: parameter.name.clone(),
                            scratch_name: parameter.name.clone(),
                            ty: parameter.ty,
                            mutable: false,
                            visibility: Visibility::Public,
                            id: argument.id.clone(),
                            semantic_id: format!("{}:param:{}", function.symbol_id, argument.name),
                            argument_id: Some(argument.id.clone()),
                            storage: StorageClass::ProcedureArgument,
                            initial_value: Value::Null,
                        };
                        output.push(symbol_occurrence(
                            source,
                            &parameter.token,
                            &symbol,
                            true,
                            false,
                        ));
                    }
                }
            }
            Stmt::Function {
                name_token,
                parameters,
                body,
                info,
                ..
            } => {
                if let Some(function) = info.as_ref().filter(|function| !function.generated_method) {
                    output.push(function_symbol_occurrence(
                        source, name_token, function, true,
                    ));
                }
                for parameter in parameters {
                    if let Some(symbol) = &parameter.symbol {
                        output.push(symbol_occurrence(
                            source,
                            &parameter.token,
                            symbol,
                            true,
                            false,
                        ));
                    }
                }
                collect_statement_symbols(source, body, output);
            }
            Stmt::Event {
                type_token,
                kind: Some(kind),
                body,
                ..
            } => {
                output.push(event_symbol_occurrence(source, type_token, kind));
                collect_statement_symbols(source, body, output);
            }
            Stmt::Send {
                message_token,
                message,
                ..
            } => output.push(message_reference_occurrence(
                source,
                message_token,
                message,
            )),
            Stmt::Call {
                token,
                receiver,
                arguments,
                info,
                ..
            } => {
                if let Some(function) = info {
                    output.push(function_symbol_occurrence(source, token, function, false));
                }
                if let Some(receiver) = receiver {
                    collect_expression_symbols(source, receiver, output);
                }
                for argument in arguments {
                    collect_expression_symbols(source, argument, output);
                }
            }
            Stmt::Return { value, .. } => collect_expression_symbols(source, value, output),
            Stmt::VarDecl {
                name_token,
                initializer,
                symbol: Some(symbol),
                ..
            } => {
                output.push(symbol_occurrence(source, name_token, symbol, true, false));
                collect_expression_symbols(source, initializer, output);
            }
            Stmt::Assign {
                token,
                value,
                symbol: Some(symbol),
                ..
            } => {
                output.push(symbol_occurrence(source, token, symbol, false, true));
                collect_expression_symbols(source, value, output);
            }
            Stmt::MemberAssign {
                field_token,
                value,
                symbol: Some(symbol),
                ..
            } => {
                output.push(field_symbol_occurrence(
                    source,
                    field_token,
                    symbol,
                    false,
                    true,
                ));
                collect_expression_symbols(source, value, output);
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                collect_expression_symbols(source, condition, output);
                collect_statement_symbols(source, then_body, output);
                collect_statement_symbols(source, else_body, output);
            }
            Stmt::While {
                condition, body, ..
            } => {
                collect_expression_symbols(source, condition, output);
                collect_statement_symbols(source, body, output);
            }
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                collect_expression_symbols(source, scrutinee, output);
                for arm in arms {
                    if let MatchPattern::Value(pattern) = &arm.pattern {
                        collect_expression_symbols(source, pattern, output);
                    }
                    match &arm.body {
                        MatchArmBody::Expression(expression) => {
                            collect_expression_symbols(source, expression, output)
                        }
                        MatchArmBody::Block(body) => {
                            collect_statement_symbols(source, body, output)
                        }
                    }
                }
            }
            _ => {}
        }
    }
}

fn imported_type_path(tokens: &[SourceToken], path: &str) -> Option<String> {
    for (index, token) in tokens.iter().enumerate() {
        if token.text != "use" {
            continue;
        }
        let Some(first) = tokens.get(index + 1).filter(|token| token.kind == SourceTokenKind::Identifier) else {
            continue;
        };
        let mut canonical = first.text.clone();
        let mut cursor = index + 2;
        let mut members = Vec::new();
        while tokens.get(cursor).map(|token| token.text.as_str()) == Some(".") {
            if tokens.get(cursor + 1).map(|token| token.text.as_str()) == Some("{") {
                cursor += 2;
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
            let Some(segment) = tokens.get(cursor + 1).filter(|token| token.kind == SourceTokenKind::Identifier) else {
                break;
            };
            canonical.push('.');
            canonical.push_str(&segment.text);
            cursor += 2;
        }
        let alias = if tokens.get(cursor).map(|token| token.text.as_str()) == Some("as") {
            tokens.get(cursor + 1).map(|token| token.text.clone())
        } else {
            None
        };
        if let Some(alias) = alias {
            if path == alias {
                return Some(canonical);
            }
            if let Some(suffix) = path.strip_prefix(&format!("{alias}.")) {
                return Some(format!("{canonical}.{suffix}"));
            }
        } else if let Some(member) = members.iter().find(|member| member.as_str() == path) {
            return Some(format!("{canonical}.{member}"));
        } else if canonical.rsplit('.').next() == Some(path) {
            return Some(canonical);
        }
    }
    None
}

pub fn analyze_symbols(source: &str) -> Result<Vec<SymbolOccurrence>, String> {
    let mut statements = Parser::new(lex(source)?).parse()?;
    // Keep the bindings resolved before a type error so editor features remain
    // available while the user is correcting an otherwise syntactically valid file.
    let _ = TypeChecker::new(Runtime::Scratch).check(&mut statements);
    let mut occurrences = Vec::new();
    collect_statement_symbols(source, &statements, &mut occurrences);
    let class_names = occurrences
        .iter()
        .filter(|occurrence| occurrence.class_symbol && occurrence.declaration)
        .map(|occurrence| {
            (
                occurrence.name.rsplit('.').next().unwrap_or(&occurrence.name).to_string(),
                occurrence.name.clone(),
            )
        })
        .collect::<Vec<_>>();
    let enum_names = occurrences
        .iter()
        .filter(|occurrence| occurrence.enum_symbol && occurrence.declaration)
        .filter_map(|occurrence| occurrence.ty.strip_prefix("enum ").map(str::to_string))
        .collect::<Vec<_>>();
    let enum_ids_by_name = occurrences
        .iter()
        .filter(|occurrence| occurrence.enum_symbol && occurrence.declaration)
        .filter_map(|occurrence| {
            Some((
                occurrence.ty.strip_prefix("enum ")?.to_string(),
                occurrence.enum_type_id?,
            ))
        })
        .collect::<HashMap<_, _>>();
    for occurrence in &mut occurrences {
        let Some(enum_id) = occurrence.enum_type_id else { continue };
        if occurrence.enum_variant {
            continue;
        }
        if let Some(enum_name) = enum_ids_by_name
            .iter()
            .find_map(|(name, id)| (*id == enum_id).then_some(name))
        {
            occurrence.ty = if occurrence.enum_symbol {
                format!("enum {enum_name}")
            } else {
                enum_name.clone()
            };
        }
    }
    let source_tokens = source_tokens(source)?;
    for (index, token) in source_tokens.iter().enumerate() {
        if token.kind != SourceTokenKind::Identifier {
            continue;
        }
        let mut type_start = index;
        while type_start >= 2
            && source_tokens[type_start - 1].text == "."
            && source_tokens[type_start - 2].kind == SourceTokenKind::Identifier
        {
            type_start -= 2;
        }
        if type_start == 0
            || !matches!(source_tokens[type_start - 1].text.as_str(), ":" | "->")
        {
            continue;
        }
        let type_path = source_tokens[type_start..=index]
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        let type_path = imported_type_path(&source_tokens, &type_path).unwrap_or(type_path);
        let exact = class_names
            .iter()
            .filter(|(_, qualified)| qualified == &type_path)
            .collect::<Vec<_>>();
        let matching = if exact.is_empty() {
            class_names
                .iter()
                .filter(|(leaf, _)| leaf == &token.text)
                .collect::<Vec<_>>()
        } else {
            exact
        };
        if matching.len() != 1 {
            continue;
        }
        let class_name = &matching[0].1;
        let parsed_token = Token::new(
            TokenKind::Identifier,
            token.text.clone(),
            token.line as usize + 1,
            token.start_character as usize + 1,
        );
        let mut occurrence = class_symbol_occurrence(
            source,
            &parsed_token,
            class_name,
            0,
            false,
        );
        occurrence.line = token.line;
        occurrence.character = token.start_character;
        occurrence.length = token.length;
        occurrences.push(occurrence);
    }
    for (index, token) in source_tokens.iter().enumerate() {
        if token.kind != SourceTokenKind::Identifier {
            continue;
        }
        let mut type_start = index;
        while type_start >= 2
            && source_tokens[type_start - 1].text == "."
            && source_tokens[type_start - 2].kind == SourceTokenKind::Identifier
        {
            type_start -= 2;
        }
        if type_start == 0
            || !matches!(source_tokens[type_start - 1].text.as_str(), ":" | "->")
        {
            continue;
        }
        let type_path = source_tokens[type_start..=index]
            .iter()
            .map(|token| token.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        let type_path = imported_type_path(&source_tokens, &type_path).unwrap_or(type_path);
        let exact = enum_names
            .iter()
            .filter(|qualified| *qualified == &type_path)
            .collect::<Vec<_>>();
        let matching = if exact.is_empty() {
            enum_names
                .iter()
                .filter(|qualified| qualified.rsplit('.').next() == Some(token.text.as_str()))
                .collect::<Vec<_>>()
        } else {
            exact
        };
        if matching.len() != 1 {
            continue;
        }
        let enum_name = matching[0].as_str();
        let Some(enum_id) = enum_ids_by_name.get(enum_name).copied() else {
            continue;
        };
        let parsed_token = Token::new(
            TokenKind::Identifier,
            token.text.clone(),
            token.line as usize + 1,
            token.start_character as usize + 1,
        );
        let mut occurrence = enum_symbol_occurrence(source, &parsed_token, enum_name, enum_id, false);
        occurrence.name = token.text.clone();
        occurrence.line = token.line;
        occurrence.character = token.start_character;
        occurrence.length = token.length;
        occurrences.push(occurrence);
    }
    let variant_references = occurrences
        .iter()
        .filter(|occurrence| occurrence.enum_variant && !occurrence.declaration)
        .cloned()
        .collect::<Vec<_>>();
    for variant in variant_references {
        let Some(enum_id) = variant.enum_type_id else {
            continue;
        };
        let Some(enum_name) = enum_ids_by_name
            .iter()
            .find_map(|(name, id)| (*id == enum_id).then_some(name.as_str()))
        else {
            continue;
        };
        let Some(variant_index) = source_tokens.iter().position(|token| {
            token.line == variant.line && token.start_character == variant.character
        }) else {
            continue;
        };
        if variant_index < 2 || source_tokens[variant_index - 1].text != "." {
            continue;
        }
        let type_token = &source_tokens[variant_index - 2];
        if type_token.kind != SourceTokenKind::Identifier {
            continue;
        }
        let parsed_token = Token::new(
            TokenKind::Identifier,
            type_token.text.clone(),
            type_token.line as usize + 1,
            1,
        );
        let mut occurrence = enum_symbol_occurrence(source, &parsed_token, enum_name, enum_id, false);
        occurrence.name = type_token.text.clone();
        occurrence.line = type_token.line;
        occurrence.character = type_token.start_character;
        occurrence.length = type_token.length;
        occurrences.push(occurrence);
    }
    let mut seen = std::collections::HashSet::new();
    occurrences.retain(|occurrence| {
        seen.insert((
            occurrence.symbol_id.clone(),
            occurrence.line,
            occurrence.character,
            occurrence.declaration,
            occurrence.modification,
        ))
    });
    Ok(occurrences)
}

fn make_binary(operator: &str, left: Expr, right: Expr, token: Token) -> Expr {
    Expr {
        kind: ExprKind::Binary {
            operator: operator.to_string(),
            left: Box::new(left),
            right: Box::new(right),
        },
        token,
        ty: Some(Type::Bool),
    }
}

fn make_unary(operator: &str, operand: Expr, token: Token) -> Expr {
    Expr {
        kind: ExprKind::Unary {
            operator: operator.to_string(),
            operand: Box::new(operand),
        },
        token,
        ty: Some(Type::Bool),
    }
}
