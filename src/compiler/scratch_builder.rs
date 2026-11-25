struct ScratchBuilder {
    blocks: Map<String, Value>,
    broadcasts: Map<String, Value>,
    next_id: usize,
}

impl ScratchBuilder {
    fn new() -> Self {
        Self {
            blocks: Map::new(),
            broadcasts: Map::new(),
            next_id: 0,
        }
    }
    fn next_id(&mut self) -> String {
        self.next_id += 1;
        format!("b{:019x}", self.next_id)
    }

    fn broadcast_id(&mut self, name: &str) -> String {
        if let Some((id, _)) = self
            .broadcasts
            .iter()
            .find(|(_, value)| value.as_str() == Some(name))
        {
            return id.clone();
        }
        let id = format!("broadcast_{:019x}", self.broadcasts.len() + 1);
        self.broadcasts.insert(id.clone(), json!(name));
        id
    }

    fn block(
        &mut self,
        opcode: &str,
        parent: Option<&str>,
        top_level: bool,
        fields: Value,
    ) -> String {
        let id = self.next_id();
        let mut block = json!({
            "opcode": opcode, "next": null, "parent": parent, "inputs": {}, "fields": fields,
            "shadow": false, "topLevel": top_level,
        });
        if top_level {
            block["x"] = json!(24);
            block["y"] = json!(24);
        }
        self.blocks.insert(id.clone(), block);
        id
    }

    fn input(&mut self, block_id: &str, name: &str, value: Value) {
        self.blocks.get_mut(block_id).unwrap()["inputs"][name] = value;
    }

    fn emit_builtin(
        &mut self,
        builtin: &BuiltinBlock,
        arguments: &[Expr],
        parent: &str,
    ) -> Result<String, String> {
        let mut fields = builtin
            .fields
            .iter()
            .map(|(name, value)| (name.to_string(), json!([value, null])))
            .collect::<Map<String, Value>>();
        let id = self.block(
            builtin.opcode,
            Some(parent),
            false,
            Value::Object(Map::new()),
        );
        for (argument, spec) in arguments.iter().zip(&builtin.arguments) {
            if spec.kind == BuiltinInputKind::List {
                let ExprKind::Name {
                    symbol: Some(list), ..
                } = &argument.kind
                else {
                    return Err("internal error: unresolved Scratch list argument".to_string());
                };
                if list.storage != StorageClass::ProjectList {
                    return Err(
                        "internal error: Scratch list argument is not a project list".to_string(),
                    );
                }
                fields.insert("LIST".to_string(), json!([list.scratch_name, list.id]));
                continue;
            }
            let input = if let Some(menu) = &spec.menu {
                let selection = if let ExprKind::Text(value) = &argument.kind {
                    value.as_str()
                } else {
                    menu.default
                };
                let field_value = if menu.opcode == "event_broadcast_menu" {
                    json!([selection, self.broadcast_id(selection)])
                } else {
                    json!([selection, null])
                };
                let mut menu_fields = Map::new();
                menu_fields.insert(menu.field.to_string(), field_value);
                let menu_id = self.block(menu.opcode, Some(&id), false, Value::Object(menu_fields));
                self.blocks.get_mut(&menu_id).unwrap()["shadow"] = json!(true);
                if matches!(&argument.kind, ExprKind::Text(_)) {
                    json!([1, menu_id])
                } else {
                    let reporter = self.reporter(argument, &id)?;
                    json!([3, reporter, menu_id])
                }
            } else {
                match spec.kind {
                    BuiltinInputKind::Boolean => self.boolean_input(argument, &id)?,
                    BuiltinInputKind::Color => {
                        if let ExprKind::Text(value) = &argument.kind {
                            json!([1, [9, value]])
                        } else {
                            let reporter = self.reporter(argument, &id)?;
                            json!([3, reporter, [9, "#000000"]])
                        }
                    }
                    BuiltinInputKind::Value => self.value_input(argument, &id)?,
                    BuiltinInputKind::List => unreachable!("handled as a Scratch LIST field"),
                }
            };
            self.input(&id, spec.input, input);
        }
        self.blocks.get_mut(&id).unwrap()["fields"] = Value::Object(fields);
        Ok(id)
    }

