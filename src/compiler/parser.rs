#[derive(Clone, Debug)]
struct Parameter {
    token: Token,
    name: String,
    ty: Type,
    symbol: Option<Symbol>,
}

#[derive(Clone, Debug)]
struct Expr {
    kind: ExprKind,
    token: Token,
    ty: Option<Type>,
}

fn constant_field_value(expression: &Expr) -> Option<(Type, Value)> {
    match &expression.kind {
        ExprKind::Text(value) => Some((Type::Text, json!(value))),
        ExprKind::Number(value) => Some((
            if value.contains('.') {
                Type::Float
            } else {
                Type::Int
            },
            serde_json::from_str(value).ok()?,
        )),
        ExprKind::Boolean(value) => Some((Type::Bool, json!(if *value { 1 } else { 0 }))),
        ExprKind::Unary { operator, operand } if operator == "-" => {
            let ExprKind::Number(value) = &operand.kind else {
                return None;
            };
            let parsed = value.parse::<f64>().ok()?;
            let negated = -parsed;
            Some((
                if value.contains('.') {
                    Type::Float
                } else {
                    Type::Int
                },
                json!(negated),
            ))
        }
        _ => None,
    }
}

#[derive(Clone, Debug)]
enum ExprKind {
    Number(String),
    Boolean(bool),
    Text(String),
    ObjectInit {
        class_id: u32,
        class_name: String,
        class_token: Token,
        constructor: Option<FunctionInfo>,
        arguments: Vec<Expr>,
    },
    ScratchListInit,
    Name {
        value: String,
        symbol: Option<Symbol>,
    },
    Member {
        object: Box<Expr>,
        name: String,
        symbol: Option<Symbol>,
    },
    Unary {
        operator: String,
        operand: Box<Expr>,
    },
    Binary {
        operator: String,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        name: String,
        receiver: Option<Box<Expr>>,
        arguments: Vec<Expr>,
        info: Option<FunctionInfo>,
        builtin: Option<BuiltinBlock>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EventKind {
    GreenFlag,
    KeyPressed(String),
    BroadcastReceived(String),
    SpriteClicked,
    CloneStart,
    BackdropSwitchedTo(String),
}

#[derive(Clone, Debug)]
enum Stmt {
    Use {
        token: Token,
        namespace: String,
        alias: Option<String>,
        alias_token: Option<Token>,
        members: Vec<(Token, String)>,
        scope: String,
    },
    Namespace {
        name_token: Token,
        path: String,
        body: Vec<Stmt>,
    },
    Class {
        name_token: Token,
        name: String,
        fields: Vec<FieldDeclaration>,
        methods: Vec<Stmt>,
    },
    Function {
        name_token: Token,
        name: String,
        warp: bool,
        parameters: Vec<Parameter>,
        return_type: Option<Type>,
        body: Vec<Stmt>,
        info: Option<FunctionInfo>,
        visibility: Visibility,
        owner_class: Option<String>,
        method_name: Option<String>,
    },
    Event {
        type_token: Token,
        event_type: String,
        values: Vec<Expr>,
        name_token: Token,
        name: String,
        kind: Option<EventKind>,
        body: Vec<Stmt>,
    },
    Call {
        token: Token,
        name: String,
        receiver: Option<Box<Expr>>,
        arguments: Vec<Expr>,
        info: Option<FunctionInfo>,
        builtin: Option<BuiltinBlock>,
    },
    Return {
        token: Token,
        value: Expr,
    },
    VarDecl {
        token: Token,
        name_token: Token,
        name: String,
        mutable: bool,
        annotation: Option<Type>,
        initializer: Expr,
        symbol: Option<Symbol>,
        object_fields: Vec<Symbol>,
    },
    Assign {
        token: Token,
        name: String,
        operator: String,
        value: Expr,
        symbol: Option<Symbol>,
    },
    MemberAssign {
        token: Token,
        field_token: Token,
        object: String,
        field: String,
        operator: String,
        value: Expr,
        symbol: Option<Symbol>,
    },
    If {
        condition: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    While {
        token: Token,
        condition: Expr,
        body: Vec<Stmt>,
    },
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
    namespace: Vec<String>,
    type_aliases: HashMap<String, Type>,
    class_ids: HashMap<String, u32>,
    next_class_id: u32,
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            index: 0,
            namespace: Vec::new(),
            type_aliases: HashMap::new(),
            class_ids: HashMap::new(),
            next_class_id: 0,
        }
    }
    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }
    fn at(&self, text: &str) -> bool {
        self.current().text == text
    }
    fn take(&mut self) -> Token {
        let token = self.current().clone();
        self.index += 1;
        token
    }
    fn expect(&mut self, text: &str) -> Result<Token, String> {
        if !self.at(text) {
            return Err(diagnostic(
                self.current(),
                format!("expected {text:?}, got {}", self.current().describe()),
            ));
        }
        Ok(self.take())
    }
    fn expect_identifier(&mut self) -> Result<Token, String> {
        if self.current().kind != TokenKind::Identifier
            || matches!(self.current().text.as_str(), "public" | "private" | "self")
        {
            return Err(diagnostic(self.current(), "expected an identifier"));
        }
        Ok(self.take())
    }
    fn type_annotation(&mut self) -> Result<Type, String> {
        let token = self.expect_identifier()?;
        let mut path = token.text.clone();
        while self.at(".") {
            self.take();
            path.push('.');
            path.push_str(&self.expect_identifier()?.text);
        }
        if let Ok(ty) = Type::parse(&path, &token) {
            return Ok(ty);
        }
        let mut scope = self.namespace.clone();
        loop {
            let prefix = scope.join(".");
            let candidate = if prefix.is_empty() {
                path.clone()
            } else {
                format!("{prefix}.{path}")
            };
            if let Some(class_id) = self.class_ids.get(&candidate) {
                return Ok(Type::Class(*class_id));
            }
            if scope.pop().is_none() {
                break;
            }
        }
        if !path.contains('.') {
            let mut scope = self.namespace.clone();
            loop {
                let scope_name = scope.join(".");
                let key = format!("{scope_name}::{path}");
                if let Some(ty) = self.type_aliases.get(&key) {
                    return Ok(*ty);
                }
                if scope.pop().is_none() {
                    break;
                }
            }
        }
        Type::parse(&path, &token)
    }
    fn semicolons(&mut self) {
        while self.at(";") {
            self.take();
        }
    }

