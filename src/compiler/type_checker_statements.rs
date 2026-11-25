impl TypeChecker {
    fn event_text_value(
        type_token: &Token,
        event_type: &str,
        values: &[Expr],
    ) -> Result<String, String> {
        if values.len() != 1 {
            return Err(diagnostic(
                type_token,
                format!(
                    "event {event_type} expects 1 string value, got {}",
                    values.len()
                ),
            ));
        }
        match &values[0].kind {
            ExprKind::Text(value) => Ok(value.clone()),
            _ => Err(diagnostic(
                &values[0].token,
                format!("event {event_type} value must be a string literal"),
            )),
        }
    }

    fn event_kind(
        type_token: &Token,
        event_type: &str,
        resolved_event_type: &str,
        values: &[Expr],
        has_parentheses: bool,
    ) -> Result<EventKind, String> {
        match resolved_event_type {
            "Scratch.Events.greenFlag" if values.is_empty() && !has_parentheses => {
                Ok(EventKind::GreenFlag)
            }
            "Scratch.Events.greenFlag" => Err(diagnostic(
                type_token,
                "event Scratch.Events.greenFlag takes no arguments or parentheses",
            )),
            "Scratch.Events.keyPressed" if has_parentheses => {
                Ok(EventKind::KeyPressed(Self::event_text_value(
                type_token,
                "Scratch.Events.keyPressed",
                values,
                )?))
            }
            "Scratch.Events.keyPressed" => Err(diagnostic(
                type_token,
                "event Scratch.Events.keyPressed expects a string selector in parentheses",
            )),
            "Scratch.Events.backdropSwitchedTo" if has_parentheses => {
                Ok(EventKind::BackdropSwitchedTo(Self::event_text_value(
                    type_token,
                    "Scratch.Events.backdropSwitchedTo",
                    values,
                )?))
            }
            "Scratch.Events.backdropSwitchedTo" => Err(diagnostic(
                type_token,
                "event Scratch.Events.backdropSwitchedTo expects a string selector in parentheses",
            )),
            "Scratch.Events.spriteClicked" if values.is_empty() && !has_parentheses => {
                Ok(EventKind::SpriteClicked)
            }
            "Scratch.Events.spriteClicked" => Err(diagnostic(
                type_token,
                "event Scratch.Events.spriteClicked takes no arguments or parentheses",
            )),
            "Scratch.Events.cloneStart" if values.is_empty() && !has_parentheses => {
                Ok(EventKind::CloneStart)
            }
            "Scratch.Events.cloneStart" => Err(diagnostic(
                type_token,
                "event Scratch.Events.cloneStart takes no arguments or parentheses",
            )),
            _ if !has_parentheses && values.is_empty() && !event_type.contains('.') => {
                Ok(EventKind::Message(event_type.to_string()))
            }
            _ => Err(diagnostic(
                type_token,
                format!("unknown Scratch event {event_type:?}; custom messages use `event name {{ ... }}`"),
            )),
        }
    }

    fn check_function(
        &mut self,
        parameters: &mut [Parameter],
        body: &mut [Stmt],
        info: &FunctionInfo,
        name_token: &Token,
    ) -> Result<(), String> {
        self.scopes.push(HashMap::new());
        let previous_method_class = self.current_method_class;
        if let Some(object_id) = &info.receiver_object {
            let Some(instance) = self.objects.get(object_id) else {
                self.scopes.pop();
                return Err("internal error: method receiver instance is missing".to_string());
            };
            if parameters.iter().any(|parameter| parameter.name == "self") {
                self.scopes.pop();
                return Err(diagnostic(name_token, "method parameter name 'self' is reserved"));
            }
            self.scopes
                .last_mut()
                .unwrap()
                .insert("self".to_string(), instance.binding.clone());
            self.current_method_class = info.owner_class;
        }
        for (parameter, argument) in parameters.iter_mut().zip(&info.arguments) {
            if self.scopes.last().unwrap().contains_key(&parameter.name) {
                self.scopes.pop();
                self.current_method_class = previous_method_class;
                return Err(diagnostic(
                    &parameter.token,
                    format!("parameter {:?} is already declared", parameter.name),
                ));
            }
            let symbol = Symbol {
                source_name: parameter.name.clone(),
                scratch_name: parameter.name.clone(),
                ty: parameter.ty,
                mutable: false,
                visibility: Visibility::Public,
                id: argument.id.clone(),
                argument_id: Some(argument.id.clone()),
                storage: StorageClass::ProcedureArgument,
                initial_value: Value::Null,
                semantic_id: if info.owner_class.is_some() {
                    format!("{}:param:{}", info.symbol_id, argument.name)
                } else {
                    argument.id.clone()
                },
            };
            self.scopes
                .last_mut()
                .unwrap()
                .insert(parameter.name.clone(), symbol.clone());
            parameter.symbol = Some(symbol);
        }
        let previous_function = self.current_function.replace(info.clone());
        let result = self.check_statements(body, false);
        self.current_function = previous_function;
        self.current_method_class = previous_method_class;
        self.scopes.pop();
        result?;
        if info.return_type.is_some() && !statements_return_on_all_paths(body) {
            return Err(diagnostic(
                name_token,
                format!("function {:?} must return a value on every path", info.name),
            ));
        }
        Ok(())
    }

    fn generated_symbol(&mut self, name: String, ty: Type) -> Symbol {
        let mut scratch_name = name.clone();
        let mut suffix = 2;
        while self.scratch_names.contains(&scratch_name)
            || self.reserved_source_names.contains(&scratch_name)
        {
            scratch_name = format!("{name}_{suffix}");
            suffix += 1;
        }
        self.scratch_names.push(scratch_name.clone());
        self.next_id += 1;
        let id = format!("v{:019x}", self.next_id);
        Symbol {
            source_name: name,
            scratch_name,
            ty,
            mutable: true,
            visibility: Visibility::Public,
            id: id.clone(),
            semantic_id: id,
            argument_id: None,
            storage: StorageClass::Target,
            initial_value: json!(0),
        }
    }

    fn check_statements(&mut self, statements: &mut [Stmt], scoped: bool) -> Result<(), String> {
        if scoped {
            self.scopes.push(HashMap::new());
        }
        let result = statements
            .iter_mut()
            .try_for_each(|statement| self.check_statement(statement));
        if scoped {
            self.scopes.pop();
        }
        result
    }

    fn lookup(&self, name: &str, token: &Token) -> Result<Symbol, String> {
        self.scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).cloned())
            .ok_or_else(|| diagnostic(token, format!("unknown variable {name:?}")))
    }

    fn list_method_call(&self, name: &str, token: &Token) -> Option<(BuiltinBlock, Expr)> {
        let (receiver_name, member) = name.rsplit_once('.')?;
        let symbol = self
            .scopes
            .iter()
            .rev()
            .find_map(|scope| scope.get(receiver_name))?;
        if symbol.ty != Type::ScratchList {
            return None;
        }
        let builtin = builtin_block(&format!("Scratch.Generics.List.{member}"))?;
        let receiver_column = token
            .column
            .saturating_sub(receiver_name.chars().count() + 1);
        let mut receiver_token = Token::new(
            TokenKind::Identifier,
            receiver_name,
            token.line,
            receiver_column,
        );
        receiver_token.source_length = receiver_name.encode_utf16().count();
        Some((
            builtin,
            Expr {
                kind: ExprKind::Name {
                    value: receiver_name.to_string(),
                    symbol: Some(symbol.clone()),
                },
                token: receiver_token,
                ty: Some(Type::ScratchList),
            },
        ))
    }

    fn check_statement(&mut self, statement: &mut Stmt) -> Result<(), String> {
        match statement {
            Stmt::Use { .. } | Stmt::Namespace { .. } | Stmt::Enum { .. } => Ok(()),
            Stmt::Class { .. } => Ok(()),
            Stmt::Function { .. } => Ok(()),
            Stmt::Event {
                type_token,
                event_type,
                values,
                has_parentheses,
                kind,
                body,
                ..
            } => {
                let resolved_event_type = self
                    .import_for(event_type, "")
                    .unwrap_or_else(|| event_type.clone());
                *kind = Some(Self::event_kind(
                    type_token,
                    event_type,
                    &resolved_event_type,
                    values,
                    *has_parentheses,
                )?);
                self.check_statements(body, true)
            }
            Stmt::Send { .. } => Ok(()),
            Stmt::Return { token, value } => {
                let Some(function) = self.current_function.clone() else {
                    return Err(diagnostic(token, "return is only valid inside a function"));
                };
                let Some(return_type) = function.return_type else {
                    return Err(diagnostic(
                        token,
                        format!("function {:?} does not return a value", function.name),
                    ));
                };
                let value_type = self.check_expr(value)?;
                if !Self::assignable(return_type, value_type) {
                    return Err(diagnostic(
                        &value.token,
                        format!(
                            "cannot return {} from a function returning {}",
                            self.readable_type_name(value_type),
                            self.readable_type_name(return_type)
                        ),
                    ));
                }
                Ok(())
            }
            Stmt::Call {
                token,
                name,
                receiver,
                arguments,
                info,
                builtin,
            } => {
                if let Some(receiver_expression) = receiver.as_deref_mut() {
                    let method_name = name.rsplit('.').next().unwrap_or(name).to_string();
                    if let Some(method) = self.resolve_instance_method(
                        receiver_expression,
                        &method_name,
                        token,
                    )? {
                        self.check_method_arguments(arguments, &method, token)?;
                        *name = method.name.clone();
                        *info = Some(method);
                        return Ok(());
                    }
                }
                if let Some((list_method, receiver)) = self.list_method_call(name, token) {
                    arguments.insert(0, receiver);
                    *name = list_method.name.to_string();
                    *builtin = Some(list_method);
                }
                let resolved_name = self.resolve_name(name);
                if let Some(block) = builtin_block(&resolved_name) {
                    if block.result.is_some() {
                        return Err(diagnostic(
                            token,
                            format!(
                                "Scratch block {:?} returns a value and must be used as an expression",
                                block.name
                            ),
                        ));
                    }
                    self.check_builtin_arguments(&block, arguments, token)?;
                    *name = resolved_name;
                    *builtin = Some(block);
                    return Ok(());
                }
                let signature = self
                    .functions
                    .get(&resolved_name)
                    .cloned()
                    .ok_or_else(|| diagnostic(token, format!("unknown function {name:?}")))?;
                if arguments.len() != signature.arguments.len() {
                    return Err(diagnostic(
                        token,
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
                *name = resolved_name;
                *info = Some(signature);
                Ok(())
            }
            Stmt::VarDecl {
                token,
                name_token,
                name,
                mutable,
                annotation,
                initializer,
                symbol,
                object_fields: object_field_symbols,
            } => {
                if name == "self" {
                    return Err(diagnostic(name_token, "'self' is a reserved name"));
                }
                if self.scopes.last().unwrap().contains_key(name) {
                    return Err(diagnostic(
                        token,
                        format!("{name:?} is already declared in this scope"),
                    ));
                }
                let initializer_type = self.check_expr(initializer)?;
                if initializer_type == Type::ScratchList {
                    if !matches!(&initializer.kind, ExprKind::ScratchListInit) {
                        return Err(diagnostic(
                            &initializer.token,
                            "Scratch list handles can only be created with Scratch.Generics.List.new()",
                        ));
                    }
                    if annotation.is_some_and(|ty| ty != Type::ScratchList) {
                        return Err(diagnostic(
                            name_token,
                            "Scratch.Generics.List.new() requires a Scratch.Generics.List variable",
                        ));
                    }
                    if self.current_function.is_some() || self.scopes.len() != 1 {
                        return Err(diagnostic(
                            name_token,
                            "Scratch lists must be created in a module-scope variable declaration",
                        ));
                    }
                    if self.scratch_names.contains(name) {
                        return Err(diagnostic(
                            name_token,
                            format!("Scratch resource name {name:?} is already in use"),
                        ));
                    }
                    self.scratch_names.push(name.clone());
                    self.next_id += 1;
                    let id = format!("l{:019x}", self.next_id);
                    let list = Symbol {
                        source_name: name.clone(),
                        scratch_name: name.clone(),
                        ty: Type::ScratchList,
                        mutable: *mutable,
                        visibility: Visibility::Public,
                        id: id.clone(),
                        semantic_id: id,
                        argument_id: None,
                        storage: StorageClass::ProjectList,
                        initial_value: json!([]),
                    };
                    self.scopes
                        .last_mut()
                        .unwrap()
                        .insert(name.clone(), list.clone());
                    *symbol = Some(list.clone());
                    self.symbols.push(list);
                    return Ok(());
                }
                if annotation == &Some(Type::ScratchList) {
                    return Err(diagnostic(
                        name_token,
                        "Scratch.Generics.List values must be initialized with Scratch.Generics.List.new()",
                    ));
                }
                if let Type::Class(class_id) = initializer_type {
                    let (initialized_class, constructor_template, _constructor_arguments) =
                        match &initializer.kind {
                            ExprKind::ObjectInit {
                                class_id: initialized,
                                constructor,
                                arguments,
                                ..
                            } if *initialized == class_id => {
                                (*initialized, constructor.clone(), arguments.clone())
                            }
                            _ => {
                                return Err(diagnostic(
                                    &initializer.token,
                                    "class instances must be initialized with ClassName.new() or a direct ClassName() constructor",
                                ));
                            }
                        };
                    if initialized_class != class_id {
                        return Err(diagnostic(
                            &initializer.token,
                            "class constructor type does not match its declaration",
                        ));
                    }
                    if annotation.is_some_and(|declared| declared != initializer_type) {
                        return Err(diagnostic(
                            name_token,
                            format!(
                                "cannot initialize {} with {}",
                                self.readable_type_name(annotation.unwrap()),
                                self.readable_type_name(initializer_type)
                            ),
                        ));
                    }
                    if self.current_function.is_some() || self.scopes.len() != 1 {
                        return Err(diagnostic(
                            token,
                            "class instances must be declared at module scope in this version",
                        ));
                    }
                    let class = self
                        .classes
                        .values()
                        .find(|class| class.id == class_id)
                        .cloned()
                        .ok_or_else(|| "internal error: unknown class instance type".to_string())?;
                    let mut scratch_name = name.clone();
                    let mut suffix = 2;
                    while self.scratch_names.contains(&scratch_name) {
                        scratch_name = format!("{name}_{suffix}");
                        suffix += 1;
                    }
                    self.scratch_names.push(scratch_name.clone());
                    let object_symbol = Symbol {
                        source_name: name.clone(),
                        scratch_name,
                        ty: initializer_type,
                        mutable: *mutable,
                        visibility: Visibility::Public,
                        id: format!("o:{:08x}", self.next_object_ordinal),
                        semantic_id: format!("o:{:08x}", self.next_object_ordinal),
                        argument_id: None,
                        storage: StorageClass::ObjectBinding,
                        initial_value: Value::Null,
                    };
                    let ordinal = self.next_object_ordinal;
                    self.next_object_ordinal += 1;
                    let mut object_fields = HashMap::new();
                    let mut instance_fields = Vec::new();
                    let mut private_fields = std::collections::HashSet::new();
                    for field in &class.fields {
                        let class_scope = class.name.rsplit_once('.').map(|(scope, _)| scope).unwrap_or("");
                        let (_, initial_value) = constant_field_value(&field.initializer)
                            .or_else(|| self.enum_variant_constant(&field.initializer, class_scope))
                            .expect("class field defaults validated during registration");
                        let base_name = format!("{name}_{}", field.name);
                        let mut field_scratch_name = base_name.clone();
                        let mut suffix = 2;
                        while self.scratch_names.contains(&field_scratch_name)
                            || self.reserved_source_names.contains(&field_scratch_name)
                        {
                            field_scratch_name = format!("{base_name}_{suffix}");
                            suffix += 1;
                        }
                        self.scratch_names.push(field_scratch_name.clone());
                        self.next_id += 1;
                        let field_id = format!("v{:019x}", self.next_id);
                        let field_symbol = Symbol {
                            source_name: format!("{name}.{}", field.name),
                            scratch_name: field_scratch_name,
                            ty: field.ty,
                            mutable: field.mutable,
                            visibility: field.visibility,
                            id: field_id,
                            semantic_id: format!("class:{}:field:{}", class.name, field.name),
                            argument_id: None,
                            storage: StorageClass::Project,
                            initial_value,
                        };
                        self.symbols.push(field_symbol.clone());
                        instance_fields.push(field_symbol.clone());
                        object_fields.insert(field.name.clone(), field_symbol);
                        if field.visibility == Visibility::Private {
                            private_fields.insert(field.name.clone());
                        }
                    }
                    let mut methods = HashMap::new();
                    let mut method_names = class.methods.keys().cloned().collect::<Vec<_>>();
                    method_names.sort();
                    for (method_index, method_name) in method_names.iter().enumerate() {
                        let template = class.methods.get(method_name).unwrap();
                        let method = self.specialize_method(
                            template,
                            class_id,
                            &object_symbol.id,
                            ordinal,
                            method_index,
                        );
                        methods.insert(method_name.clone(), method);
                    }
                    self.scopes
                        .last_mut()
                        .unwrap()
                        .insert(name.clone(), object_symbol.clone());
                    self.objects.insert(
                        object_symbol.id.clone(),
                        ObjectInstance {
                            class_id,
                            ordinal,
                            binding: object_symbol.clone(),
                            fields: object_fields,
                            private_fields,
                            methods,
                        },
                    );
                    if let ExprKind::ObjectInit { constructor, .. } = &mut initializer.kind {
                        *constructor = constructor_template.map(|_| {
                            self.objects[&object_symbol.id].methods["new"].clone()
                        });
                    }
                    *symbol = Some(object_symbol);
                    *object_field_symbols = instance_fields;
                    return Ok(());
                }
                let declared_type = annotation.unwrap_or(initializer_type);
                if !Self::assignable(declared_type, initializer_type) {
                    return Err(diagnostic(
                        &initializer.token,
                        format!(
                            "cannot initialize {} with {}",
                            self.readable_type_name(declared_type),
                            self.readable_type_name(initializer_type)
                        ),
                    ));
                }
                let mut scratch_name = name.clone();
                let mut suffix = 2;
                while self.scratch_names.contains(&scratch_name) {
                    scratch_name = format!("{name}_{suffix}");
                    suffix += 1;
                }
                self.scratch_names.push(scratch_name.clone());
                self.next_id += 1;
                let symbol_id = format!("v{:019x}", self.next_id);
                let semantic_id = self
                    .current_function
                    .as_ref()
                    .filter(|function| function.generated_method)
                    .map(|function| {
                        format!(
                            "{}:local:{}:{}:{}",
                            function.symbol_id, name, name_token.line, name_token.column
                        )
                    })
                    .unwrap_or_else(|| symbol_id.clone());
                let value = Symbol {
                    source_name: name.clone(),
                    scratch_name,
                    ty: declared_type,
                    mutable: *mutable,
                    visibility: Visibility::Public,
                    id: symbol_id.clone(),
                    semantic_id,
                    argument_id: None,
                    storage: if self.current_function.is_none() && self.scopes.len() == 1 {
                        StorageClass::Project
                    } else {
                        StorageClass::Target
                    },
                    initial_value: json!(0),
                };
                self.scopes
                    .last_mut()
                    .unwrap()
                    .insert(name.clone(), value.clone());
                *symbol = Some(value.clone());
                self.symbols.push(value);
                Ok(())
            }
            Stmt::Assign {
                token,
                name,
                operator,
                value,
                symbol,
            } => {
                let resolved = self.lookup(name, token)?;
                if resolved.ty == Type::ScratchList {
                    return Err(diagnostic(
                        token,
                        "Scratch list handles cannot be reassigned; use Scratch.Generics.List operations",
                    ));
                }
                if matches!(resolved.ty, Type::Class(_)) {
                    return Err(diagnostic(
                        token,
                        "class instance references cannot be reassigned yet",
                    ));
                }
                if !resolved.mutable {
                    return Err(diagnostic(
                        token,
                        format!("cannot assign to const {name:?}"),
                    ));
                }
                let mut value_type = self.check_expr(value)?;
                if operator != "=" {
                    if !resolved.ty.numeric() || !value_type.numeric() {
                        return Err(diagnostic(
                            token,
                            format!("{operator} requires numeric values"),
                        ));
                    }
                    if resolved.ty == Type::Float || value_type == Type::Float {
                        value_type = Type::Float;
                    }
                }
                if !Self::assignable(resolved.ty, value_type) {
                    return Err(diagnostic(
                        &value.token,
                        format!(
                            "cannot assign {} to {} variable {name:?}",
                            self.readable_type_name(value_type),
                            self.readable_type_name(resolved.ty)
                        ),
                    ));
                }
                *symbol = Some(resolved);
                Ok(())
            }
            Stmt::MemberAssign {
                token,
                field_token,
                object,
                field,
                operator,
                value,
                symbol,
                ..
            } => {
                let object_symbol = self.lookup(object, token)?;
                let Type::Class(class_id) = object_symbol.ty else {
                    return Err(diagnostic(
                        token,
                        "field assignment requires a class instance",
                    ));
                };
                let instance = self.objects.get(&object_symbol.id).ok_or_else(|| {
                    diagnostic(
                        token,
                        "field assignment requires a statically declared instance",
                    )
                })?;
                if instance.class_id != class_id {
                    return Err("internal error: class instance type mismatch".to_string());
                }
                if instance.private_fields.contains(field)
                    && self.current_method_class != Some(class_id)
                {
                    return Err(diagnostic(
                        &field_token,
                        format!("field {field:?} is private"),
                    ));
                }
                let resolved =
                    instance.fields.get(field).cloned().ok_or_else(|| {
                        diagnostic(token, format!("class has no field {field:?}"))
                    })?;
                if !resolved.mutable {
                    return Err(diagnostic(
                        token,
                        format!("cannot assign to const field {field:?}"),
                    ));
                }
                let mut value_type = self.check_expr(value)?;
                if operator != "=" {
                    if !resolved.ty.numeric() || !value_type.numeric() {
                        return Err(diagnostic(
                            token,
                            format!("{operator} requires numeric values"),
                        ));
                    }
                    if resolved.ty == Type::Float || value_type == Type::Float {
                        value_type = Type::Float;
                    }
                }
                if !Self::assignable(resolved.ty, value_type) {
                    return Err(diagnostic(
                        &value.token,
                        format!(
                            "cannot assign {} to {} field {:?}",
                            self.readable_type_name(value_type),
                            self.readable_type_name(resolved.ty),
                            field
                        ),
                    ));
                }
                *symbol = Some(resolved);
                Ok(())
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                if self.check_expr(condition)? != Type::Bool {
                    return Err(diagnostic(
                        &condition.token,
                        "if condition must have type bool",
                    ));
                }
                self.check_statements(then_body, true)?;
                self.check_statements(else_body, true)
            }
            Stmt::While {
                condition, body, ..
            } => {
                if self.check_expr(condition)? != Type::Bool {
                    return Err(diagnostic(
                        &condition.token,
                        "while condition must have type bool",
                    ));
                }
                self.check_statements(body, true)
            }
        }
    }
}