    fn sequence(
        &mut self,
        statements: &[Stmt],
        parent: &str,
        function: Option<&FunctionInfo>,
    ) -> Result<Option<String>, String> {
        let mut first = None;
        let mut previous: Option<String> = None;
        for statement in statements {
            if matches!(
                statement,
                Stmt::Function { .. }
                    | Stmt::Class { .. }
                    | Stmt::Namespace { .. }
                    | Stmt::Use { .. }
            ) {
                continue;
            }
            if matches!(statement, Stmt::VarDecl { symbol: Some(symbol), .. }
                if symbol.storage == StorageClass::ProjectList)
            {
                continue;
            }
            if matches!(statement, Stmt::VarDecl { symbol: Some(symbol), object_fields, .. }
                if symbol.storage == StorageClass::ObjectBinding && object_fields.is_empty())
            {
                continue;
            }
            let statement_parent = previous.as_deref().unwrap_or(parent);
            let current = if let Some(function) = function.filter(|info| info.return_type.is_some())
            {
                let completion = function.completed_symbol.as_ref().ok_or_else(|| {
                    "internal error: missing function completion variable".to_string()
                })?;
                let guard = self.block("control_if", Some(statement_parent), false, json!({}));
                let completed = Expr {
                    kind: ExprKind::Name {
                        value: completion.source_name.clone(),
                        symbol: Some(completion.clone()),
                    },
                    token: Token::new(TokenKind::Identifier, "return", 1, 1),
                    ty: Some(Type::Bool),
                };
                let not_completed =
                    make_unary("!", completed, Token::new(TokenKind::Symbol, "!", 1, 1));
                let condition = self.boolean_input(&not_completed, &guard)?;
                self.input(&guard, "CONDITION", condition);
                let inner = self.statement(statement, &guard, Some(function))?;
                self.input(&guard, "SUBSTACK", json!([2, inner]));
                guard
            } else {
                self.statement(statement, statement_parent, function)?
            };
            if let Some(previous_id) = previous.as_ref() {
                self.blocks.get_mut(previous_id).unwrap()["next"] = json!(current);
            }
            if first.is_none() {
                first = Some(current.clone());
            }
            previous = Some(current);
        }
        Ok(first)
    }

    fn procedure_mutation(info: &FunctionInfo) -> Value {
        let argument_ids = info
            .arguments
            .iter()
            .map(|argument| argument.id.clone())
            .collect::<Vec<_>>();
        let argument_names = info
            .arguments
            .iter()
            .map(|argument| argument.name.clone())
            .collect::<Vec<_>>();
        let argument_defaults = info
            .arguments
            .iter()
            .map(|argument| match argument.ty {
                Type::Bool => "false",
                Type::Text | Type::Any | Type::ScratchValue => "",
                Type::Int | Type::Float | Type::Class(_) => "0",
                Type::ScratchList => unreachable!("Scratch lists cannot be procedure arguments"),
            })
            .collect::<Vec<_>>();
        json!({
            "tagName": "mutation",
            "children": [],
            "proccode": info.proccode,
            "argumentids": serde_json::to_string(&argument_ids).unwrap(),
            "argumentnames": serde_json::to_string(&argument_names).unwrap(),
            "argumentdefaults": serde_json::to_string(&argument_defaults).unwrap(),
            "warp": "false"
        })
    }