    fn function_declaration_starts_here(&self) -> bool {
        let mut index = self.index;
        while matches!(
            self.tokens.get(index).map(|token| token.text.as_str()),
            Some("const" | "warp")
        ) {
            index += 1;
        }
        self.tokens.get(index).map(|token| token.text.as_str()) == Some("fn")
    }

    fn parse(&mut self) -> Result<Vec<Stmt>, String> {
        let mut statements = Vec::new();
        while self.current().kind != TokenKind::Eof {
            if self.at("}") {
                return Err(diagnostic(self.current(), "unexpected '}'"));
            }
            if self.at("use") {
                statements.push(self.use_declaration()?);
            } else if self.at("ns") {
                statements.push(self.namespace_declaration()?);
            } else if self.at("class") {
                statements.push(self.class_declaration()?);
            } else if self.function_declaration_starts_here() {
                statements.push(self.function_declaration()?);
            } else if self.at("event") {
                statements.push(self.event_declaration()?);
            } else {
                statements.push(self.statement()?);
            }
            self.semicolons();
        }
        Ok(statements)
    }

    fn namespace_path(&self) -> String {
        self.namespace.join(".")
    }

    fn qualified_name(&self, leaf: &str) -> String {
        if self.namespace.is_empty() {
            leaf.to_string()
        } else {
            format!("{}.{}", self.namespace_path(), leaf)
        }
    }

