impl TypeChecker {
    fn reserve_source_names(&mut self, statements: &[Stmt]) {
        fn collect(statements: &[Stmt], names: &mut std::collections::HashSet<String>) {
            for statement in statements {
                match statement {
                    Stmt::Namespace { body, .. } => collect(body, names),
                    Stmt::Class { methods, .. } => collect(methods, names),
                    Stmt::Function { parameters, body, .. } => {
                        names.extend(parameters.iter().map(|parameter| parameter.name.clone()));
                        collect(body, names);
                    }
                    Stmt::Event { body, .. } => collect(body, names),
                    Stmt::VarDecl { name, .. } => {
                        names.insert(name.clone());
                    }
                    Stmt::If { then_body, else_body, .. } => {
                        collect(then_body, names);
                        collect(else_body, names);
                    }
                    Stmt::While { body, .. } => collect(body, names),
                    _ => {}
                }
            }
        }
        collect(statements, &mut self.reserved_source_names);
    }

    fn specialize_method(
        &mut self,
        template: &FunctionInfo,
        class_id: u32,
        object_id: &str,
        ordinal: u32,
        method_index: usize,
    ) -> FunctionInfo {
        let mut suffix = 1usize;
        let (name, proccode) = loop {
            let base = format!("__rake_method_{class_id}_{ordinal}_{method_index}_{suffix}");
            let mut proccode = base.clone();
            for argument in &template.arguments {
                proccode.push_str(match argument.ty {
                    Type::Bool => " %b",
                    Type::Text | Type::Any | Type::ScratchValue => " %s",
                    Type::Int | Type::Float | Type::Class(_) => " %n",
                    Type::ScratchList => unreachable!("Scratch lists cannot be method parameters"),
                });
            }
            if self.used_proccodes.insert(proccode.clone()) {
                break (base, proccode);
            }
            suffix += 1;
        };
        let mut info = template.clone();
        info.name = name.clone();
        info.proccode = proccode;
        info.arguments = template
            .arguments
            .iter()
            .enumerate()
            .map(|(argument_index, argument)| FunctionArgument {
                name: argument.name.clone(),
                id: format!("am{ordinal:08x}_{method_index:04x}_{argument_index:04x}"),
                ty: argument.ty,
            })
            .collect();
        info.return_symbol = info.return_type.and_then(|ty| {
            (!matches!(ty, Type::Class(_))).then(|| {
                let symbol = self.generated_symbol(format!("__rake_return_{name}"), ty);
                self.symbols.push(symbol.clone());
                symbol
            })
        });
        info.completed_symbol = info.return_type.map(|_| {
            let symbol = self.generated_symbol(format!("__rake_completed_{name}"), Type::Bool);
            self.symbols.push(symbol.clone());
            symbol
        });
        info.owner_class = Some(class_id);
        info.generated_method = true;
        info.receiver_object = Some(object_id.to_string());
        info
    }

    fn register_classes(&mut self, statements: &[Stmt]) -> Result<(), String> {
        let mut next_class_id = 0u32;
        self.register_classes_in(statements, &mut next_class_id)
    }

    fn register_classes_in(
        &mut self,
        statements: &[Stmt],
        next_class_id: &mut u32,
    ) -> Result<(), String> {
        for statement in statements {
            if let Stmt::Namespace { body, .. } = statement {
                self.register_classes_in(body, next_class_id)?;
                continue;
            }
            let Stmt::Class {
                name_token,
                name,
                fields,
                ..
            } = statement
            else {
                continue;
            };
            if self.classes.contains_key(name) {
                return Err(diagnostic(
                    name_token,
                    format!("class {name:?} is already declared"),
                ));
            }
            let mut field_names = std::collections::HashSet::new();
            for field in fields {
                if field.ty == Type::ScratchList {
                    return Err(diagnostic(
                        &field.token,
                        "Scratch lists are named resources and cannot be class fields",
                    ));
                }
                if !field_names.insert(field.name.as_str()) {
                    return Err(diagnostic(
                        &field.token,
                        format!("field {:?} is already declared", field.name),
                    ));
                }
                let Some((initializer_type, _)) = constant_field_value(&field.initializer) else {
                    return Err(diagnostic(
                        &field.initializer.token,
                        "class field defaults must be numeric, boolean, or text literals",
                    ));
                };
                if !Self::assignable(field.ty, initializer_type) {
                    return Err(diagnostic(
                        &field.initializer.token,
                        format!(
                            "cannot initialize {} field {:?} with {}",
                            field.ty.name(),
                            field.name,
                            initializer_type.name()
                        ),
                    ));
                }
            }
            self.classes.insert(
                name.clone(),
                ClassInfo {
                    name: name.clone(),
                    id: *next_class_id,
                    fields: fields.clone(),
                    methods: HashMap::new(),
                },
            );
            *next_class_id += 1;
        }
        Ok(())
    }

