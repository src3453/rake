fn statements_return_on_all_paths(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| match statement {
        Stmt::Return { .. } => true,
        Stmt::If {
            then_body,
            else_body,
            ..
        } => {
            !else_body.is_empty()
                && statements_return_on_all_paths(then_body)
                && statements_return_on_all_paths(else_body)
        }
        Stmt::Match { arms, .. } => {
            !arms.is_empty()
                && arms.iter().all(|arm| match &arm.body {
                    MatchArmBody::Block(body) => statements_return_on_all_paths(body),
                    MatchArmBody::Expression(_) => false,
                })
        }
        _ => false,
    })
}

fn collect_expression_calls(expression: &Expr, calls: &mut Vec<String>) {
    match &expression.kind {
        ExprKind::Call {
            name, arguments, ..
        } => {
            calls.push(name.clone());
            for argument in arguments {
                collect_expression_calls(argument, calls);
            }
        }
        ExprKind::Unary { operand, .. } | ExprKind::Convert { operand, .. } => {
            collect_expression_calls(operand, calls)
        }
        ExprKind::Member { object, .. } => collect_expression_calls(object, calls),
        ExprKind::Binary { left, right, .. }
        | ExprKind::Index {
            object: left,
            index: right,
        } => {
            collect_expression_calls(left, calls);
            collect_expression_calls(right, calls);
        }
        ExprKind::Match { scrutinee, arms } => {
            collect_expression_calls(scrutinee, calls);
            for arm in arms {
                if let MatchPattern::Value(pattern) = &arm.pattern {
                    collect_expression_calls(pattern, calls);
                }
                match &arm.body {
                    MatchArmBody::Expression(expression) => {
                        collect_expression_calls(expression, calls)
                    }
                    MatchArmBody::Block(body) => collect_statement_calls(body, calls),
                }
            }
        }
        ExprKind::Number(_)
        | ExprKind::Boolean(_)
        | ExprKind::Text(_)
        | ExprKind::ObjectInit { .. }
        | ExprKind::EnumValue { .. }
        | ExprKind::ScratchListInit
        | ExprKind::Name { .. } => {}
    }
}

fn collect_statement_calls(statements: &[Stmt], calls: &mut Vec<String>) {
    for statement in statements {
        match statement {
            Stmt::Use { .. } | Stmt::Send { .. } => {}
            Stmt::Namespace { body, .. } => collect_statement_calls(body, calls),
            Stmt::Class { .. } | Stmt::Enum { .. } => {}
            Stmt::Function { body, .. } => collect_statement_calls(body, calls),
            Stmt::Event { body, .. } => collect_statement_calls(body, calls),
            Stmt::Call {
                name, arguments, ..
            } => {
                calls.push(name.clone());
                for argument in arguments {
                    collect_expression_calls(argument, calls);
                }
            }
            Stmt::Return { value, .. } => collect_expression_calls(value, calls),
            Stmt::VarDecl { initializer, .. } => collect_expression_calls(initializer, calls),
            Stmt::Assign { value, .. } | Stmt::MemberAssign { value, .. } => {
                collect_expression_calls(value, calls)
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
            } => {
                collect_expression_calls(condition, calls);
                collect_statement_calls(then_body, calls);
                collect_statement_calls(else_body, calls);
            }
            Stmt::While {
                condition, body, ..
            } => {
                collect_expression_calls(condition, calls);
                collect_statement_calls(body, calls);
            }
            Stmt::Match {
                scrutinee, arms, ..
            } => {
                collect_expression_calls(scrutinee, calls);
                for arm in arms {
                    if let MatchPattern::Value(pattern) = &arm.pattern {
                        collect_expression_calls(pattern, calls);
                    }
                    match &arm.body {
                        MatchArmBody::Expression(expression) => {
                            collect_expression_calls(expression, calls)
                        }
                        MatchArmBody::Block(body) => collect_statement_calls(body, calls),
                    }
                }
            }
        }
    }
}

fn reaches_function(
    current: &str,
    target: &str,
    graph: &HashMap<String, Vec<String>>,
    visited: &mut Vec<String>,
) -> bool {
    let Some(callees) = graph.get(current) else {
        return false;
    };
    for callee in callees {
        if callee == target {
            return true;
        }
        if !visited.contains(callee) {
            visited.push(callee.clone());
            if reaches_function(callee, target, graph, visited) {
                return true;
            }
        }
    }
    false
}

fn validate_recursive_return_functions(statements: &[Stmt]) -> Result<(), String> {
    let mut graph = HashMap::new();
    fn collect<'a>(
        statements: &'a [Stmt],
        graph: &mut HashMap<String, Vec<String>>,
        returned: &mut Vec<(&'a Token, String)>,
    ) {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => collect(body, graph, returned),
                Stmt::Function {
                    name_token,
                    name,
                    body,
                    return_type,
                    ..
                } => {
                    let mut calls = Vec::new();
                    collect_statement_calls(body, &mut calls);
                    graph.insert(name.clone(), calls);
                    if return_type.is_some() {
                        returned.push((name_token, name.clone()));
                    }
                }
                _ => {}
            }
        }
    }
    let mut returned = Vec::new();
    collect(statements, &mut graph, &mut returned);
    for (name_token, name) in returned {
        if reaches_function(&name, &name, &graph, &mut vec![name.clone()]) {
            return Err(diagnostic(
                name_token,
                format!(
                    "recursive calls involving function {name:?} are not supported when it returns a value"
                ),
            ));
        }
    }
    Ok(())
}