    fn use_declaration(&mut self) -> Result<Stmt, String> {
        let token = self.expect("use")?;
        let mut path = self.expect_identifier()?.text;
        let mut members = Vec::new();
        let mut alias = None;
        let mut alias_token = None;
        while self.at(".") {
            self.take();
            if self.at("{") {
                self.take();
                if !self.at("}") {
                    loop {
                        let member = self.expect_identifier()?;
                        members.push((member.clone(), member.text));
                        if !self.at(",") {
                            break;
                        }
                        self.take();
                    }
                }
                self.expect("}")?;
                break;
            }
            path.push('.');
            path.push_str(&self.expect_identifier()?.text);
        }
        if self.at("as") {
            self.take();
            let name = self.expect_identifier()?;
            alias = Some(name.text.clone());
            alias_token = Some(name);
            if self.at(".")
                && self
                    .tokens
                    .get(self.index + 1)
                    .map(|token| token.text.as_str())
                    == Some("{")
            {
                self.take();
                self.take();
                if !self.at("}") {
                    loop {
                        let member = self.expect_identifier()?;
                        members.push((member.clone(), member.text));
                        if !self.at(",") {
                            break;
                        }
                        self.take();
                    }
                }
                self.expect("}")?;
            }
        }
        let alias_at_end = self.at("as");
        if alias_at_end {
            self.take();
            let name = self.expect_identifier()?;
            alias = Some(name.text.clone());
            alias_token = Some(name);
        }
        let scope = self.namespace_path();
        if members.is_empty() {
            if let Some(ty) = builtin_type_path(&path) {
                let local = alias
                    .clone()
                    .unwrap_or_else(|| path.rsplit('.').next().unwrap_or(&path).to_string());
                self.type_aliases.insert(format!("{scope}::{local}"), ty);
            }
        } else {
            for (_, member) in &members {
                let target = format!("{path}.{member}");
                if let Some(ty) = builtin_type_path(&target) {
                    let local = alias
                        .as_ref()
                        .map(|alias| format!("{alias}.{member}"))
                        .unwrap_or_else(|| member.clone());
                    self.type_aliases.insert(format!("{scope}::{local}"), ty);
                }
            }
        }
        Ok(Stmt::Use {
            token,
            namespace: path,
            alias,
            alias_token,
            members,
            scope,
        })
    }

    fn namespace_declaration(&mut self) -> Result<Stmt, String> {
        self.expect("ns")?;
        let name_token = self.expect_identifier()?;
        let name = name_token.text.clone();
        let path = self.qualified_name(&name);
        self.expect("{")?;
        self.namespace.push(name.clone());
        let mut body = Vec::new();
        self.semicolons();
        while !self.at("}") {
            if self.current().kind == TokenKind::Eof {
                self.namespace.pop();
                return Err(diagnostic(
                    self.current(),
                    "expected '}' before end of file",
                ));
            }
            if self.at("use") {
                body.push(self.use_declaration()?);
            } else if self.at("ns") {
                body.push(self.namespace_declaration()?);
            } else if self.at("class") {
                body.push(self.class_declaration()?);
            } else if self.function_declaration_starts_here() {
                body.push(self.function_declaration()?);
            } else if self.at("event") {
                return Err(diagnostic(
                    self.current(),
                    "event handlers must be declared at module scope",
                ));
            } else {
                self.namespace.pop();
                return Err(diagnostic(
                    self.current(),
                    "namespace bodies may contain only use, namespace, class, and function declarations",
                ));
            }
            self.semicolons();
        }
        self.take();
        self.namespace.pop();
        Ok(Stmt::Namespace {
            name_token,
            path,
            body,
        })
    }