    fn procedure_definition(&mut self, function: &Stmt) -> Result<(), String> {
        let Stmt::Function {
            body,
            info: Some(info),
            ..
        } = function
        else {
            return Err("internal error: unresolved function signature".to_string());
        };
        let definition = self.block("procedures_definition", None, true, json!({}));
        let prototype = self.block("procedures_prototype", Some(&definition), false, json!({}));
        self.blocks.get_mut(&prototype).unwrap()["shadow"] = json!(true);
        self.blocks.get_mut(&prototype).unwrap()["mutation"] = Self::procedure_mutation(info);
        self.input(&definition, "custom_block", json!([1, prototype]));
        for argument in &info.arguments {
            let opcode = if argument.ty == Type::Bool {
                "argument_reporter_boolean"
            } else {
                "argument_reporter_string_number"
            };
            let reporter = self.block(
                opcode,
                Some(&prototype),
                false,
                json!({"VALUE": [argument.name.clone(), null]}),
            );
            self.input(&prototype, &argument.id, json!([1, reporter]));
        }
        let first = if let Some(completion) = &info.completed_symbol {
            let initialize = self.block(
                "data_setvariableto",
                Some(&definition),
                false,
                json!({"VARIABLE": [completion.scratch_name, completion.id]}),
            );
            let value = Expr {
                kind: ExprKind::Boolean(false),
                token: Token::new(TokenKind::Identifier, "false", 1, 1),
                ty: Some(Type::Bool),
            };
            let input = self.value_input(&value, &initialize)?;
            self.input(&initialize, "VALUE", input);
            let first = self.sequence(body, &initialize, Some(info))?;
            self.blocks.get_mut(&initialize).unwrap()["next"] = json!(first);
            Some(initialize)
        } else {
            self.sequence(body, &definition, Some(info))?
        };
        self.blocks.get_mut(&definition).unwrap()["next"] = json!(first);
        Ok(())
    }

