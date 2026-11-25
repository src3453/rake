impl TypeChecker {
    fn assignable(expected: Type, actual: Type) -> bool {
        expected == Type::Any
            || expected == actual
            || (expected == Type::Float && actual == Type::Int)
            || (expected == Type::ScratchValue
                && matches!(actual, Type::Int | Type::Float | Type::Bool | Type::Text))
    }

    fn check_builtin_arguments(
        &mut self,
        builtin: &BuiltinBlock,
        arguments: &mut [Expr],
        token: &Token,
    ) -> Result<(), String> {
        if arguments.len() != builtin.arguments.len() {
            return Err(diagnostic(
                token,
                format!(
                    "Scratch block {:?} expects {} argument(s), got {}",
                    builtin.name,
                    builtin.arguments.len(),
                    arguments.len()
                ),
            ));
        }
        for (argument, spec) in arguments.iter_mut().zip(&builtin.arguments) {
            let actual = self.check_expr(argument)?;
            let valid = match spec.kind {
                BuiltinInputKind::Boolean => actual == Type::Bool,
                BuiltinInputKind::Color => actual == Type::Text,
                BuiltinInputKind::Value => !matches!(actual, Type::Class(_) | Type::ScratchList),
                BuiltinInputKind::List => {
                    actual == Type::ScratchList
                        && matches!(
                            &argument.kind,
                            ExprKind::Name {
                                symbol: Some(Symbol {
                                    storage: StorageClass::ProjectList,
                                    ..
                                }),
                                ..
                            }
                        )
                }
            };
            if !valid {
                let expected = match spec.kind {
                    BuiltinInputKind::Boolean => "bool",
                    BuiltinInputKind::Color => "text",
                    BuiltinInputKind::Value => "Scratch value",
                    BuiltinInputKind::List => "Scratch.Generics.List resource",
                };
                return Err(diagnostic(
                    &argument.token,
                    format!(
                        "cannot pass {} to {expected} input {:?} of {:?}",
                        actual.name(),
                        spec.name,
                        builtin.name
                    ),
                ));
            }
        }
        Ok(())
    }

    fn check_expr(&mut self, expression: &mut Expr) -> Result<Type, String> {
        let call_token = expression.token.clone();
        if let ExprKind::Call {
            name,
            arguments,
            builtin,
            ..
        } = &mut expression.kind
        {
            if let Some((list_method, receiver)) = self.list_method_call(name, &call_token) {
                arguments.insert(0, receiver);
                *name = list_method.name.to_string();
                *builtin = Some(list_method);
            }
        }
        let resolved_call = match &expression.kind {
            ExprKind::Call { name, .. } => Some(self.resolve_name(name)),
            _ => None,
        };
        if resolved_call.as_deref() == Some("Scratch.Generics.List.new") {
            let ExprKind::Call { arguments, .. } = &expression.kind else {
                unreachable!();
            };
            if !arguments.is_empty() {
                return Err(diagnostic(
                    &expression.token,
                    "Scratch.Generics.List.new() takes no arguments",
                ));
            }
            expression.kind = ExprKind::ScratchListInit;
            expression.ty = Some(Type::ScratchList);
            return Ok(Type::ScratchList);
        }
        if let (Some(resolved_name), ExprKind::Call { arguments, .. }) =
            (resolved_call.as_ref(), &expression.kind)
        {
            if let Some(class) = self.classes.get(resolved_name) {
                if !arguments.is_empty() {
                    return Err(diagnostic(
                        &expression.token,
                        format!("class {:?} has no constructor parameters yet", class.name),
                    ));
                }
                let class_id = class.id;
                let class_name = class.name.clone();
                expression.kind = ExprKind::ObjectInit {
                    class_id,
                    class_name,
                };
                expression.ty = Some(Type::Class(class_id));
                return Ok(Type::Class(class_id));
            }
        }
        let ty = match &mut expression.kind {
            ExprKind::Number(value) => {
                if value.contains('.') {
                    Type::Float
                } else {
                    Type::Int
                }
            }
            ExprKind::Boolean(_) => Type::Bool,
            ExprKind::Text(_) => Type::Text,
            ExprKind::ObjectInit { class_id, .. } => Type::Class(*class_id),
            ExprKind::ScratchListInit => Type::ScratchList,
            ExprKind::Name { value, symbol } => {
                let resolved = self.lookup(value, &expression.token)?;
                *symbol = Some(resolved.clone());
                resolved.ty
            }
            ExprKind::Member {
                object,
                name,
                symbol,
            } => {
                let object_type = self.check_expr(object)?;
                let Type::Class(class_id) = object_type else {
                    return Err(diagnostic(
                        &expression.token,
                        "field access requires a class instance",
                    ));
                };
                let ExprKind::Name {
                    symbol: Some(object_symbol),
                    ..
                } = &object.kind
                else {
                    return Err(diagnostic(
                        &expression.token,
                        "field access currently requires a named class instance",
                    ));
                };
                let instance = self.objects.get(&object_symbol.id).ok_or_else(|| {
                    diagnostic(
                        &expression.token,
                        "field access requires a statically declared class instance",
                    )
                })?;
                if instance.class_id != class_id {
                    return Err("internal error: class instance type mismatch".to_string());
                }
                let field = instance.fields.get(name).cloned().ok_or_else(|| {
                    diagnostic(&expression.token, format!("class has no field {name:?}"))
                })?;
                *symbol = Some(field.clone());
                field.ty
            }
            ExprKind::Call {
                name,
                arguments,
                info,
                builtin,
            } => {
                let resolved_name = resolved_call.unwrap_or_else(|| name.clone());
                *name = resolved_name.clone();
                if let Some(block) = builtin_block(&resolved_name) {
                    let Some(result_type) = block.result else {
                        return Err(diagnostic(
                            &expression.token,
                            format!("Scratch command {:?} does not return a value", block.name),
                        ));
                    };
                    self.check_builtin_arguments(&block, arguments, &expression.token)?;
                    *builtin = Some(block);
                    expression.ty = Some(result_type);
                    return Ok(result_type);
                }
                let signature = self.functions.get(&resolved_name).cloned().ok_or_else(|| {
                    diagnostic(&expression.token, format!("unknown function {name:?}"))
                })?;
                let Some(return_type) = signature.return_type else {
                    return Err(diagnostic(
                        &expression.token,
                        format!("function {name:?} does not return a value"),
                    ));
                };
                if arguments.len() != signature.arguments.len() {
                    return Err(diagnostic(
                        &expression.token,
                        format!(
                            "function {resolved_name:?} expects {} argument(s), got {}",
                            signature.arguments.len(),
                            arguments.len()
                        ),
                    ));
                }
                for (argument, parameter) in arguments.iter_mut().zip(&signature.arguments) {
                    let actual = self.check_expr(argument)?;
                    if !Self::assignable(parameter.ty, actual) {
                        return Err(diagnostic(
                            &argument.token,
                            format!(
                                "cannot pass {} to {} parameter {:?} of function {resolved_name:?}",
                                actual.name(),
                                parameter.ty.name(),
                                parameter.name
                            ),
                        ));
                    }
                }
                *info = Some(signature);
                return_type
            }
            ExprKind::Unary { operator, operand } => {
                let operand_type = self.check_expr(operand)?;
                match operator.as_str() {
                    "!" if operand_type == Type::Bool => Type::Bool,
                    "!" => {
                        return Err(diagnostic(&expression.token, "'!' requires a bool operand"));
                    }
                    "-" if operand_type.numeric() => operand_type,
                    _ => return Err(diagnostic(&expression.token, "unary '-' requires a number")),
                }
            }
            ExprKind::Binary {
                operator,
                left,
                right,
            } => {
                let left_type = self.check_expr(left)?;
                let right_type = self.check_expr(right)?;
                match operator.as_str() {
                    "+" | "-" | "*" | "/" if left_type.numeric() && right_type.numeric() => {
                        if operator == "/" || left_type == Type::Float || right_type == Type::Float
                        {
                            Type::Float
                        } else {
                            Type::Int
                        }
                    }
                    "+" | "-" | "*" | "/" => {
                        return Err(diagnostic(
                            &expression.token,
                            format!("{operator:?} requires numeric operands"),
                        ));
                    }
                    "<" | "<=" | ">" | ">=" if left_type.numeric() && right_type.numeric() => {
                        Type::Bool
                    }
                    "<" | "<=" | ">" | ">=" => {
                        return Err(diagnostic(
                            &expression.token,
                            format!("{operator:?} requires numeric operands"),
                        ));
                    }
                    "==" | "!=" if matches!(left_type, Type::Class(_)) => {
                        return Err(diagnostic(
                            &expression.token,
                            "class instance identity comparisons are not implemented yet",
                        ));
                    }
                    "==" | "!="
                        if left_type == right_type
                            || left_type == Type::ScratchValue
                            || right_type == Type::ScratchValue
                            || matches!(
                                (left_type, right_type),
                                (Type::Int, Type::Float) | (Type::Float, Type::Int)
                            ) =>
                    {
                        Type::Bool
                    }
                    "==" | "!=" => {
                        return Err(diagnostic(
                            &expression.token,
                            "equality operands must have compatible types",
                        ));
                    }
                    "&&" | "||" if left_type == Type::Bool && right_type == Type::Bool => {
                        Type::Bool
                    }
                    "&&" | "||" => {
                        return Err(diagnostic(
                            &expression.token,
                            format!("{operator:?} requires bool operands"),
                        ));
                    }
                    _ => {
                        return Err(diagnostic(
                            &expression.token,
                            format!("unsupported operator {operator:?}"),
                        ));
                    }
                }
            }
        };
        expression.ty = Some(ty);
        Ok(ty)
    }
}