    fn class_declaration(&mut self) -> Result<Stmt, String> {
        self.expect("class")?;
        let name_token = self.expect_identifier()?;
        let name = self.qualified_name(&name_token.text);
        let class_id = self.next_class_id;
        self.next_class_id += 1;
        self.class_ids.insert(name.clone(), class_id);
        self.expect("{")?;
        let mut fields = Vec::new();
        let mut methods = Vec::new();
        self.semicolons();
        while !self.at("}") {
            if self.current().kind == TokenKind::Eof {
                return Err(diagnostic(
                    self.current(),
                    "expected '}' before end of file",
                ));
            }
            let visibility = if self.at("public") {
                self.take();
                Visibility::Public
            } else if self.at("private") {
                self.take();
                Visibility::Private
            } else {
                Visibility::Public
            };
            if self.at("var") || self.at("const") {
                let keyword = self.take();
                let field_token = self.expect_identifier()?;
                let field_name = field_token.text.clone();
                self.expect(":")?;
                let ty = self.type_annotation()?;
                self.expect("=")?;
                let initializer = self.expression(1)?;
                fields.push(FieldDeclaration {
                    token: field_token,
                    name: field_name,
                    ty,
                    mutable: keyword.text == "var",
                    visibility,
                    initializer,
                });
            } else if self.function_declaration_starts_here() {
                let mut method = self.function_declaration()?;
                let Stmt::Function {
                    name: function_name,
                    visibility: method_visibility,
                    owner_class,
                    method_name,
                    ..
                } = &mut method
                else {
                    unreachable!();
                };
                let leaf_name = function_name
                    .rsplit('.')
                    .next()
                    .unwrap_or(function_name)
                    .to_string();
                *method_name = Some(leaf_name.clone());
                *owner_class = Some(name.clone());
                *function_name = format!("{name}.{leaf_name}");
                *method_visibility = visibility;
                methods.push(method);
            } else {
                return Err(diagnostic(
                    self.current(),
                    "class bodies support fields and fn methods, optionally prefixed with public or private",
                ));
            }
            self.semicolons();
        }
        self.take();
        Ok(Stmt::Class {
            name_token,
            name,
            fields,
            methods,
        })
    }

    fn function_declaration(&mut self) -> Result<Stmt, String> {
        let mut is_const = false;
        let mut warp = false;
        while self.at("const") || self.at("warp") {
            let modifier = self.take();
            let already_present = if modifier.text == "const" {
                let previous = is_const;
                is_const = true;
                previous
            } else {
                let previous = warp;
                warp = true;
                previous
            };
            if already_present {
                return Err(diagnostic(
                    &modifier,
                    format!("duplicate function modifier {:?}", modifier.text),
                ));
            }
        }
        self.expect("fn")?;
        let name_token = self.expect_identifier()?;
        let name = self.qualified_name(&name_token.text);
        self.expect("(")?;
        let mut parameters = Vec::new();
        if !self.at(")") {
            loop {
                let parameter_token = self.expect_identifier()?;
                self.expect(":")?;
                let ty = self.type_annotation()?;
                parameters.push(Parameter {
                    name: parameter_token.text.clone(),
                    token: parameter_token,
                    ty,
                    symbol: None,
                });
                if !self.at(",") {
                    break;
                }
                self.take();
            }
        }
        self.expect(")")?;
        let return_type = if self.at("->") {
            self.take();
            Some(self.type_annotation()?)
        } else {
            None
        };
        let body = self.block()?;
        Ok(Stmt::Function {
            name_token,
            name,
            warp,
            parameters,
            return_type,
            body,
            info: None,
            visibility: Visibility::Public,
            owner_class: None,
            method_name: None,
        })
    }

    fn event_declaration(&mut self) -> Result<Stmt, String> {
        self.expect("event")?;
        let type_token = self.expect_identifier()?;
        let event_type = type_token.text.clone();
        self.expect("(")?;
        let mut values = Vec::new();
        if !self.at(")") {
            loop {
                values.push(self.expression(1)?);
                if !self.at(",") {
                    break;
                }
                self.take();
            }
        }
        self.expect(")")?;
        let name_token = self.expect_identifier()?;
        let name = name_token.text.clone();
        let body = self.block()?;
        Ok(Stmt::Event {
            type_token,
            event_type,
            values,
            name_token,
            name,
            kind: None,
            body,
        })
    }

