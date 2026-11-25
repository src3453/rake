use ariadne::{Label, Report, ReportKind, Source};
use serde_json::{Map, Value, json};
use std::collections::HashMap;
use std::fs::{self, File};
#[cfg(test)]
use std::io::Read;
use std::io::Write;
use std::path::Path;
#[cfg(test)]
use zip::ZipArchive;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipWriter};

#[derive(Clone, Debug)]
struct Token {
    kind: TokenKind,
    text: String,
    line: usize,
    column: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TokenKind {
    Identifier,
    Number,
    Symbol,
    Eof,
}

impl Token {
    fn new(kind: TokenKind, text: impl Into<String>, line: usize, column: usize) -> Self {
        Self {
            kind,
            text: text.into(),
            line,
            column,
        }
    }

    fn describe(&self) -> String {
        if self.kind == TokenKind::Eof {
            "end of file".to_string()
        } else {
            format!("{:?}", self.text)
        }
    }
}

fn diagnostic(token: &Token, message: impl AsRef<str>) -> String {
    format!("{}:{}: {}", token.line, token.column, message.as_ref())
}

fn lex(source: &str) -> Result<Vec<Token>, String> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let (mut index, mut line, mut column) = (0, 1, 1);
    while index < chars.len() {
        let current = chars[index];
        if current == '\n' {
            line += 1;
            column = 1;
            index += 1;
            continue;
        }
        if current.is_whitespace() {
            index += 1;
            column += 1;
            continue;
        }
        if current == '/' && chars.get(index + 1) == Some(&'/') {
            while index < chars.len() && chars[index] != '\n' {
                index += 1;
                column += 1;
            }
            continue;
        }
        let start_line = line;
        let start_column = column;
        if current.is_ascii_alphabetic() || current == '_' {
            let start = index;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                index += 1;
                column += 1;
            }
            let text: String = chars[start..index].iter().collect();
            tokens.push(Token::new(
                TokenKind::Identifier,
                text,
                start_line,
                start_column,
            ));
            continue;
        }
        if current.is_ascii_digit() {
            let start = index;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
                column += 1;
            }
            if index + 1 < chars.len() && chars[index] == '.' && chars[index + 1].is_ascii_digit() {
                index += 1;
                column += 1;
                while index < chars.len() && chars[index].is_ascii_digit() {
                    index += 1;
                    column += 1;
                }
            }
            let text: String = chars[start..index].iter().collect();
            tokens.push(Token::new(
                TokenKind::Number,
                text,
                start_line,
                start_column,
            ));
            continue;
        }
        let pair = chars
            .get(index..index + 2)
            .map(|slice| slice.iter().collect::<String>());
        if let Some(pair) = pair.filter(|pair| {
            matches!(
                pair.as_str(),
                "==" | "!=" | "<=" | ">=" | "&&" | "||" | "+=" | "-="
            )
        }) {
            tokens.push(Token::new(
                TokenKind::Symbol,
                pair,
                start_line,
                start_column,
            ));
            index += 2;
            column += 2;
            continue;
        }
        if "{}():;,+-*/<>=!".contains(current) {
            tokens.push(Token::new(
                TokenKind::Symbol,
                current.to_string(),
                start_line,
                start_column,
            ));
            index += 1;
            column += 1;
            continue;
        }
        return Err(diagnostic(
            &Token::new(TokenKind::Symbol, current.to_string(), line, column),
            format!("unexpected character {current:?}"),
        ));
    }
    tokens.push(Token::new(TokenKind::Eof, "", line, column));
    Ok(tokens)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Type {
    Int,
    Float,
    Bool,
}

