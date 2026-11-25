struct GeneratedSymbolAllocator {
    next_id: usize,
    next_result: usize,
    used_names: Vec<String>,
}

impl GeneratedSymbolAllocator {
    fn new(symbols: &[Symbol]) -> Self {
        Self {
            next_id: symbols.len(),
            next_result: 0,
            used_names: symbols
                .iter()
                .map(|symbol| symbol.scratch_name.clone())
                .collect(),
        }
    }

    fn result_symbol(&mut self, symbols: &mut Vec<Symbol>, ty: Type) -> Symbol {
        self.next_result += 1;
        let base = format!("__rake_result_{:04}", self.next_result);
        let mut scratch_name = base.clone();
        let mut suffix = 2;
        while self.used_names.contains(&scratch_name) {
            scratch_name = format!("{base}_{suffix}");
            suffix += 1;
        }
        self.used_names.push(scratch_name.clone());
        self.next_id += 1;
        let symbol_id = format!("v{:019x}", self.next_id);
        let symbol = Symbol {
            source_name: base,
            scratch_name,
            ty,
            mutable: false,
            id: symbol_id.clone(),
            semantic_id: symbol_id,
            argument_id: None,
            storage: StorageClass::Target,
            initial_value: json!(0),
        };
        symbols.push(symbol.clone());
        symbol
    }
}

fn lower_function_calls(
    statements: &mut Vec<Stmt>,
    symbols: &mut Vec<Symbol>,
) -> Result<(), String> {
    let mut allocator = GeneratedSymbolAllocator::new(symbols);
    lower_statement_list(statements, symbols, &mut allocator)
}

fn lower_statement_list(
    statements: &mut Vec<Stmt>,
    symbols: &mut Vec<Symbol>,
    allocator: &mut GeneratedSymbolAllocator,
) -> Result<(), String> {
    let source_statements = std::mem::take(statements);
    let mut lowered = Vec::new();
    for mut statement in source_statements {
        let mut before = Vec::new();
        match &mut statement {
            Stmt::Use { .. } | Stmt::Class { .. } => {}
            Stmt::Namespace { body, .. } => {
                lower_statement_list(body, symbols, allocator)?;
            }
            Stmt::Function { body, .. } => {
                lower_statement_list(body, symbols, allocator)?;
            }
            Stmt::Call { arguments, .. } => {
                for argument in arguments {
                    lower_expression(argument, &mut before, symbols, allocator)?;
                }
            }
            Stmt::VarDecl { initializer, .. } => {
                lower_expression(initializer, &mut before, symbols, allocator)?;
            }
            Stmt::Assign { value, .. }
            | Stmt::MemberAssign { value, .. }
            | Stmt::Return { value, .. } => {
                lower_expression(value, &mut before, symbols, allocator)?;
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                lower_expression(condition, &mut before, symbols, allocator)?;
                lower_statement_list(then_body, symbols, allocator)?;
                lower_statement_list(else_body, symbols, allocator)?;
            }
            Stmt::While {
                condition, body, ..
            } => {
                let mut refresh = Vec::new();
                lower_expression(condition, &mut refresh, symbols, allocator)?;
                lower_statement_list(body, symbols, allocator)?;
                body.extend(refresh.iter().cloned());
                before = refresh;
            }
        }
        lowered.extend(before);
        lowered.push(statement);
    }
    *statements = lowered;
    Ok(())
}

fn lower_expression(
    expression: &mut Expr,
    before: &mut Vec<Stmt>,
    symbols: &mut Vec<Symbol>,
    allocator: &mut GeneratedSymbolAllocator,
) -> Result<(), String> {
    match &mut expression.kind {
        ExprKind::Unary { operand, .. } => {
            lower_expression(operand, before, symbols, allocator)?;
        }
        ExprKind::Binary { left, right, .. } => {
            lower_expression(left, before, symbols, allocator)?;
            lower_expression(right, before, symbols, allocator)?;
        }
        ExprKind::Call {
            name,
            arguments,
            info,
            builtin,
        } => {
            for argument in arguments.iter_mut() {
                lower_expression(argument, before, symbols, allocator)?;
            }
            if builtin.is_some() {
                return Ok(());
            }
            let info = info
                .clone()
                .ok_or_else(|| "internal error: unresolved function call".to_string())?;
            let return_symbol = info.return_symbol.clone().ok_or_else(|| {
                "internal error: function call has no return variable".to_string()
            })?;
            let ty = info
                .return_type
                .ok_or_else(|| "internal error: function call has no return type".to_string())?;
            let result_symbol = allocator.result_symbol(symbols, ty);
            let token = expression.token.clone();
            before.push(Stmt::Call {
                token: token.clone(),
                name: name.clone(),
                arguments: arguments.clone(),
                info: Some(info),
                builtin: None,
            });
            let returned_value = Expr {
                kind: ExprKind::Name {
                    value: return_symbol.source_name.clone(),
                    symbol: Some(return_symbol),
                },
                token: token.clone(),
                ty: Some(ty),
            };
            before.push(Stmt::Assign {
                token: token.clone(),
                name: result_symbol.source_name.clone(),
                operator: "=".to_string(),
                value: returned_value,
                symbol: Some(result_symbol.clone()),
            });
            expression.kind = ExprKind::Name {
                value: result_symbol.source_name.clone(),
                symbol: Some(result_symbol),
            };
            expression.ty = Some(ty);
        }
        ExprKind::Member { .. }
        | ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::Text(_)
        | ExprKind::ObjectInit { .. }
        | ExprKind::ScratchListInit
        | ExprKind::Name { .. } => {}
    }
    Ok(())
}