    fn statement(&mut self) -> Result<Stmt, String> {
        if self.at("var") || self.at("const") {
            return self.declaration();
        }
        if self.at("if") {
            return self.if_statement();
        }
        if self.at("while") {
            return self.while_statement();
        }
        if self.at("return") {
            let token = self.take();
            let value = self.expression(1)?;
            return Ok(Stmt::Return { token, value });
        }
        if self.current().kind == TokenKind::Identifier
            && self.tokens[self.index + 1].text == "."
            && self.tokens[self.index + 2].kind == TokenKind::Identifier
            && matches!(self.tokens[self.index + 3].text.as_str(), "=" | "+=" | "-=")
        {
            let token = self.take();
            let object = token.text.clone();
            self.take();
            let field_token = self.take();
            let field = field_token.text.clone();
            let operator = self.take().text;
            let value = self.expression(1)?;
            return Ok(Stmt::MemberAssign {
                token,
                field_token,
                object,
                field,
                operator,
                value,
                symbol: None,
            });
        }
        if self.current().kind == TokenKind::Identifier
            && matches!(self.tokens[self.index + 1].text.as_str(), "=" | "+=" | "-=")
        {
            let token = self.take();
            let name = token.text.clone();
            let operator = self.take().text;
            let value = self.expression(1)?;
            return Ok(Stmt::Assign {
                token,
                name,
                operator,
                value,
                symbol: None,
            });
        }
        if self.current().kind == TokenKind::Identifier && self.tokens[self.index + 1].text == "." {
            let expression = self.expression(1)?;
            if let ExprKind::Call {
                name,
                receiver,
                arguments,
                ..
            } = expression.kind
            {
                return Ok(Stmt::Call {
                    token: expression.token,
                    name,
                    receiver,
                    arguments,
                    info: None,
                    builtin: None,
                });
            }
            return Err(diagnostic(&expression.token, "expected a function call"));
        }
        if self.current().kind == TokenKind::Identifier && self.tokens[self.index + 1].text == "(" {
            return self.call_statement();
        }
        Err(diagnostic(
            self.current(),
            "expected a variable declaration, assignment, function call, if, or while",
        ))
    }

    fn call_statement(&mut self) -> Result<Stmt, String> {
        let token = self.expect_identifier()?;
        let name = token.text.clone();
        self.expect("(")?;
        let mut arguments = Vec::new();
        if !self.at(")") {
            loop {
                arguments.push(self.expression(1)?);
                if !self.at(",") {
                    break;
                }
                self.take();
            }
        }
        self.expect(")")?;
        Ok(Stmt::Call {
            token,
            name,
            receiver: None,
            arguments,
            info: None,
            builtin: None,
        })
    }

    fn declaration(&mut self) -> Result<Stmt, String> {
        let keyword = self.take();
        let name_token = self.expect_identifier()?;
        let name = name_token.text.clone();
        let annotation = if self.at(":") {
            self.take();
            Some(self.type_annotation()?)
        } else {
            None
        };
        self.expect("=")?;
        let initializer = self.expression(1)?;
        Ok(Stmt::VarDecl {
            token: keyword.clone(),
            name_token,
            name,
            mutable: keyword.text == "var",
            annotation,
            initializer,
            symbol: None,
            object_fields: Vec::new(),
        })
    }

    fn block(&mut self) -> Result<Vec<Stmt>, String> {
        self.expect("{")?;
        let mut body = Vec::new();
        self.semicolons();
        while !self.at("}") {
            if self.current().kind == TokenKind::Eof {
                return Err(diagnostic(
                    self.current(),
                    "expected '}' before end of file",
                ));
            }
            body.push(self.statement()?);
            self.semicolons();
        }
        self.take();
        Ok(body)
    }

    fn if_statement(&mut self) -> Result<Stmt, String> {
        self.take();
        let condition = self.expression(1)?;
        let then_body = self.block()?;
        let else_body = if self.at("else") {
            self.take();
            self.block()?
        } else {
            Vec::new()
        };
        Ok(Stmt::If {
            condition,
            then_body,
            else_body,
        })
    }

    fn while_statement(&mut self) -> Result<Stmt, String> {
        let token = self.take();
        let condition = self.expression(1)?;
        let body = self.block()?;
        Ok(Stmt::While {
            token,
            condition,
            body,
        })
    }

    fn precedence(operator: &str) -> Option<u8> {
        match operator {
            "||" => Some(1),
            "&&" => Some(2),
            "==" | "!=" => Some(3),
            "<" | "<=" | ">" | ">=" => Some(4),
            "+" | "-" => Some(5),
            "*" | "/" => Some(6),
            _ => None,
        }
    }

