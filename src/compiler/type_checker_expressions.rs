impl TypeChecker {
    fn match_constant_key(expression: &Expr) -> Option<String> {
        match &expression.kind {
            ExprKind::Number(value) => {
                let number = value.parse::<f64>().ok()?;
                Some(format!("number:{:016x}", if number == 0.0 { 0 } else { number.to_bits() }))
            }
            ExprKind::Boolean(value) => Some(format!("bool:{value}")),
            ExprKind::Text(value) => Some(format!("text:{value:?}")),
            ExprKind::EnumValue {
                enum_id, value, ..
            } => Some(format!("enum:{enum_id}:{value}")),
            ExprKind::Unary { operator, operand } if operator == "-" => {
                let ExprKind::Number(value) = &operand.kind else {
                    return None;
                };
                let number = -value.parse::<f64>().ok()?;
                Some(format!("number:{:016x}", if number == 0.0 { 0 } else { number.to_bits() }))
            }
            _ => None,
        }
    }

    fn match_statements_diverge(statements: &[Stmt]) -> bool {
        statements.iter().any(|statement| match statement {
            Stmt::Return { .. } => true,
            Stmt::If {
                then_body,
                else_body,
                ..
            } => {
                !else_body.is_empty()
                    && Self::match_statements_diverge(then_body)
                    && Self::match_statements_diverge(else_body)
            }
            Stmt::Match { arms, .. } => {
                !arms.is_empty()
                    && arms.iter().all(|arm| match &arm.body {
                        MatchArmBody::Block(body) => Self::match_statements_diverge(body),
                        MatchArmBody::Expression(_) => false,
                    })
            }
            _ => false,
        })
    }

    fn merge_match_types(current: Type, next: Type) -> Option<Type> {
        if current == next {
            Some(current)
        } else if current.numeric() && next.numeric() {
            Some(Type::Float)
        } else if current == Type::Any {
            Some(next)
        } else if next == Type::Any {
            Some(current)
        } else if current == Type::ScratchValue
            && matches!(next, Type::Int | Type::Float | Type::Bool | Type::Text)
        {
            Some(Type::ScratchValue)
        } else if next == Type::ScratchValue
            && matches!(current, Type::Int | Type::Float | Type::Bool | Type::Text)
        {
            Some(Type::ScratchValue)
        } else {
            None
        }
    }

    fn check_match(
        &mut self,
        scrutinee: &mut Expr,
        arms: &mut [MatchArm],
        expression_context: bool,
    ) -> Result<Type, String> {
        if arms.is_empty() {
            return Err(diagnostic(&scrutinee.token, "match must contain at least one arm"));
        }
        let scrutinee_type = self.check_expr(scrutinee)?;
        if matches!(scrutinee_type, Type::Class(_) | Type::ScratchList) {
            return Err(diagnostic(
                &scrutinee.token,
                "match requires an enum or scalar value",
            ));
        }

        let mut wildcard_index = None;
        let mut pattern_keys = std::collections::HashSet::new();
        let mut enum_values = std::collections::HashSet::new();
        let mut bool_values = std::collections::HashSet::new();
        let mut result_type = None;

        for (index, arm) in arms.iter_mut().enumerate() {
            match &mut arm.pattern {
                MatchPattern::Wildcard(token) => {
                    if wildcard_index.replace(index).is_some() {
                        return Err(diagnostic(token, "a match can contain only one wildcard arm"));
                    }
                }
                MatchPattern::Value(pattern) => {
                    if wildcard_index.is_some() {
                        return Err(diagnostic(
                            &arm.token,
                            "wildcard match arm must be the last arm",
                        ));
                    }
                    let pattern_type = self.check_expr(pattern)?;
                    let compatible = pattern_type == scrutinee_type
                        || (scrutinee_type.numeric() && pattern_type.numeric())
                        || (scrutinee_type == Type::ScratchValue
                            && matches!(pattern_type, Type::Int | Type::Float | Type::Bool | Type::Text))
                        || scrutinee_type == Type::Any;
                    if !compatible {
                        return Err(diagnostic(
                            &pattern.token,
                            format!(
                                "match pattern has type {}, but the matched value has type {}",
                                self.readable_type_name(pattern_type),
                                self.readable_type_name(scrutinee_type)
                            ),
                        ));
                    }
                    let Some(key) = Self::match_constant_key(pattern) else {
                        return Err(diagnostic(
                            &pattern.token,
                            "match patterns must be literals or enum variants",
                        ));
                    };
                    if !pattern_keys.insert(key) {
                        return Err(diagnostic(&pattern.token, "duplicate match pattern"));
                    }
                    match (&pattern.kind, scrutinee_type) {
                        (ExprKind::EnumValue { enum_id, value, .. }, Type::Enum(_)) => {
                            enum_values.insert((*enum_id, *value));
                        }
                        (ExprKind::Boolean(value), Type::Bool) => {
                            bool_values.insert(*value);
                        }
                        _ => {}
                    }
                }
            }

            match &mut arm.body {
                MatchArmBody::Expression(expression) if expression_context => {
                    let arm_type = self.check_expr(expression)?;
                    if matches!(arm_type, Type::Class(_) | Type::ScratchList) {
                        return Err(diagnostic(
                            &expression.token,
                            "match arms must produce scalar or enum values",
                        ));
                    }
                    result_type = Some(match result_type {
                        None => arm_type,
                        Some(current) => Self::merge_match_types(current, arm_type).ok_or_else(|| {
                            diagnostic(
                                &expression.token,
                                format!(
                                    "match arm type {} does not match previous arm type {}",
                                    self.readable_type_name(arm_type),
                                    self.readable_type_name(current)
                                ),
                            )
                        })?,
                    });
                }
                MatchArmBody::Expression(expression) => {
                    if let ExprKind::Call {
                        name,
                        receiver,
                        arguments,
                        ..
                    } = &expression.kind
                    {
                        let statement = Stmt::Call {
                            token: expression.token.clone(),
                            name: name.clone(),
                            receiver: receiver.clone(),
                            arguments: arguments.clone(),
                            info: None,
                            builtin: None,
                        };
                        let mut statements = vec![statement];
                        self.check_statements(&mut statements, true)?;
                        arm.body = MatchArmBody::Block(statements);
                    } else {
                        self.check_expr(expression)?;
                    }
                }
                MatchArmBody::Block(body) => {
                    self.check_statements(body, true)?;
                    if expression_context {
                        if !Self::match_statements_diverge(body) {
                            return Err(diagnostic(
                                &arm.token,
                                "a match block arm used as a value must end by returning from its function",
                            ));
                        }
                    }
                }
            }
        }

        if wildcard_index.is_none() {
            let exhaustive = match scrutinee_type {
                Type::Enum(enum_id) => self
                    .enums
                    .values()
                    .find(|enumeration| enumeration.id == enum_id)
                    .is_some_and(|enumeration| {
                        enumeration
                            .variants
                            .values()
                            .all(|variant| enum_values.contains(&(enum_id, variant.value)))
                    }),
                Type::Bool => bool_values.contains(&true) && bool_values.contains(&false),
                _ => false,
            };
            if !exhaustive {
                return Err(diagnostic(
                    &scrutinee.token,
                    "match is not exhaustive; add a wildcard arm `_ => ...`",
                ));
            }
        }

        Ok(if expression_context {
            result_type.unwrap_or(Type::Any)
        } else {
            Type::Any
        })
    }

    fn check_method_arguments(
        &mut self,
        arguments: &mut [Expr],
        signature: &FunctionInfo,
        token: &Token,
    ) -> Result<(), String> {
        if arguments.len() != signature.arguments.len() {
            return Err(diagnostic(
                token,
                format!(
                    "method {:?} expects {} argument(s), got {}",
                    signature.name,
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
                        "cannot pass {} to {} parameter {:?} of method {:?}",
                        self.readable_type_name(actual),
                        self.readable_type_name(parameter.ty),
                        parameter.name,
                        signature.name
                    ),
                ));
            }
        }
        Ok(())
    }

    fn resolve_instance_method(
        &mut self,
        receiver: &mut Expr,
        method_name: &str,
        token: &Token,
    ) -> Result<Option<FunctionInfo>, String> {
        let receiver_name = match &receiver.kind {
            ExprKind::Name { value, .. } => value.clone(),
            _ => return Ok(None),
        };
        let Some(symbol) = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(&receiver_name).cloned())
        else {
            return Ok(None);
        };
        let Type::Class(class_id) = symbol.ty else {
            return Ok(None);
        };
        self.check_expr(receiver)?;
        let object_symbol = match &receiver.kind {
            ExprKind::Name {
                symbol: Some(symbol), ..
            } => symbol,
            _ => unreachable!("checked method receiver must remain a name"),
        };
        let instance = self.objects.get(&object_symbol.id).ok_or_else(|| {
            diagnostic(token, "method calls require a statically declared class instance")
        })?;
        if instance.class_id != class_id {
            return Err("internal error: class instance type mismatch".to_string());
        }
        let method = instance.methods.get(method_name).cloned().ok_or_else(|| {
            diagnostic(token, format!("class has no method {method_name:?}"))
        })?;
        if method.return_type == Some(Type::Class(class_id)) {
            return Err(diagnostic(
                token,
                "the new method must be called as ClassName.new() when declaring an instance",
            ));
        }
        if method.visibility == Visibility::Private
            && self.current_method_class != Some(class_id)
        {
            return Err(diagnostic(
                token,
                format!("method {method_name:?} is private"),
            ));
        }
        Ok(Some(method))
    }

    fn check_class_constructor_call(&mut self, expression: &mut Expr) -> Result<bool, String> {
        let (receiver_name, class_token, method_name, call_token) = match &expression.kind {
            ExprKind::Call {
                name,
                receiver: Some(receiver),
                ..
            } => {
                let Some(value) = qualified_expression_name(receiver) else {
                    return Ok(false);
                };
                if matches!(&receiver.kind, ExprKind::Name { .. })
                    && self.scopes.iter().rev().any(|scope| scope.contains_key(&value))
                {
                    return Ok(false);
                }
                (
                    value,
                    receiver.token.clone(),
                    name.rsplit('.').next().unwrap_or(name).to_string(),
                    expression.token.clone(),
                )
            }
            _ => return Ok(false),
        };
        let class_name = self.resolve_name(&receiver_name);
        let Some(class) = self.classes.get(&class_name).cloned() else {
            return Ok(false);
        };
        if method_name != "new" {
            return Err(diagnostic(
                &call_token,
                format!("class {:?} has no associated method {method_name:?}", class.name),
            ));
        }
        let constructor = class.methods.get("new").cloned().ok_or_else(|| {
            diagnostic(&call_token, format!("class {:?} has no constructor method", class.name))
        })?;
        if constructor.visibility == Visibility::Private
            && self.current_method_class != Some(class.id)
        {
            return Err(diagnostic(&call_token, "constructor method new is private"));
        }
        let ExprKind::Call { arguments, .. } = &mut expression.kind else {
            unreachable!();
        };
        self.check_method_arguments(arguments, &constructor, &call_token)?;
        if constructor.return_type != Some(Type::Class(class.id)) {
            return Err(diagnostic(
                &call_token,
                "new must return self with the declared class type",
            ));
        }
        let arguments = arguments.clone();
        expression.kind = ExprKind::ObjectInit {
            class_id: class.id,
            class_name: class.name,
            class_token,
            constructor: Some(constructor),
            arguments,
        };
        expression.ty = Some(Type::Class(class.id));
        Ok(true)
    }

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
                BuiltinInputKind::Note => matches!(actual, Type::Int | Type::Float),
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
                    BuiltinInputKind::Note => "a numeric MIDI note",
                    BuiltinInputKind::Value => "Scratch value",
                    BuiltinInputKind::List => "Scratch.Generics.List resource",
                };
                return Err(diagnostic(
                    &argument.token,
                    format!(
                        "cannot pass {} to {expected} input {:?} of {:?}",
                        self.readable_type_name(actual),
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
        if let ExprKind::Member { object, name, .. } = &expression.kind {
            if let Some(enum_path) = qualified_expression_name(object) {
                let resolved = self.resolve_name(&enum_path);
                if let Some(enumeration) = self.enums.get(&resolved).cloned() {
                    let Some(variant) = enumeration.variants.get(name) else {
                        return Err(diagnostic(
                            &expression.token,
                            format!("enum {:?} has no variant {name:?}", enumeration.name),
                        ));
                    };
                    expression.kind = ExprKind::EnumValue {
                        enum_id: enumeration.id,
                        enum_name: enumeration.name,
                        variant_name: variant.name.clone(),
                        value: variant.value,
                    };
                    expression.ty = Some(Type::Enum(enumeration.id));
                    return Ok(Type::Enum(enumeration.id));
                }
            }
        }
        if self.check_class_constructor_call(expression)? {
            return Ok(expression.ty.unwrap());
        }
        let method_call = if let ExprKind::Call {
            name,
            receiver: Some(receiver),
            builtin: None,
            ..
        } = &mut expression.kind
        {
            self.resolve_instance_method(
                receiver,
                name.rsplit('.').next().unwrap_or(name),
                &call_token,
            )?
        } else {
            None
        };
        if let Some(method) = method_call {
            let ExprKind::Call { name, arguments, info, .. } = &mut expression.kind else {
                unreachable!();
            };
            self.check_method_arguments(arguments, &method, &call_token)?;
            let return_type = method.return_type.ok_or_else(|| {
                diagnostic(&call_token, format!("method {:?} does not return a value", method.name))
            })?;
            *name = method.name.clone();
            *info = Some(method);
            expression.ty = Some(return_type);
            return Ok(return_type);
        }
        if let ExprKind::Call {
            name,
            receiver,
            arguments,
            builtin,
            ..
        } = &mut expression.kind
        {
            if let Some((list_method, list_receiver)) = self.list_method_call(name, &call_token) {
                arguments.insert(0, list_receiver);
                *name = list_method.name.to_string();
                *builtin = Some(list_method);
                *receiver = None;
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
                if class.methods.contains_key("new") {
                    return Err(diagnostic(
                        &expression.token,
                        "classes with a new method must be initialized with ClassName.new()",
                    ));
                }
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
                    class_token: expression.token.clone(),
                    constructor: None,
                    arguments: Vec::new(),
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
            ExprKind::EnumValue { enum_id, .. } => Type::Enum(*enum_id),
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
                if instance.private_fields.contains(name)
                    && self.current_method_class != Some(class_id)
                {
                    return Err(diagnostic(
                        &expression.token,
                        format!("field {name:?} is private"),
                    ));
                }
                *symbol = Some(field.clone());
                field.ty
            }
            ExprKind::Call {
                name,
                receiver: _,
                arguments,
                info,
                builtin,
            } => {
                let resolved_name = resolved_call.unwrap_or_else(|| name.clone());
                *name = resolved_name.clone();
                if let Some(block) = builtin_block(&resolved_name) {
                    self.check_builtin_runtime(&block, &expression.token)?;
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
                                self.readable_type_name(actual),
                                self.readable_type_name(parameter.ty),
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
            ExprKind::Match { scrutinee, arms } => {
                return self.check_match(scrutinee, arms, true).map(|ty| {
                    expression.ty = Some(ty);
                    ty
                });
            }
        };
        expression.ty = Some(ty);
        Ok(ty)
    }
}