impl Type {
    fn parse(token: &Token) -> Result<Self, String> {
        match token.text.as_str() {
            "int" => Ok(Self::Int),
            "float" => Ok(Self::Float),
            "bool" => Ok(Self::Bool),
            _ => Err(diagnostic(
                token,
                format!(
                    "unknown type {:?}; expected int, float, or bool",
                    token.text
                ),
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
        }
    }

    fn numeric(self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }
}

#[derive(Clone, Debug)]
struct Symbol {
    source_name: String,
    scratch_name: String,
    ty: Type,
    mutable: bool,
    id: String,
}

#[derive(Clone, Debug)]
struct Expr {
    kind: ExprKind,
    token: Token,
    ty: Option<Type>,
}

#[derive(Clone, Debug)]
enum ExprKind {
    Number(String),
    Boolean(bool),
    Name {
        value: String,
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
}

#[derive(Clone, Debug)]
enum Stmt {
    VarDecl {
        token: Token,
        name: String,
        mutable: bool,
        annotation: Option<Type>,
        initializer: Expr,
        symbol: Option<Symbol>,
    },
    Assign {
        token: Token,
        name: String,
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
}

impl Parser {
    fn new(tokens: Vec<Token>) -> Self {
        Self { tokens, index: 0 }
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
        if self.current().kind != TokenKind::Identifier {
            return Err(diagnostic(self.current(), "expected an identifier"));
        }
        Ok(self.take())
    }
    fn semicolons(&mut self) {
        while self.at(";") {
            self.take();
        }
    }

    fn parse(&mut self) -> Result<Vec<Stmt>, String> {
        let mut statements = Vec::new();
        while self.current().kind != TokenKind::Eof {
            if self.at("}") {
                return Err(diagnostic(self.current(), "unexpected '}'"));
            }
            statements.push(self.statement()?);
            self.semicolons();
        }
        Ok(statements)
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
        Err(diagnostic(
            self.current(),
            "expected a variable declaration, assignment, if, or while",
        ))
    }

    fn declaration(&mut self) -> Result<Stmt, String> {
        let keyword = self.take();
        let name = self.expect_identifier()?.text;
        let annotation = if self.at(":") {
            self.take();
            Some(Type::parse(&self.expect_identifier()?)?)
        } else {
            None
        };
        self.expect("=")?;
        let initializer = self.expression(1)?;
        Ok(Stmt::VarDecl {
            token: keyword.clone(),
            name,
            mutable: keyword.text == "var",
            annotation,
            initializer,
            symbol: None,
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
            return Ok(expr);
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
                Ok(Expr {
                    kind: ExprKind::Name {
                        value: token.text.clone(),
                        symbol: None,
                    },
                    token,
                    ty: None,
                })
            }
            _ => Err(diagnostic(&token, "expected an expression")),
        }
    }
}

struct TypeChecker {
    scopes: Vec<HashMap<String, Symbol>>,
    symbols: Vec<Symbol>,
    scratch_names: Vec<String>,
    next_id: usize,
}

impl TypeChecker {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            symbols: Vec::new(),
            scratch_names: Vec::new(),
            next_id: 0,
        }
    }

    fn check(mut self, statements: &mut [Stmt]) -> Result<Vec<Symbol>, String> {
        self.check_statements(statements, false)?;
        Ok(self.symbols)
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

    fn check_statement(&mut self, statement: &mut Stmt) -> Result<(), String> {
        match statement {
            Stmt::VarDecl {
                token,
                name,
                mutable,
                annotation,
                initializer,
                symbol,
            } => {
                if self.scopes.last().unwrap().contains_key(name) {
                    return Err(diagnostic(
                        token,
                        format!("{name:?} is already declared in this scope"),
                    ));
                }
                let initializer_type = self.check_expr(initializer)?;
                let declared_type = annotation.unwrap_or(initializer_type);
                if !Self::assignable(declared_type, initializer_type) {
                    return Err(diagnostic(
                        &initializer.token,
                        format!(
                            "cannot initialize {} with {}",
                            declared_type.name(),
                            initializer_type.name()
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
                let value = Symbol {
                    source_name: name.clone(),
                    scratch_name,
                    ty: declared_type,
                    mutable: *mutable,
                    id: format!("v{:019x}", self.next_id),
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
                            value_type.name(),
                            resolved.ty.name()
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

    fn assignable(expected: Type, actual: Type) -> bool {
        expected == actual || (expected == Type::Float && actual == Type::Int)
    }

    fn check_expr(&mut self, expression: &mut Expr) -> Result<Type, String> {
        let ty = match &mut expression.kind {
            ExprKind::Number(value) => {
                if value.contains('.') {
                    Type::Float
                } else {
                    Type::Int
                }
            }
            ExprKind::Boolean(_) => Type::Bool,
            ExprKind::Name { value, symbol } => {
                let resolved = self.lookup(value, &expression.token)?;
                *symbol = Some(resolved.clone());
                resolved.ty
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
                    "==" | "!="
                        if left_type == right_type
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

struct ScratchBuilder {
    blocks: Map<String, Value>,
    next_id: usize,
}

impl ScratchBuilder {
    fn new() -> Self {
        Self {
            blocks: Map::new(),
            next_id: 0,
        }
    }
    fn next_id(&mut self) -> String {
        self.next_id += 1;
        format!("b{:019x}", self.next_id)
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

    fn sequence(&mut self, statements: &[Stmt], parent: &str) -> Result<Option<String>, String> {
        let mut first = None;
        let mut previous: Option<String> = None;
        for statement in statements {
            let current = self.statement(statement, previous.as_deref().unwrap_or(parent))?;
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

    fn statement(&mut self, statement: &Stmt, parent: &str) -> Result<String, String> {
        match statement {
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
                let then_first = self.sequence(then_body, &id)?;
                self.input(&id, "SUBSTACK", json!([2, then_first]));
                if !else_body.is_empty() {
                    let else_first = self.sequence(else_body, &id)?;
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
                let negated = Expr {
                    kind: ExprKind::Unary {
                        operator: "!".to_string(),
                        operand: Box::new(condition.clone()),
                    },
                    token: token.clone(),
                    ty: Some(Type::Bool),
                };
                let condition_input = self.boolean_input(&negated, &id)?;
                self.input(&id, "CONDITION", condition_input);
                let body_first = self.sequence(body, &id)?;
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
            } => Ok(self.variable_reporter(symbol, parent)),
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
        }
    }

    fn variable_reporter(&mut self, symbol: &Symbol, parent: &str) -> String {
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

pub fn compile_source(source: &str) -> Result<(Value, Vec<(String, Vec<u8>)>, usize), String> {
    let tokens = lex(source)?;
    let mut statements = Parser::new(tokens).parse()?;
    let symbols = TypeChecker::new().check(&mut statements)?;
    let mut builder = ScratchBuilder::new();
    let hat = builder.block("event_whenflagclicked", None, true, json!({}));
    let first = builder.sequence(&statements, &hat)?;
    builder.blocks.get_mut(&hat).unwrap()["next"] = json!(first);
    let block_count = builder.blocks.len();

    let backdrop_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"480\" height=\"360\" viewBox=\"0 0 480 360\"><rect width=\"480\" height=\"360\" fill=\"#ffffff\"/></svg>";
    let sprite_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\" viewBox=\"0 0 100 100\"><circle cx=\"50\" cy=\"50\" r=\"40\" fill=\"#4c97ff\" stroke=\"#3373cc\" stroke-width=\"4\"/></svg>";
    // These IDs are the MD5 hashes of the exact built-in SVG bytes above.
    let backdrop_id = "ff3f2e0196df3c7d286c4c13e441b003";
    let sprite_id = "c5091827e2b0bff8cb6133200ffa1fee";
    let backdrop_costume = json!({"assetId": backdrop_id, "name": "backdrop1", "dataFormat": "svg", "md5ext": format!("{backdrop_id}.svg"), "rotationCenterX": 240, "rotationCenterY": 180});
    let sprite_costume = json!({"assetId": sprite_id, "name": "costume1", "dataFormat": "svg", "md5ext": format!("{sprite_id}.svg"), "bitmapResolution": 1, "rotationCenterX": 50, "rotationCenterY": 50});

    let stage = json!({
        "isStage": true, "name": "Stage", "variables": {}, "lists": {}, "broadcasts": {}, "blocks": {}, "comments": {},
        "currentCostume": 0, "costumes": [backdrop_costume], "sounds": [], "volume": 100, "layerOrder": 0, "tempo": 60,
        "videoTransparency": 50, "videoState": "on", "textToSpeechLanguage": null
    });
    let mut variables = Map::new();
    for symbol in &symbols {
        variables.insert(symbol.id.clone(), json!([symbol.scratch_name, 0]));
    }
    let sprite = json!({
        "isStage": false, "name": "Sprite1", "variables": variables, "lists": {}, "broadcasts": {}, "blocks": builder.blocks, "comments": {},
        "currentCostume": 0, "costumes": [sprite_costume], "sounds": [], "volume": 100, "layerOrder": 1,
        "visible": true, "x": 0, "y": 0, "size": 100, "direction": 90, "draggable": false, "rotationStyle": "all around"
    });
    let project = json!({
        "targets": [stage, sprite], "monitors": [], "extensions": [],
        "meta": {"semver": "3.0.0", "vm": "15.3.0", "agent": "Rake compiler"}
    });
    let assets = vec![
        (format!("{backdrop_id}.svg"), backdrop_svg.to_vec()),
        (format!("{sprite_id}.svg"), sprite_svg.to_vec()),
    ];
    Ok((project, assets, block_count))
}

/// Render a frontend diagnostic as a source-labelled Ariadne report.
/// Frontend errors use `line:column: message`; lexer columns count Unicode scalars.
pub fn render_diagnostic(file_name: &str, source: &str, message: &str) -> String {
    let mut parts = message.splitn(3, ':');
    let line_number = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let column_number = parts
        .next()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(1)
        .max(1);
    let detail = parts.next().unwrap_or(message).trim();

    let line_start: usize = source
        .split_inclusive('\n')
        .take(line_number.saturating_sub(1))
        .map(|preceding| preceding.chars().count())
        .sum();
    let line_text = source.split('\n').nth(line_number - 1).unwrap_or("");
    let char_column = column_number - 1;
    let byte_column = line_text
        .char_indices()
        .nth(char_column)
        .map(|(index, _)| index)
        .unwrap_or(line_text.len());
    let start = line_start + char_column.min(line_text.chars().count());
    let remainder = line_text.get(byte_column..).unwrap_or("");
    let mut chars = remainder.chars();
    let first = chars.next();
    let token_len = match first {
        Some(character) if character == '_' || character.is_alphanumeric() => remainder
            .chars()
            .take_while(|character| *character == '_' || character.is_alphanumeric())
            .count(),
        Some(first) => {
            let second = chars.next();
            if matches!(
                (first, second),
                ('=', Some('='))
                    | ('!', Some('='))
                    | ('<', Some('='))
                    | ('>', Some('='))
                    | ('&', Some('&'))
                    | ('|', Some('|'))
                    | ('+', Some('='))
                    | ('-', Some('='))
            ) {
                2
            } else {
                1
            }
        }
        None => 0,
    };
    let end = start + token_len;
    let span = start..end;
    let report = Report::build(ReportKind::Error, (file_name, span.clone()))
        .with_message("compilation failed")
        .with_label(Label::new((file_name, span)).with_message(detail.to_string()))
        .finish();
    let mut output = Vec::new();
    if report
        .write((file_name, Source::from(source)), &mut output)
        .is_err()
    {
        return message.to_string();
    }
    String::from_utf8_lossy(&output).into_owned()
}

pub fn write_sb3(path: &Path, project: &Value, assets: &[(String, Vec<u8>)]) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    let file =
        File::create(path).map_err(|error| format!("cannot create {}: {error}", path.display()))?;
    let mut archive = ZipWriter::new(file);
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    archive
        .start_file("project.json", options)
        .map_err(|error| error.to_string())?;
    archive
        .write_all(
            serde_json::to_string(project)
                .map_err(|error| error.to_string())?
                .as_bytes(),
        )
        .map_err(|error| error.to_string())?;
    for (name, contents) in assets {
        archive
            .start_file(name, options)
            .map_err(|error| error.to_string())?;
        archive
            .write_all(contents)
            .map_err(|error| error.to_string())?;
    }
    archive.finish().map_err(|error| error.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn source_project(source: &str) -> (Value, Vec<(String, Vec<u8>)>) {
        let (project, assets, _) = compile_source(source).expect("compile source");
        (project, assets)
    }

    #[test]
    fn emits_sb3_with_control_flow_and_standard_blocks() {
        let (project, assets) = source_project(
            "var count: int = 0\nvar enabled: bool = true\nif enabled && count < 3 { count += 1 }\nwhile count < 3 { count = count + 1 }",
        );
        assert_eq!(project["targets"][0]["name"], "Stage");
        assert_eq!(project["targets"][1]["name"], "Sprite1");
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let opcodes: Vec<&str> = blocks
            .values()
            .map(|block| block["opcode"].as_str().unwrap())
            .collect();
        for opcode in [
            "event_whenflagclicked",
            "control_if",
            "control_repeat_until",
            "operator_and",
            "operator_lt",
            "operator_add",
        ] {
            assert!(opcodes.contains(&opcode), "missing opcode {opcode}");
        }
        assert_eq!(assets.len(), 2);
    }

    #[test]
    fn rejects_const_assignment_and_non_boolean_conditions() {
        assert!(
            compile_source("const score: int = 0\nscore = 1")
                .unwrap_err()
                .contains("cannot assign to const")
        );
        assert!(
            compile_source("var score: int = 0\nif score { score = 1 }")
                .unwrap_err()
                .contains("if condition must have type bool")
        );
    }

    #[test]
    fn rejects_unknown_names_and_resolves_shadowed_bindings() {
        assert!(
            compile_source("var score = missing + 1")
                .unwrap_err()
                .contains("unknown variable \"missing\"")
        );
        let (project, _) = source_project(
            "var value: int = 1\nif true { var value: int = 2\nvalue = value + 1 }\nvalue = value + 1",
        );
        let variables = project["targets"][1]["variables"].as_object().unwrap();
        let names: Vec<&str> = variables
            .values()
            .map(|entry| entry[0].as_str().unwrap())
            .collect();
        assert!(names.contains(&"value"));
        assert!(names.contains(&"value_2"));
    }

    #[test]
    fn renders_source_spanned_errors_with_ariadne() {
        let source = "// 東京\nvar score: int = 0\nif score { score = 1 }";
        let error = compile_source(source).unwrap_err();
        let rendered = render_diagnostic("example.src", source, &error);
        assert!(rendered.contains("example.src"));
        assert!(rendered.contains("example.src:3:4"));
        assert!(rendered.contains("if condition must have type bool"));
    }

    #[test]
    fn writes_project_json_and_assets_into_sb3_archive() {
        let (project, assets) = source_project("var value: int = 3");
        let path = std::env::temp_dir().join(format!("rake-test-{}.sb3", std::process::id()));
        write_sb3(&path, &project, &assets).expect("write archive");
        let file = File::open(&path).expect("open archive");
        let mut archive = ZipArchive::new(file).expect("read zip");
        let mut text = String::new();
        archive
            .by_name("project.json")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        let saved: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(saved["meta"]["agent"], "Rake compiler");
        assert_eq!(archive.len(), assets.len() + 1);
        drop(archive);
        fs::remove_file(path).unwrap();
    }
}