    fn expression(&mut self, minimum: u8) -> Result<Expr, String> {
        let mut left = self.prefix()?;
        while let Some(precedence) = Self::precedence(&self.current().text) {
            if precedence < minimum {
                break;
            }
            let operator = self.take();
            let right = self.expression(precedence + 1)?;
            left = Expr {
                kind: ExprKind::Binary {
                    operator: operator.text.clone(),
                    left: Box::new(left),
                    right: Box::new(right),
                },
                token: operator,
                ty: None,
            };
        }
        Ok(left)
    }

    fn prefix(&mut self) -> Result<Expr, String> {
        let token = self.current().clone();
        if token.text == "!" || token.text == "-" {
            self.take();
            let operand = self.expression(7)?;
            return Ok(Expr {
                kind: ExprKind::Unary {
                    operator: token.text.clone(),
                    operand: Box::new(operand),
                },
                token,
                ty: None,
            });
        }
        if token.text == "(" {
            self.take();
            let expr = self.expression(1)?;
            self.expect(")")?;
            return self.postfix(expr);
        }
        match token.kind {
            TokenKind::Number => {
                self.take();
                Ok(Expr {
                    kind: ExprKind::Number(token.text.clone()),
                    token: token.clone(),
                    ty: Some(if token.text.contains('.') {
                        Type::Float
                    } else {
                        Type::Int
                    }),
                })
            }
            TokenKind::String => {
                self.take();
                Ok(Expr {
                    kind: ExprKind::Text(token.text.clone()),
                    token,
                    ty: Some(Type::Text),
                })
            }
            TokenKind::Identifier if token.text == "true" || token.text == "false" => {
                self.take();
                Ok(Expr {
                    kind: ExprKind::Boolean(token.text == "true"),
                    token,
                    ty: Some(Type::Bool),
                })
            }
            TokenKind::Identifier => {
                self.take();
                if self.at("(") {
                    self.take();
                    let mut arguments = Vec::new();
                    if !self.at(")") {
                        loop {
                            arguments.push(self.expression(1)?);
                            if !self.at(",") {
                                break;
                            }
                            self.take();
                        }
                    }
                    self.expect(")")?;
                    self.postfix(Expr {
                        kind: ExprKind::Call {
                            name: token.text.clone(),
                            receiver: None,
                            arguments,
                            info: None,
                            builtin: None,
                        },
                        token,
                        ty: None,
                    })
                } else {
                    self.postfix(Expr {
                        kind: ExprKind::Name {
                            value: token.text.clone(),
                            symbol: None,
                        },
                        token,
                        ty: None,
                    })
                }
            }
            _ => Err(diagnostic(&token, "expected an expression")),
        }
    }

    fn postfix(&mut self, mut expression: Expr) -> Result<Expr, String> {
        while self.at(".") {
            self.take();
            let member = self.expect_identifier()?;
            if self.at("(") {
                let name = qualified_expression_name(&expression)
                    .map(|prefix| format!("{prefix}.{}", member.text))
                    .unwrap_or_else(|| member.text.clone());
                let token = member.clone();
                let arguments = self.call_arguments()?;
                expression = Expr {
                    kind: ExprKind::Call {
                        name,
                        receiver: Some(Box::new(expression)),
                        arguments,
                        info: None,
                        builtin: None,
                    },
                    token,
                    ty: None,
                };
                continue;
            }
            let token = member.clone();
            expression = Expr {
                kind: ExprKind::Member {
                    object: Box::new(expression),
                    name: member.text,
                    symbol: None,
                },
                token,
                ty: None,
            };
        }
        Ok(expression)
    }

    fn call_arguments(&mut self) -> Result<Vec<Expr>, String> {
        self.expect("(")?;
        let mut arguments = Vec::new();
        if !self.at(")") {
            loop {
                arguments.push(self.expression(1)?);
                if !self.at(",") {
                    break;
                }
                self.take();
            }
        }
        self.expect(")")?;
        Ok(arguments)
    }
}

fn qualified_expression_name(expression: &Expr) -> Option<String> {
    match &expression.kind {
        ExprKind::Name { value, .. } => Some(value.clone()),
        ExprKind::Member { object, name, .. } => {
            Some(format!("{}.{}", qualified_expression_name(object)?, name))
        }
        _ => None,
    }
}
