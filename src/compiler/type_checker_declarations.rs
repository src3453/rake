impl TypeChecker {
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
            if let Stmt::Namespace { body, .. } = statement {
                self.register_functions_in(body, function_index)?;
                continue;
            }
            let Stmt::Function {
                name_token,
                name,
                parameters,
                return_type,
                info,
                ..
            } = statement
            else {
                continue;
            };
            if self.classes.contains_key(name) {
                return Err(diagnostic(
                    name_token,
                    format!("function and class names cannot both be {name:?}"),
                ));
            }
            let return_type = *return_type;
            if return_type == Some(Type::ScratchList) {
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
            if self.functions.contains_key(name) {
                return Err(diagnostic(
                    name_token,
                    format!("function {name:?} is already declared"),
                ));
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
            let mut proccode = name.clone();
            for argument in &arguments {
                proccode.push_str(match argument.ty {
                    Type::Bool => " %b",
                    Type::Text | Type::Any | Type::ScratchValue => " %s",
                    Type::Int | Type::Float | Type::Class(_) => " %n",
                    Type::ScratchList => {
                        unreachable!("Scratch lists cannot be procedure arguments")
                    }
                });
            }
            let signature_arguments = arguments
                .iter()
                .map(|argument| format!("{}: {}", argument.name, argument.ty.name()))
                .collect::<Vec<_>>()
                .join(", ");
            let return_symbol = return_type.map(|_| {
                let symbol =
                    self.generated_symbol(format!("__rake_return_{name}"), return_type.unwrap());
                self.symbols.push(symbol.clone());
                symbol
            });
            let completed_symbol = return_type.map(|_| {
                let symbol = self.generated_symbol(format!("__rake_completed_{name}"), Type::Bool);
                self.symbols.push(symbol.clone());
                symbol
            });
            let signature = match return_type {
                Some(ty) => format!("fn {name}({signature_arguments}) -> {}", ty.name()),
                None => format!("fn {name}({signature_arguments})"),
            };
            let function_info = FunctionInfo {
                name: name.clone(),
                symbol_id: format!("f{:019x}", *function_index + 1),
                signature,
                proccode,
                arguments,
                return_type,
                return_symbol,
                completed_symbol,
            };
            *info = Some(function_info.clone());
            self.functions.insert(name.clone(), function_info);
            *function_index += 1;
        }
        Ok(())
    }

    fn check_function_declarations(&mut self, statements: &mut [Stmt]) -> Result<(), String> {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => self.check_function_declarations(body)?,
                Stmt::Function {
                    name_token,
                    parameters,
                    body,
                    info: Some(info),
                    ..
                } => {
                    self.check_function(parameters, body, info, name_token)?;
                }
                _ => {}
            }
        }
        Ok(())
    }
}