    fn register_functions(&mut self, statements: &mut [Stmt]) -> Result<(), String> {
        let mut function_index = 0usize;
        self.register_functions_in(statements, &mut function_index)
    }

    fn register_functions_in(
        &mut self,
        statements: &mut [Stmt],
        function_index: &mut usize,
    ) -> Result<(), String> {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => {
                    self.register_functions_in(body, function_index)?;
                }
                Stmt::Class { name, methods, .. } => {
                    let class_id = self.classes.get(name).unwrap().id;
                    let mut method_names = std::collections::HashSet::new();
                    for method in methods {
                        let Stmt::Function {
                            name_token,
                            name: method_qualified_name,
                            warp,
                            parameters,
                            return_type,
                            info,
                            visibility,
                            method_name,
                            ..
                        } = method
                        else {
                            unreachable!();
                        };
                        let leaf = method_name.as_deref().unwrap_or_default();
                        if !method_names.insert(leaf.to_string()) {
                            return Err(diagnostic(
                                name_token,
                                format!("method {leaf:?} is already declared on class {name:?}"),
                            ));
                        }
                        let function_info = self.register_function_signature(
                            name_token,
                            method_qualified_name,
                            *warp,
                            parameters,
                            return_type,
                            *visibility,
                            Some(class_id),
                            leaf,
                            function_index,
                        )?;
                        *info = Some(function_info.clone());
                        self.classes
                            .get_mut(name)
                            .unwrap()
                            .methods
                            .insert(leaf.to_string(), function_info);
                    }
                }
                Stmt::Function {
                    name_token,
                    name,
                    warp,
                    parameters,
                    return_type,
                    info,
                    visibility,
                    ..
                } => {
                    if self.classes.contains_key(name) {
                        return Err(diagnostic(
                            name_token,
                            format!("function and class names cannot both be {name:?}"),
                        ));
                    }
                    let function_info = self.register_function_signature(
                        name_token,
                        name,
                        *warp,
                        parameters,
                        return_type,
                        *visibility,
                        None,
                        "",
                        function_index,
                    )?;
                    if self.functions.insert(name.clone(), function_info.clone()).is_some() {
                        return Err(diagnostic(
                            name_token,
                            format!("function {name:?} is already declared"),
                        ));
                    }
                    *info = Some(function_info);
                }
                _ => {}
            }
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    fn register_function_signature(
        &mut self,
        name_token: &Token,
        name: &str,
        warp: bool,
        parameters: &[Parameter],
        return_type: &mut Option<Type>,
        visibility: Visibility,
        owner_class: Option<u32>,
        method_name: &str,
        function_index: &mut usize,
    ) -> Result<FunctionInfo, String> {
        if owner_class.is_some() && method_name == "new" && return_type.is_none() {
            *return_type = owner_class.map(Type::Class);
        }
        if *return_type == Some(Type::ScratchList) {
            return Err(diagnostic(
                name_token,
                "Scratch lists are static resources and cannot be returned from functions",
            ));
        }
        if let Some(parameter) = parameters
            .iter()
            .find(|parameter| parameter.ty == Type::ScratchList)
        {
            return Err(diagnostic(
                &parameter.token,
                "Scratch lists are static resources and cannot be function parameters",
            ));
        }
        if let Some(parameter) = parameters
            .iter()
            .find(|parameter| matches!(parameter.ty, Type::Class(_)))
        {
            return Err(diagnostic(
                &parameter.token,
                "class instances cannot be passed as function or method parameters yet",
            ));
        }
        let return_type = *return_type;
        if owner_class.is_none() && matches!(return_type, Some(Type::Class(_))) {
            return Err(diagnostic(
                name_token,
                "only a class new method may return a class instance",
            ));
        }
        if let Some(owner_class) = owner_class {
            if method_name == "new" && return_type != Some(Type::Class(owner_class)) {
                return Err(diagnostic(
                    name_token,
                    "new must return self with the declared class type",
                ));
            }
            if let Some(Type::Class(returned_class)) = return_type {
                if method_name != "new" || returned_class != owner_class {
                    return Err(diagnostic(
                        name_token,
                        "only the new method may return its own class instance",
                    ));
                }
            }
        }
        let arguments = parameters
            .iter()
            .enumerate()
            .map(|(argument_index, parameter)| FunctionArgument {
                name: parameter.name.clone(),
                id: format!("a{:08x}_{argument_index:04x}", *function_index),
                ty: parameter.ty,
            })
            .collect::<Vec<_>>();
        let mut proccode = name.to_string();
        for argument in &arguments {
            proccode.push_str(match argument.ty {
                Type::Bool => " %b",
                Type::Text | Type::Any | Type::ScratchValue => " %s",
                Type::Int | Type::Float | Type::Class(_) => " %n",
                Type::ScratchList => unreachable!("Scratch lists cannot be procedure arguments"),
            });
        }
        let signature_arguments = arguments
            .iter()
            .map(|argument| format!("{}: {}", argument.name, argument.ty.name()))
            .collect::<Vec<_>>()
            .join(", ");
        let return_symbol = return_type.and_then(|ty| {
            (owner_class.is_none() && !matches!(ty, Type::Class(_))).then(|| {
                let symbol = self.generated_symbol(format!("__rake_return_{name}"), ty);
                self.symbols.push(symbol.clone());
                symbol
            })
        });
        let completed_symbol = return_type.filter(|_| owner_class.is_none()).map(|_| {
            let symbol = self.generated_symbol(format!("__rake_completed_{name}"), Type::Bool);
            self.symbols.push(symbol.clone());
            symbol
        });
        let function_keyword = if warp { "warp fn" } else { "fn" };
        let visibility_prefix = if owner_class.is_some() && visibility == Visibility::Private {
            "private "
        } else {
            ""
        };
        let return_type_name = return_type.map(|ty| match ty {
            Type::Class(class_id) => self
                .classes
                .values()
                .find(|class| class.id == class_id)
                .map(|class| class.name.clone())
                .unwrap_or_else(|| ty.name().to_string()),
            _ => ty.name().to_string(),
        });
        let signature = match return_type {
            Some(_) => format!("{visibility_prefix}{function_keyword} {name}({signature_arguments}) -> {}", return_type_name.unwrap()),
            None => format!("{visibility_prefix}{function_keyword} {name}({signature_arguments})"),
        };
        let function_info = FunctionInfo {
            name: name.to_string(),
            warp,
            symbol_id: format!("f{:019x}", *function_index + 1),
            signature,
            proccode,
            arguments,
            return_type,
            return_symbol,
            completed_symbol,
            visibility,
            owner_class,
            generated_method: false,
            receiver_object: None,
        };
        if owner_class.is_none() && !self.used_proccodes.insert(function_info.proccode.clone()) {
            return Err(diagnostic(
                name_token,
                format!("Scratch custom block name {name:?} is already in use"),
            ));
        }
        *function_index += 1;
        Ok(function_info)
    }

