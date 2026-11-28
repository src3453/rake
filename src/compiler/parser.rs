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
    EnumValue {
        enum_id: u32,
        enum_name: String,
        variant_name: String,
        value: i64,
    },
    Match {
        scrutinee: Box<Expr>,
        arms: Vec<MatchArm>,
    },
    /// `text[index]`: the character at a 0-based index.
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
    },
    /// A type conversion call such as `int(x)` or `text(x)`, resolved by the type checker.
    Convert {
        target: Type,
        operand: Box<Expr>,
    },
}

/// The scalar type a conversion function name such as `int` or `text` converts to.
fn conversion_target(name: &str) -> Option<Type> {
    match name {
        "int" => Some(Type::Int),
        "float" => Some(Type::Float),
        "bool" => Some(Type::Bool),
        "text" => Some(Type::Text),
        _ => None,
    }
}

#[derive(Clone, Debug)]
enum MatchPattern {
    Wildcard(Token),
    Value(Expr),
}

#[derive(Clone, Debug)]
enum MatchArmBody {
    Expression(Expr),
    Block(Vec<Stmt>),
}

#[derive(Clone, Debug)]
struct MatchArm {
    token: Token,
    pattern: MatchPattern,
    body: MatchArmBody,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum EventKind {
    GreenFlag,
    KeyPressed(String),
    SpriteClicked,
    CloneStart,
    BackdropSwitchedTo(String),
    Message(String),
}

#[derive(Clone, Debug)]
enum Stmt {
    Use {
        token: Token,
        /// The imported namespace or symbol path. Empty for `from "..." use *`.
        namespace: String,
        alias: Option<String>,
        alias_token: Option<Token>,
        members: Vec<(Token, String)>,
        scope: String,
        /// `use Path.*` / `from "..." use *`: import every direct member.
        glob: bool,
        /// The library module named by a `from "path"` clause, if any.
        module: Option<ModuleImport>,
    },
    Namespace {
        name_token: Token,
        path: String,
        body: Vec<Stmt>,
    },
    Enum {
        name_token: Token,
        name: String,
        id: u32,
        variants: Vec<EnumVariantInfo>,
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
        has_parentheses: bool,
        kind: Option<EventKind>,
        body: Vec<Stmt>,
    },
    Send {
        message_token: Token,
        message: String,
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
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
    },
}

/// A token that locates a statement in diagnostics.
fn statement_token(statement: &Stmt) -> Token {
    match statement {
        Stmt::Use { token, .. }
        | Stmt::Call { token, .. }
        | Stmt::Return { token, .. }
        | Stmt::VarDecl { token, .. }
        | Stmt::Assign { token, .. }
        | Stmt::MemberAssign { token, .. }
        | Stmt::While { token, .. } => token.clone(),
        Stmt::Namespace { name_token, .. }
        | Stmt::Enum { name_token, .. }
        | Stmt::Class { name_token, .. }
        | Stmt::Function { name_token, .. } => name_token.clone(),
        Stmt::Event { type_token, .. } => type_token.clone(),
        Stmt::Send { message_token, .. } => message_token.clone(),
        Stmt::If { condition, .. } => condition.token.clone(),
        Stmt::Match { scrutinee, .. } => scrutinee.token.clone(),
    }
}

/// The module ID of a `from` import that editor analysis could not load.
const UNRESOLVED_MODULE: u32 = u32::MAX;

/// The `from "path"` clause of a `use` declaration.
#[derive(Clone, Debug)]
struct ModuleImport {
    token: Token,
    /// The path exactly as written, relative to the project root.
    path: String,
    /// The loaded module ID.
    id: u32,
}

/// One parser is shared by every module of a program so class and enum IDs stay
/// unique; `use` aliases and visibility are reset for each module.
struct Parser {
    tokens: Vec<Token>,
    index: usize,
    namespace: Vec<String>,
    type_aliases: HashMap<String, Type>,
    class_ids: HashMap<String, u32>,
    next_class_id: u32,
    enum_ids: HashMap<String, u32>,
    next_enum_id: u32,
    namespace_aliases: HashMap<String, String>,
    /// The module that declared each class and enum (by qualified name).
    type_files: HashMap<String, u32>,
    /// The module being parsed and the modules it may reference (itself plus
    /// every module it imports directly with `from`).
    current_file: u32,
    visible_files: std::collections::HashSet<u32>,
    /// Module IDs for the `from` paths of the module being parsed.
    module_ids: HashMap<String, u32>,
    /// Editor analysis: accept `from` imports that were not loaded.
    lenient_imports: bool,
}

/// Expand the initial C-style token macros before parsing. Macro bodies are
/// token sequences; parameters are substituted without interpreting strings.
pub(crate) fn expand_macros(tokens: Vec<Token>) -> Result<Vec<Token>, String> {
    use std::collections::HashMap;
    type Macro = (Token, Vec<String>, Vec<Token>);
    let mut macros: HashMap<String, Macro> = HashMap::new();
    let mut source = Vec::new();
    let mut i = 0;
    while i + 1 < tokens.len() {
        if tokens[i].text == "macro" && tokens[i + 1].text == "!" {
            let start = tokens[i].clone();
            i += 2;
            let name_token = tokens
                .get(i)
                .ok_or_else(|| diagnostic(&start, "expected macro name"))?;
            if name_token.kind != TokenKind::Identifier {
                return Err(diagnostic(name_token, "expected macro name"));
            }
            let name = name_token.text.clone();
            if macros.contains_key(&name) {
                return Err(diagnostic(
                    name_token,
                    format!("macro '{name}' is already defined"),
                ));
            }
            i += 1;
            if tokens.get(i).map(|t| t.text.as_str()) != Some("(") {
                return Err(diagnostic(&start, "expected '(' after macro name"));
            }
            i += 1;
            let mut params = Vec::new();
            while tokens.get(i).map(|t| t.text.as_str()) != Some(")") {
                let t = tokens
                    .get(i)
                    .ok_or_else(|| diagnostic(&start, "unterminated macro parameters"))?;
                if t.kind != TokenKind::Identifier {
                    return Err(diagnostic(t, "expected macro parameter"));
                }
                if params.contains(&t.text) {
                    return Err(diagnostic(
                        t,
                        format!("duplicate macro parameter '{}'", t.text),
                    ));
                }
                params.push(t.text.clone());
                i += 1;
                if tokens.get(i).map(|t| t.text.as_str()) == Some(",") {
                    i += 1;
                } else if tokens.get(i).map(|t| t.text.as_str()) != Some(")") {
                    return Err(diagnostic(
                        &tokens[i],
                        "expected ',' or ')' in macro parameters",
                    ));
                }
            }
            i += 1;
            if tokens.get(i).map(|t| t.text.as_str()) != Some("{") {
                return Err(diagnostic(&start, "expected '{' before macro body"));
            }
            i += 1;
            let body_start = i;
            let mut depth = 1;
            while i < tokens.len() && depth > 0 {
                if tokens[i].text == "{" {
                    depth += 1;
                } else if tokens[i].text == "}" {
                    depth -= 1;
                }
                i += 1;
            }
            if depth != 0 {
                return Err(diagnostic(&start, "unterminated macro body"));
            }
            macros.insert(name, (start, params, tokens[body_start..i - 1].to_vec()));
        } else {
            source.push(tokens[i].clone());
            i += 1;
        }
    }
    if i < tokens.len() {
        source.push(tokens[i].clone());
    }
    fn expand(
        input: Vec<Token>,
        macros: &HashMap<String, Macro>,
        depth: usize,
    ) -> Result<Vec<Token>, String> {
        if depth > 64 {
            return Err("macro expansion exceeded 64 levels".into());
        }
        let mut out = Vec::new();
        let mut i = 0;
        while i < input.len() {
            if i + 2 < input.len()
                && input[i].kind == TokenKind::Identifier
                && input[i + 1].text == "!"
                && input[i + 2].text == "("
            {
                if let Some((_definition, params, body)) = macros.get(&input[i].text) {
                    let call = input[i].clone();
                    i += 3;
                    let mut args: Vec<Vec<Token>> = vec![Vec::new()];
                    let mut parens = 0;
                    let mut braces = 0;
                    let mut brackets = 0;
                    while i < input.len() {
                        let t = &input[i];
                        if t.text == ")" && parens == 0 && braces == 0 && brackets == 0 {
                            break;
                        }
                        if t.text == "," && parens == 0 && braces == 0 && brackets == 0 {
                            args.push(Vec::new());
                        } else {
                            match t.text.as_str() {
                                "(" => parens += 1,
                                ")" => parens -= 1,
                                "{" => braces += 1,
                                "}" => braces -= 1,
                                "[" => brackets += 1,
                                "]" => brackets -= 1,
                                _ => {}
                            }
                            args.last_mut().unwrap().push(t.clone());
                        }
                        i += 1;
                    }
                    if i == input.len() {
                        return Err(diagnostic(&call, "unterminated macro invocation"));
                    }
                    i += 1;
                    if args.len() == 1 && args[0].is_empty() {
                        args.clear();
                    }
                    if args.len() != params.len() {
                        return Err(diagnostic(
                            &call,
                            format!(
                                "macro expects {} arguments, got {}",
                                params.len(),
                                args.len()
                            ),
                        ));
                    }
                    let map: HashMap<_, _> = params.iter().cloned().zip(args).collect();
                    let mut substituted = Vec::new();
                    for token in body {
                        if token.kind == TokenKind::Identifier {
                            if let Some(arg) = map.get(&token.text) {
                                substituted.extend(arg.clone());
                                continue;
                            }
                        }
                        let mut t = token.clone();
                        t.line = call.line;
                        t.column = call.column;
                        substituted.push(t);
                    }
                    let expanded = expand(substituted, macros, depth + 1)?;
                    out.extend(expanded);
                    continue;
                }
            }
            out.push(input[i].clone());
            i += 1;
        }
        Ok(out)
    }
    let mut expanded = expand(source, &macros, 0)?;
    if let Some(eof) = tokens.last().filter(|t| t.kind == TokenKind::Eof) {
        expanded.push(eof.clone());
    }
    Ok(expanded)
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
            enum_ids: HashMap::new(),
            next_enum_id: 0,
            namespace_aliases: HashMap::new(),
            type_files: HashMap::new(),
            current_file: 0,
            visible_files: std::collections::HashSet::from([0]),
            module_ids: HashMap::new(),
            lenient_imports: false,
        }
    }

    /// Parse one module of a multi-file program. Modules must be parsed after the
    /// modules they import so their classes and enums are already known.
    fn parse_module(
        &mut self,
        tokens: Vec<Token>,
        file: u32,
        module_ids: HashMap<String, u32>,
    ) -> Result<Vec<Stmt>, String> {
        self.tokens = tokens;
        self.index = 0;
        self.namespace.clear();
        self.type_aliases.clear();
        self.namespace_aliases.clear();
        self.current_file = file;
        self.visible_files = module_ids.values().copied().chain([file]).collect();
        self.module_ids = module_ids;
        let statements = self.parse()?;
        if file != 0 {
            for statement in &statements {
                let allowed = matches!(
                    statement,
                    Stmt::Use { .. }
                        | Stmt::Namespace { .. }
                        | Stmt::Class { .. }
                        | Stmt::Enum { .. }
                        | Stmt::Function { .. }
                );
                if !allowed {
                    let token = statement_token(statement);
                    return Err(diagnostic(
                        &token,
                        "library modules may contain only use, namespace, class, enum, and function declarations",
                    ));
                }
            }
        }
        Ok(statements)
    }

    /// A class or enum declared in a module the current module can see.
    fn visible_type(&self, ids: &HashMap<String, u32>, name: &str) -> Option<u32> {
        let id = ids.get(name)?;
        let file = self.type_files.get(name).copied().unwrap_or(0);
        self.visible_files.contains(&file).then_some(*id)
    }

    fn visible_class(&self, name: &str) -> Option<u32> {
        self.visible_type(&self.class_ids, name)
    }

    fn visible_enum(&self, name: &str) -> Option<u32> {
        self.visible_type(&self.enum_ids, name)
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
            if let Some(class_id) = self.visible_class(&candidate) {
                return Ok(Type::Class(class_id));
            }
            if let Some(enum_id) = self.visible_enum(&candidate) {
                return Ok(Type::Enum(enum_id));
            }
            if let Some((alias, suffix)) = path.split_once('.') {
                let alias_key = format!("{prefix}::{alias}");
                if let Some(namespace) = self.namespace_aliases.get(&alias_key) {
                    let target = format!("{namespace}.{suffix}");
                    if let Some(class_id) = self.visible_class(&target) {
                        return Ok(Type::Class(class_id));
                    }
                    if let Some(enum_id) = self.visible_enum(&target) {
                        return Ok(Type::Enum(enum_id));
                    }
                }
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
            if self.at("use") || self.at_module_import() {
                statements.push(self.use_declaration()?);
            } else if self.at("ns") {
                statements.push(self.namespace_declaration()?);
            } else if self.at("class") {
                statements.push(self.class_declaration()?);
            } else if self.at("enum") {
                statements.push(self.enum_declaration()?);
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

    /// `from "path" use ...` at the start of a statement. `from` is only a keyword
    /// here, so it stays usable as an identifier (for example a named argument).
    fn at_module_import(&self) -> bool {
        self.at("from")
            && self.current().kind == TokenKind::Identifier
            && self
                .tokens
                .get(self.index + 1)
                .is_some_and(|token| token.kind == TokenKind::String)
    }

    fn use_member_list(&mut self, members: &mut Vec<(Token, String)>) -> Result<(), String> {
        self.expect("{")?;
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
        Ok(())
    }

    fn use_declaration(&mut self) -> Result<Stmt, String> {
        let module = if self.at_module_import() {
            self.take();
            let token = self.take();
            let path = token.text.clone();
            let id = self
                .module_ids
                .get(&path)
                .copied()
                .or(self.lenient_imports.then_some(UNRESOLVED_MODULE));
            let Some(id) = id else {
                return Err(diagnostic(
                    &token,
                    format!(
                        "cannot load library {path:?}: `from` imports need the source file's location; compile the file from disk inside a project"
                    ),
                ));
            };
            Some(ModuleImport { token, path, id })
        } else {
            None
        };
        let token = self.expect("use")?;
        let mut members = Vec::new();
        let mut alias = None;
        let mut alias_token = None;
        let mut glob = false;
        let mut path = String::new();
        if self.at("*") {
            let star = self.take();
            if module.is_none() {
                return Err(diagnostic(
                    &star,
                    "`use *` needs a library: write `from \"path\" use *`, or `use Namespace.*`",
                ));
            }
            glob = true;
        } else {
            path = self.expect_identifier()?.text;
            while self.at(".") {
                self.take();
                if self.at("{") {
                    self.use_member_list(&mut members)?;
                    break;
                }
                if self.at("*") {
                    self.take();
                    glob = true;
                    break;
                }
                path.push('.');
                path.push_str(&self.expect_identifier()?.text);
            }
            if self.at("as") {
                let as_token = self.take();
                if glob {
                    return Err(diagnostic(&as_token, "a `*` import cannot have an alias"));
                }
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
                    self.use_member_list(&mut members)?;
                }
            }
            if self.at("as") {
                self.take();
                let name = self.expect_identifier()?;
                alias = Some(name.text.clone());
                alias_token = Some(name);
            }
        }
        let scope = self.namespace_path();
        if glob {
            self.register_glob_type_aliases(&scope, &path, module.as_ref().map(|module| module.id));
        } else if members.is_empty() {
            let local = alias
                .clone()
                .unwrap_or_else(|| path.rsplit('.').next().unwrap_or(&path).to_string());
            if let Some(ty) = self.imported_type(&path) {
                self.type_aliases.insert(format!("{scope}::{local}"), ty);
            }
            if let Some(alias) = &alias {
                self.namespace_aliases
                    .insert(format!("{scope}::{alias}"), path.clone());
            }
        } else {
            for (_, member) in &members {
                let target = format!("{path}.{member}");
                if let Some(ty) = self.imported_type(&target) {
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
            glob,
            module,
        })
    }

    /// The type a `use` path names, if it is a built-in type or a visible class or enum.
    fn imported_type(&self, path: &str) -> Option<Type> {
        builtin_type_path(path)
            .or_else(|| self.visible_class(path).map(Type::Class))
            .or_else(|| self.visible_enum(path).map(Type::Enum))
    }

    /// Make every class and enum directly inside `path` usable by its leaf name.
    /// With an empty path (`from "lib" use *`), that is the top level of `module`.
    fn register_glob_type_aliases(&mut self, scope: &str, path: &str, module: Option<u32>) {
        let prefix = if path.is_empty() {
            String::new()
        } else {
            format!("{path}.")
        };
        let mut additions = Vec::new();
        for builtin in ["Scratch.Generics.Value", "Scratch.Generics.List"] {
            if let Some(leaf) = builtin.strip_prefix(&prefix).filter(|leaf| !leaf.contains('.')) {
                if let Some(ty) = builtin_type_path(builtin) {
                    additions.push((leaf.to_string(), ty));
                }
            }
        }
        let mut child_namespaces = std::collections::BTreeSet::new();
        for (name, file) in &self.type_files {
            if path.is_empty() && Some(*file) != module {
                continue;
            }
            let Some(rest) = name.strip_prefix(&prefix) else {
                continue;
            };
            if let Some((child, _)) = rest.split_once('.') {
                child_namespaces.insert(child.to_string());
            } else if let Some(ty) = self.imported_type(name) {
                additions.push((rest.to_string(), ty));
            }
        }
        for (leaf, ty) in additions {
            self.type_aliases.insert(format!("{scope}::{leaf}"), ty);
        }
        for child in child_namespaces {
            self.namespace_aliases
                .insert(format!("{scope}::{child}"), format!("{prefix}{child}"));
        }
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
            if self.at_module_import() {
                return Err(diagnostic(
                    self.current(),
                    "`from \"...\" use` must be declared at the top level of a file",
                ));
            }
            if self.at("use") {
                body.push(self.use_declaration()?);
            } else if self.at("ns") {
                body.push(self.namespace_declaration()?);
            } else if self.at("class") {
                body.push(self.class_declaration()?);
            } else if self.at("enum") {
                body.push(self.enum_declaration()?);
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
                    "namespace bodies may contain only use, namespace, class, enum, and function declarations",
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
        self.type_files.insert(name.clone(), self.current_file);
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

    fn enum_declaration(&mut self) -> Result<Stmt, String> {
        self.expect("enum")?;
        let name_token = self.expect_identifier()?;
        let name = self.qualified_name(&name_token.text);
        let id = self.next_enum_id;
        self.next_enum_id += 1;
        self.enum_ids.insert(name.clone(), id);
        self.type_files.insert(name.clone(), self.current_file);
        self.expect("{")?;
        let mut variants = Vec::new();
        if !self.at("}") {
            loop {
                let token = self.expect_identifier()?;
                variants.push(EnumVariantInfo {
                    name: token.text.clone(),
                    token,
                    value: variants.len() as i64,
                });
                if !self.at(",") {
                    break;
                }
                self.take();
                if self.at("}") {
                    break;
                }
            }
        }
        self.expect("}")?;
        if variants.is_empty() {
            return Err(diagnostic(
                &name_token,
                "an enum must declare at least one variant",
            ));
        }
        Ok(Stmt::Enum {
            name_token,
            name,
            id,
            variants,
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
        let mut type_token = self.expect_identifier()?;
        let mut event_type = type_token.text.clone();
        while self.at(".") {
            self.take();
            type_token = self.expect_identifier()?;
            event_type.push('.');
            event_type.push_str(&type_token.text);
        }
        let has_parentheses = self.at("(");
        let mut values = Vec::new();
        if has_parentheses {
            self.take();
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
        }
        let body = self.block()?;
        Ok(Stmt::Event {
            type_token,
            event_type,
            values,
            has_parentheses,
            kind: None,
            body,
        })
    }

    fn statement(&mut self) -> Result<Stmt, String> {
        if self.at("match") {
            let (_, scrutinee, arms) = self.match_expression_parts()?;
            return Ok(Stmt::Match {
                scrutinee,
                arms,
            });
        }
        if self.at("send") {
            self.take();
            let message_token = self.expect_identifier()?;
            let message = message_token.text.clone();
            return Ok(Stmt::Send {
                message_token,
                message,
            });
        }
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
            && is_assignment_operator(&self.tokens[self.index + 3].text)
        {
            let token = self.take();
            let object = token.text.clone();
            self.take();
            let field_token = self.take();
            let field = field_token.text.clone();
            let operator_token = self.take();
            let operator = operator_token.text.clone();
            let value = self.assignment_value(&operator_token)?;
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
            && is_assignment_operator(&self.tokens[self.index + 1].text)
        {
            let token = self.take();
            let name = token.text.clone();
            let operator_token = self.take();
            let operator = operator_token.text.clone();
            let value = self.assignment_value(&operator_token)?;
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
        if self.current().kind == TokenKind::Identifier && self.tokens[self.index + 1].text == "[" {
            return Err(diagnostic(
                &self.tokens[self.index + 1],
                "text characters cannot be assigned; build a new text value instead",
            ));
        }
        Err(diagnostic(
            self.current(),
            "expected a variable declaration, assignment, function call, send, if, while, or match",
        ))
    }

    /// The right-hand side of an assignment. `x++` / `x--` have none, so they
    /// assign `x + 1` / `x - 1` through an implicit literal `1`.
    fn assignment_value(&mut self, operator: &Token) -> Result<Expr, String> {
        if operator.text == "++" || operator.text == "--" {
            return Ok(Expr {
                kind: ExprKind::Number("1".to_string()),
                token: operator.clone(),
                ty: Some(Type::Int),
            });
        }
        self.expression(1)
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

    fn match_expression_parts(&mut self) -> Result<(Token, Expr, Vec<MatchArm>), String> {
        let token = self.expect("match")?;
        let scrutinee = self.expression(1)?;
        self.expect("{")?;
        let mut arms = Vec::new();
        self.semicolons();
        while !self.at("}") {
            if self.current().kind == TokenKind::Eof {
                return Err(diagnostic(self.current(), "expected '}' before end of file"));
            }
            let arm_token = self.current().clone();
            let pattern = if self.current().kind == TokenKind::Identifier && self.at("_") {
                MatchPattern::Wildcard(self.take())
            } else {
                MatchPattern::Value(self.expression(1)?)
            };
            self.expect("=>")?;
            let body = if self.at("{") {
                MatchArmBody::Block(self.block()?)
            } else {
                MatchArmBody::Expression(self.expression(1)?)
            };
            arms.push(MatchArm {
                token: arm_token,
                pattern,
                body,
            });
            if self.at(",") {
                self.take();
                self.semicolons();
            } else if !self.at("}") {
                return Err(diagnostic(self.current(), "expected ',' or '}' after match arm"));
            }
        }
        self.take();
        if arms.is_empty() {
            return Err(diagnostic(&token, "match must contain at least one arm"));
        }
        Ok((token, scrutinee, arms))
    }

    fn precedence(operator: &str) -> Option<u8> {
        match operator {
            "||" => Some(1),
            "&&" => Some(2),
            "==" | "!=" => Some(3),
            "<" | "<=" | ">" | ">=" => Some(4),
            "+" | "-" => Some(5),
            "*" | "/" | "%" => Some(6),
            _ => None,
        }
    }

    fn expression(&mut self, minimum: u8) -> Result<Expr, String> {
        let mut left = self.prefix()?;
        while let Some(precedence) = (self.current().kind == TokenKind::Symbol)
            .then(|| Self::precedence(&self.current().text))
            .flatten()
        {
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
        if token.text == "match" {
            let (token, scrutinee, arms) = self.match_expression_parts()?;
            return Ok(Expr {
                kind: ExprKind::Match {
                    scrutinee: Box::new(scrutinee),
                    arms,
                },
                token,
                ty: None,
            });
        }
        if token.kind == TokenKind::Symbol && (token.text == "!" || token.text == "-") {
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
        if token.kind == TokenKind::Symbol && token.text == "(" {
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
                self.postfix(Expr {
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
        while self.current().kind == TokenKind::Symbol && (self.at(".") || self.at("[")) {
            if self.at("[") {
                let token = self.take();
                let index = self.expression(1)?;
                self.expect("]")?;
                expression = Expr {
                    kind: ExprKind::Index {
                        object: Box::new(expression),
                        index: Box::new(index),
                    },
                    token,
                    ty: None,
                };
                continue;
            }
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

/// Statement operators that assign to a variable or field.
fn is_assignment_operator(operator: &str) -> bool {
    matches!(
        operator,
        "=" | "+=" | "-=" | "*=" | "/=" | "%=" | "++" | "--"
    )
}

/// The binary operator a compound assignment applies, e.g. `+` for `+=` and `++`.
fn compound_assignment_operator(operator: &str) -> Option<&'static str> {
    match operator {
        "+=" | "++" => Some("+"),
        "-=" | "--" => Some("-"),
        "*=" => Some("*"),
        "/=" => Some("/"),
        "%=" => Some("%"),
        _ => None,
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