    fn statement(
        &mut self,
        statement: &Stmt,
        parent: &str,
        function: Option<&FunctionInfo>,
    ) -> Result<String, String> {
        match statement {
            Stmt::Return { value, .. } => {
                let function = function.ok_or_else(|| {
                    "internal error: return statement outside a function".to_string()
                })?;
                let return_symbol = function.return_symbol.as_ref().ok_or_else(|| {
                    "internal error: return statement in a function without a result".to_string()
                })?;
                let completion = function.completed_symbol.as_ref().ok_or_else(|| {
                    "internal error: missing function completion variable".to_string()
                })?;
                let assign = self.block(
                    "data_setvariableto",
                    Some(parent),
                    false,
                    json!({"VARIABLE": [return_symbol.scratch_name, return_symbol.id]}),
                );
                let input = self.value_input(value, &assign)?;
                self.input(&assign, "VALUE", input);
                let finish = self.block(
                    "data_setvariableto",
                    Some(&assign),
                    false,
                    json!({"VARIABLE": [completion.scratch_name, completion.id]}),
                );
                let value = Expr {
                    kind: ExprKind::Boolean(true),
                    token: Token::new(TokenKind::Identifier, "true", 1, 1),
                    ty: Some(Type::Bool),
                };
                let input = self.value_input(&value, &finish)?;
                self.input(&finish, "VALUE", input);
                self.blocks.get_mut(&assign).unwrap()["next"] = json!(finish);
                Ok(assign)
            }
            Stmt::Call {
                arguments,
                info: Some(info),
                ..
            } => {
                let id = self.block("procedures_call", Some(parent), false, json!({}));
                self.blocks.get_mut(&id).unwrap()["mutation"] = Self::procedure_mutation(info);
                for (argument, parameter) in arguments.iter().zip(&info.arguments) {
                    let input = if parameter.ty == Type::Bool {
                        self.boolean_input(argument, &id)?
                    } else {
                        self.value_input(argument, &id)?
                    };
                    self.input(&id, &parameter.id, input);
                }
                Ok(id)
            }
            Stmt::Call {
                arguments,
                builtin: Some(builtin),
                ..
            } => self.emit_builtin(builtin, arguments, parent),
            Stmt::Function { .. } | Stmt::Call { .. } => {
                Err("internal error: unresolved function call".to_string())
            }
            Stmt::VarDecl {
                object_fields,
                symbol:
                    Some(Symbol {
                        storage: StorageClass::ObjectBinding,
                        ..
                    }),
                ..
            } => {
                let mut first = None;
                let mut previous: Option<String> = None;
                for field in object_fields {
                    let parent_block = previous.as_deref().unwrap_or(parent);
                    let block = self.block(
                        "data_setvariableto",
                        Some(parent_block),
                        false,
                        json!({"VARIABLE": [field.scratch_name.clone(), field.id.clone()]}),
                    );
                    let value = if field.ty == Type::Bool {
                        Expr {
                            kind: ExprKind::Boolean(
                                field.initial_value.as_f64().unwrap_or(0.0) != 0.0,
                            ),
                            token: Token::new(
                                TokenKind::Identifier,
                                field.source_name.clone(),
                                1,
                                1,
                            ),
                            ty: Some(Type::Bool),
                        }
                    } else if field.ty == Type::Text {
                        Expr {
                            kind: ExprKind::Text(
                                field.initial_value.as_str().unwrap_or_default().to_string(),
                            ),
                            token: Token::new(TokenKind::String, field.source_name.clone(), 1, 1),
                            ty: Some(Type::Text),
                        }
                    } else {
                        Expr {
                            kind: ExprKind::Number(field.initial_value.to_string()),
                            token: Token::new(
                                TokenKind::Identifier,
                                field.source_name.clone(),
                                1,
                                1,
                            ),
                            ty: Some(field.ty),
                        }
                    };
                    let input = self.value_input(&value, &block)?;
                    self.input(&block, "VALUE", input);
                    if let Some(previous_id) = previous.as_ref() {
                        self.blocks.get_mut(previous_id).unwrap()["next"] = json!(block);
                    } else {
                        first = Some(block.clone());
                    }
                    previous = Some(block);
                }
                first.ok_or_else(|| "internal error: empty object initialization".to_string())
            }
            Stmt::VarDecl {
                initializer,
                symbol: Some(symbol),
                ..
            } => {
                let id = self.block(
                    "data_setvariableto",
                    Some(parent),
                    false,
                    json!({"VARIABLE": [symbol.scratch_name, symbol.id]}),
                );
                let value = self.value_input(initializer, &id)?;
                self.input(&id, "VALUE", value);
                Ok(id)
            }
            Stmt::Assign {
                token,
                operator,
                value,
                symbol: Some(symbol),
                ..
            }
            | Stmt::MemberAssign {
                token,
                operator,
                value,
                symbol: Some(symbol),
                ..
            } => {
                let id = self.block(
                    "data_setvariableto",
                    Some(parent),
                    false,
                    json!({"VARIABLE": [symbol.scratch_name, symbol.id]}),
                );
                let assigned = if operator == "+=" || operator == "-=" {
                    let left = Expr {
                        kind: ExprKind::Name {
                            value: symbol.source_name.clone(),
                            symbol: Some(symbol.clone()),
                        },
                        token: token.clone(),
                        ty: Some(symbol.ty),
                    };
                    Expr {
                        kind: ExprKind::Binary {
                            operator: if operator == "+=" { "+" } else { "-" }.to_string(),
                            left: Box::new(left),
                            right: Box::new(value.clone()),
                        },
                        token: token.clone(),
                        ty: Some(symbol.ty),
                    }
                } else {
                    value.clone()
                };
                let input = self.value_input(&assigned, &id)?;
                self.input(&id, "VALUE", input);
                Ok(id)
            }
            Stmt::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                let opcode = if else_body.is_empty() {
                    "control_if"
                } else {
                    "control_if_else"
                };
                let id = self.block(opcode, Some(parent), false, json!({}));
                let condition_input = self.boolean_input(condition, &id)?;
                self.input(&id, "CONDITION", condition_input);
                let then_first = self.sequence(then_body, &id, function)?;
                self.input(&id, "SUBSTACK", json!([2, then_first]));
                if !else_body.is_empty() {
                    let else_first = self.sequence(else_body, &id, function)?;
                    self.input(&id, "SUBSTACK2", json!([2, else_first]));
                }
                Ok(id)
            }
            Stmt::While {
                condition,
                body,
                token,
            } => {
                let id = self.block("control_repeat_until", Some(parent), false, json!({}));
                let negated = make_unary("!", condition.clone(), token.clone());
                let stop_condition = if let Some(completion) =
                    function.and_then(|info| info.completed_symbol.as_ref())
                {
                    let completed = Expr {
                        kind: ExprKind::Name {
                            value: completion.source_name.clone(),
                            symbol: Some(completion.clone()),
                        },
                        token: token.clone(),
                        ty: Some(Type::Bool),
                    };
                    make_binary("||", negated, completed, token.clone())
                } else {
                    negated
                };
                let condition_input = self.boolean_input(&stop_condition, &id)?;
                self.input(&id, "CONDITION", condition_input);
                let body_first = self.sequence(body, &id, function)?;
                self.input(&id, "SUBSTACK", json!([2, body_first]));
                Ok(id)
            }
            _ => Err("internal error: unresolved variable binding".to_string()),
        }
    }

    fn boolean_input(&mut self, expr: &Expr, parent: &str) -> Result<Value, String> {
        Ok(json!([2, self.reporter(expr, parent)?]))
    }

    fn value_input(&mut self, expr: &Expr, parent: &str) -> Result<Value, String> {
        if let ExprKind::Number(value) = &expr.kind {
            return Ok(json!([1, [4, value]]));
        }
        if let ExprKind::Text(value) = &expr.kind {
            return Ok(json!([1, [10, value]]));
        }
        let reporter = self.reporter(expr, parent)?;
        let shadow = match expr.ty {
            Some(Type::Int | Type::Float) => json!([4, 10]),
            _ => json!([10, ""]),
        };
        Ok(json!([3, reporter, shadow]))
    }

    fn reporter(&mut self, expr: &Expr, parent: &str) -> Result<String, String> {
        match &expr.kind {
            ExprKind::Number(value) => self.boolean_literal(value != "0", parent),
            ExprKind::Boolean(value) => self.boolean_literal(*value, parent),
            ExprKind::Text(_) => {
                Err("text literals are Scratch shadow inputs, not reporters".to_string())
            }
            ExprKind::Member {
                symbol: Some(symbol),
                ..
            } if symbol.ty == Type::Bool => {
                let id = self.block("operator_equals", Some(parent), false, json!({}));
                let variable = self.variable_reporter(symbol, &id);
                self.input(&id, "OPERAND1", json!([3, variable, [10, ""]]));
                self.input(&id, "OPERAND2", json!([1, [10, "1"]]));
                Ok(id)
            }
            ExprKind::Member {
                symbol: Some(symbol),
                ..
            } => Ok(self.variable_reporter(symbol, parent)),
            ExprKind::Member { .. } => {
                Err("internal error: unresolved field reference".to_string())
            }
            ExprKind::ObjectInit { .. } => {
                Err("internal error: object constructor was not lowered".to_string())
            }
            ExprKind::ScratchListInit => Err(
                "internal error: Scratch list constructor is only valid in a declaration"
                    .to_string(),
            ),
            ExprKind::Name {
                symbol: Some(symbol),
                ..
            } if symbol.ty == Type::Bool && symbol.argument_id.is_some() => {
                Ok(self.variable_reporter(symbol, parent))
            }
            ExprKind::Name {
                symbol: Some(symbol),
                ..
            } if symbol.ty == Type::Bool => {
                let id = self.block("operator_equals", Some(parent), false, json!({}));
                let variable = self.variable_reporter(symbol, &id);
                self.input(&id, "OPERAND1", json!([3, variable, [10, ""]]));
                self.input(&id, "OPERAND2", json!([1, [10, "1"]]));
                Ok(id)
            }
            ExprKind::Name {
                symbol: Some(symbol),
                ..
            } if symbol.ty != Type::ScratchList => Ok(self.variable_reporter(symbol, parent)),
            ExprKind::Name {
                symbol: Some(_), ..
            } => Err("internal error: Scratch list handles are not reporters".to_string()),
            ExprKind::Name { .. } => {
                Err("internal error: unresolved variable reference".to_string())
            }
            ExprKind::Unary { operator, operand } if operator == "!" => {
                let id = self.block("operator_not", Some(parent), false, json!({}));
                let input = self.boolean_input(operand, &id)?;
                self.input(&id, "OPERAND", input);
                Ok(id)
            }
            ExprKind::Unary { operator, operand } if operator == "-" => {
                let id = self.block("operator_subtract", Some(parent), false, json!({}));
                self.input(&id, "NUM1", json!([1, [4, "0"]]));
                let input = self.value_input(operand, &id)?;
                self.input(&id, "NUM2", input);
                Ok(id)
            }
            ExprKind::Unary { operator, .. } => Err(format!(
                "internal error: unsupported unary operator {operator:?}"
            )),
            ExprKind::Binary {
                operator,
                left,
                right,
            } if operator == "&&" || operator == "||" => {
                let opcode = if operator == "&&" {
                    "operator_and"
                } else {
                    "operator_or"
                };
                let id = self.block(opcode, Some(parent), false, json!({}));
                let left_input = self.boolean_input(left, &id)?;
                let right_input = self.boolean_input(right, &id)?;
                self.input(&id, "OPERAND1", left_input);
                self.input(&id, "OPERAND2", right_input);
                Ok(id)
            }
            ExprKind::Binary {
                operator,
                left,
                right,
            } if (operator == "==" || operator == "!=") && left.ty == Some(Type::Bool) => {
                let both_true = make_binary(
                    "&&",
                    (**left).clone(),
                    (**right).clone(),
                    expr.token.clone(),
                );
                let not_left = make_unary("!", (**left).clone(), expr.token.clone());
                let not_right = make_unary("!", (**right).clone(), expr.token.clone());
                let both_false = make_binary("&&", not_left, not_right, expr.token.clone());
                let equivalent = make_binary("||", both_true, both_false, expr.token.clone());
                if operator == "!=" {
                    self.reporter(&make_unary("!", equivalent, expr.token.clone()), parent)
                } else {
                    self.reporter(&equivalent, parent)
                }
            }
            ExprKind::Binary {
                operator,
                left,
                right,
            } => {
                let (opcode, inverted) = match operator.as_str() {
                    "+" => ("operator_add", false),
                    "-" => ("operator_subtract", false),
                    "*" => ("operator_multiply", false),
                    "/" => ("operator_divide", false),
                    "==" => ("operator_equals", false),
                    "!=" => ("operator_equals", true),
                    "<" => ("operator_lt", false),
                    "<=" => ("operator_gt", true),
                    ">" => ("operator_gt", false),
                    ">=" => ("operator_lt", true),
                    other => return Err(format!("internal error: unsupported operator {other:?}")),
                };
                let id = self.block(opcode, Some(parent), false, json!({}));
                let (first_name, second_name) =
                    if matches!(operator.as_str(), "+" | "-" | "*" | "/") {
                        ("NUM1", "NUM2")
                    } else {
                        ("OPERAND1", "OPERAND2")
                    };
                let left_input = self.value_input(left, &id)?;
                let right_input = self.value_input(right, &id)?;
                self.input(&id, first_name, left_input);
                self.input(&id, second_name, right_input);
                if inverted {
                    let not_id = self.block("operator_not", Some(parent), false, json!({}));
                    self.input(&not_id, "OPERAND", json!([2, id]));
                    self.blocks.get_mut(&id).unwrap()["parent"] = json!(not_id);
                    Ok(not_id)
                } else {
                    Ok(id)
                }
            }
            ExprKind::Call {
                arguments,
                builtin: Some(builtin),
                ..
            } => {
                if builtin.result.is_none() {
                    return Err(format!(
                        "internal error: Scratch command {:?} used as a reporter",
                        builtin.name
                    ));
                }
                self.emit_builtin(builtin, arguments, parent)
            }
            ExprKind::Call { .. } => {
                Err("internal error: function call was not lowered".to_string())
            }
        }
    }

    fn variable_reporter(&mut self, symbol: &Symbol, parent: &str) -> String {
        if symbol.argument_id.is_some() {
            let opcode = if symbol.ty == Type::Bool {
                "argument_reporter_boolean"
            } else {
                "argument_reporter_string_number"
            };
            return self.block(
                opcode,
                Some(parent),
                false,
                json!({"VALUE": [symbol.source_name, null]}),
            );
        }
        self.block(
            "data_variable",
            Some(parent),
            false,
            json!({"VARIABLE": [symbol.scratch_name, symbol.id]}),
        )
    }

    fn boolean_literal(&mut self, value: bool, parent: &str) -> Result<String, String> {
        let id = self.block("operator_equals", Some(parent), false, json!({}));
        self.input(&id, "OPERAND1", json!([1, [10, "1"]]));
        self.input(
            &id,
            "OPERAND2",
            json!([1, [10, if value { "1" } else { "0" }]]),
        );
        Ok(id)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SymbolOccurrence {
    pub symbol_id: String,
    pub name: String,
    pub ty: String,
    pub mutable: bool,
    pub parameter: bool,
    pub function: bool,
    pub class_symbol: bool,
    pub namespace_symbol: bool,
    pub scratch_list: bool,
    pub line: u32,
    pub character: u32,
    pub length: u32,
    pub declaration: bool,
    pub modification: bool,
}
