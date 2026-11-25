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
        class_symbol: false,
        namespace_symbol: false,
        scratch_list: symbol.ty == Type::ScratchList,
        line: (token.line - 1) as u32,
        character,
        length: token.text.encode_utf16().count() as u32,
        declaration,
        modification,
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
        class_symbol: false,
        namespace_symbol: true,
        scratch_list: false,
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
        name: function.name.clone(),
        ty: function.signature.clone(),
        mutable: false,
        parameter: false,
        function: true,
        class_symbol: false,
        namespace_symbol: false,
        scratch_list: false,
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
            ..
        } => output.push(symbol_occurrence(
            source,
            &expression.token,
            symbol,
            false,
            false,
        )),
        ExprKind::Unary { operand, .. } => collect_expression_symbols(source, operand, output),
        ExprKind::ObjectInit {
            class_id,
            class_name,
        } => output.push(class_symbol_occurrence(
            source,
            &expression.token,
            class_name,
            *class_id,
            false,
        )),
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
            arguments, info, ..
        } => {
            if let Some(function) = info {
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
        ExprKind::Binary { left, right, .. } => {
            collect_expression_symbols(source, left, output);
            collect_expression_symbols(source, right, output);
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
            Stmt::Class {
                name_token,
                name,
                fields,
            } => {
                output.push(class_symbol_occurrence(source, name_token, name, 0, true));
                for field in fields {
                    let semantic_id = format!("class:{name}:field:{}", field.name);
                    let symbol = Symbol {
                        source_name: field.name.clone(),
                        scratch_name: field.name.clone(),
                        ty: field.ty,
                        mutable: field.mutable,
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
            }
            Stmt::Function {
                name_token,
                parameters,
                body,
                info,
                ..
            } => {
                if let Some(function) = info {
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
            Stmt::Call {
                token,
                arguments,
                info,
                ..
            } => {
                if let Some(function) = info {
                    output.push(function_symbol_occurrence(source, token, function, false));
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
            _ => {}
        }
    }
}

pub fn analyze_symbols(source: &str) -> Result<Vec<SymbolOccurrence>, String> {
    let mut statements = Parser::new(lex(source)?).parse()?;
    // Keep the bindings resolved before a type error so editor features remain
    // available while the user is correcting an otherwise syntactically valid file.
    let _ = TypeChecker::new().check(&mut statements);
    let mut occurrences = Vec::new();
    collect_statement_symbols(source, &statements, &mut occurrences);
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