    fn check_function_declarations(&mut self, statements: &mut Vec<Stmt>) -> Result<(), String> {
        fn check_globals(checker: &mut TypeChecker, statements: &mut [Stmt]) -> Result<(), String> {
            for statement in statements {
                match statement {
                    Stmt::Namespace { body, .. } => check_globals(checker, body)?,
                    Stmt::Function {
                        name_token,
                        parameters,
                        body,
                        info: Some(info),
                        owner_class: None,
                        ..
                    } => checker.check_function(parameters, body, info, name_token)?,
                    _ => {}
                }
            }
            Ok(())
        }
        fn collect_methods(
            checker: &TypeChecker,
            statements: &[Stmt],
            methods: &mut Vec<(u32, Stmt)>,
        ) {
            for statement in statements {
                match statement {
                    Stmt::Namespace { body, .. } => collect_methods(checker, body, methods),
                    Stmt::Class { name, methods: class_methods, .. } => {
                        let class_id = checker.classes[name].id;
                        methods.extend(class_methods.iter().cloned().map(|method| (class_id, method)));
                    }
                    _ => {}
                }
            }
        }
        check_globals(self, statements)?;
        let mut method_declarations = Vec::new();
        collect_methods(self, statements, &mut method_declarations);
        let mut instances = self
            .objects
            .iter()
            .map(|(object_id, instance)| (object_id.clone(), instance.clone()))
            .collect::<Vec<_>>();
        instances.sort_by_key(|(_, instance)| instance.ordinal);
        let mut generated = Vec::new();
        for (class_id, declaration) in method_declarations {
            let Stmt::Function {
                name_token,
                name,
                warp,
                parameters,
                return_type,
                info: Some(_template),
                visibility,
                owner_class,
                method_name,
                body,
            } = declaration
            else {
                continue;
            };
            for (object_id, instance) in instances.iter().filter(|(_, instance)| instance.class_id == class_id) {
                let Some(specialized) = instance.methods.get(method_name.as_deref().unwrap_or("")) else {
                    continue;
                };
                let mut function = Stmt::Function {
                    name_token: name_token.clone(),
                    name: name.clone(),
                    warp,
                    parameters: parameters.clone(),
                    return_type,
                    body: body.clone(),
                    info: Some(specialized.clone()),
                    visibility,
                    owner_class: owner_class.clone(),
                    method_name: method_name.clone(),
                };
                let Stmt::Function { parameters, body, info: Some(info), .. } = &mut function else {
                    unreachable!();
                };
                let _ = object_id;
                self.check_function(parameters, body, info, &name_token)?;
                generated.push(function);
            }
        }
        statements.extend(generated);
        Ok(())
    }
}
