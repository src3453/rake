// Experimental Scratch 3 (`.sb3`) → Rake decompiler.
//
// The decompiler turns an existing project into a Rake project directory: one `.rks`
// scene, one `.rk` source per target, and the costume and sound files. It maps blocks
// back through the same built-in catalog the compiler uses, so every emitted call is
// one the compiler accepts. Blocks Rake cannot express yet are kept as comments.

use std::collections::{BTreeSet, HashSet};

const DECOMPILE_RESERVED: &[&str] = &[
    "var", "const", "fn", "warp", "if", "else", "while", "return", "match", "send", "event", "use",
    "ns", "class", "enum", "macro", "public", "private", "self", "true", "false", "as", "new", "int",
    "float", "bool", "text", "Scratch", "Motion", "Looks", "Sound", "Events", "Control", "Sensing",
    "Operators", "SList", "Stage",
];

/// One file of a decompiled project, relative to the output directory.
#[derive(Clone, Debug)]
pub struct DecompiledFile {
    pub path: String,
    pub contents: Vec<u8>,
}

/// A decompiled project: the files to write and what could not be translated exactly.
#[derive(Clone, Debug)]
pub struct Decompilation {
    pub files: Vec<DecompiledFile>,
    pub scene_path: String,
    pub warnings: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DTy {
    Int,
    Float,
    Bool,
    Text,
    Value,
}

impl DTy {
    fn numeric(self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }

    fn from_type(ty: Type) -> Self {
        match ty {
            Type::Int => Self::Int,
            Type::Float => Self::Float,
            Type::Bool => Self::Bool,
            Type::Text => Self::Text,
            _ => Self::Value,
        }
    }
}

/// A rendered Rake expression with its static type and binding strength (10 = atom).
#[derive(Clone, Debug)]
struct DExpr {
    text: String,
    ty: DTy,
    prec: u8,
}

impl DExpr {
    fn atom(text: impl Into<String>, ty: DTy) -> Self {
        Self { text: text.into(), ty, prec: 10 }
    }

    fn text_literal(value: &str) -> Self {
        Self::atom(rake_string(value), DTy::Text)
    }
}

fn rake_string(value: &str) -> String {
    let mut out = String::from("\"");
    for ch in value.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            other => out.push(other),
        }
    }
    out.push('"');
    out
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        format!("{value}")
    }
}

/// A Scratch number as a Rake literal, or `None` when it is not a finite number.
fn number_literal(raw: &str) -> Option<DExpr> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return None;
    }
    let value: f64 = trimmed.parse().ok()?;
    if !value.is_finite() {
        return None;
    }
    let ty = if value.fract() == 0.0 && value.abs() < 1e15 { DTy::Int } else { DTy::Float };
    let text = format_number(if value == 0.0 { 0.0 } else { value });
    let prec = if text.starts_with('-') { 9 } else { 10 };
    Some(DExpr { text, ty, prec })
}

/// A Rake identifier derived from a Scratch name. Rake identifiers are ASCII only.
fn rake_identifier(raw: &str, fallback: &str) -> String {
    let mut out = String::new();
    for ch in raw.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch);
        } else if !out.is_empty() && !out.ends_with('_') {
            out.push('_');
        }
    }
    while out.ends_with('_') {
        out.pop();
    }
    if out.is_empty() {
        return fallback.to_string();
    }
    if out.starts_with(|ch: char| ch.is_ascii_digit()) {
        out = format!("{fallback}_{out}");
    }
    if DECOMPILE_RESERVED.contains(&out.as_str()) {
        out.push('_');
    }
    out
}

/// Turn a custom block's proccode (`"move player %s steps"`) into `movePlayerSteps`.
fn procedure_identifier(proccode: &str) -> String {
    let mut out = String::new();
    for word in proccode
        .split(|ch: char| !ch.is_ascii_alphanumeric() && ch != '%')
        .filter(|word| !word.is_empty() && !matches!(*word, "%s" | "%b" | "%n"))
    {
        let word: String = word.chars().filter(|ch| ch.is_ascii_alphanumeric()).collect();
        if word.is_empty() {
            continue;
        }
        if out.is_empty() {
            out.push_str(&word);
        } else {
            let mut chars = word.chars();
            if let Some(first) = chars.next() {
                out.push(first.to_ascii_uppercase());
                out.extend(chars);
            }
        }
    }
    rake_identifier(&out, "customBlock")
}

fn unique_name(base: &str, taken: &mut HashSet<String>) -> String {
    let mut candidate = base.to_string();
    let mut suffix = 2;
    while taken.contains(&candidate) || taken.contains(&candidate.to_ascii_lowercase()) {
        candidate = format!("{base}_{suffix}");
        suffix += 1;
    }
    taken.insert(candidate.clone());
    candidate
}

/// A file name that is valid on every platform, keeping the Scratch name where possible.
fn safe_file_stem(raw: &str, fallback: &str) -> String {
    let mut out: String = raw
        .chars()
        .map(|ch| if ch.is_control() || "<>:\"/\\|?*".contains(ch) { '_' } else { ch })
        .collect();
    while out.ends_with('.') || out.ends_with(' ') {
        out.pop();
    }
    let out = out.trim_start().to_string();
    if out.is_empty() { fallback.to_string() } else { out }
}

fn binary_precedence(operator: &str) -> u8 {
    match operator {
        "||" => 1,
        "&&" => 2,
        "==" | "!=" => 3,
        "<" | "<=" | ">" | ">=" => 4,
        "+" | "-" => 5,
        _ => 6,
    }
}

fn make_binary_text(operator: &str, left: DExpr, right: DExpr, ty: DTy) -> DExpr {
    let prec = binary_precedence(operator);
    let left = if left.prec < prec { format!("({})", left.text) } else { left.text };
    let right = if right.prec <= prec { format!("({})", right.text) } else { right.text };
    DExpr { text: format!("{left} {operator} {right}"), ty, prec }
}

fn make_not_text(operand: DExpr) -> DExpr {
    let inner = if operand.prec < 9 { format!("({})", operand.text) } else { operand.text };
    DExpr { text: format!("!{inner}"), ty: DTy::Bool, prec: 9 }
}

fn field_text<'v>(block: &'v Value, field: &str) -> Option<&'v str> {
    block.get("fields")?.get(field)?.get(0)?.as_str()
}

fn field_ref(block: &Value, field: &str) -> Option<(String, Option<String>)> {
    let entry = block.get("fields")?.get(field)?;
    let name = match entry.get(0)? {
        Value::String(text) => text.clone(),
        other => other.to_string(),
    };
    let id = entry.get(1).and_then(Value::as_str).map(str::to_string);
    Some((name, id))
}

fn json_scalar_text(value: &Value) -> String {
    match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        other => other.to_string(),
    }
}

#[derive(Clone, Debug)]
struct DecompiledVariable {
    rake: String,
    scratch: String,
    initial: Value,
    owner: Option<String>,
    cloud: bool,
}

#[derive(Clone, Debug)]
struct DecompiledList {
    rake: String,
    scratch: String,
    items: usize,
    owner: Option<String>,
}

#[derive(Clone, Debug)]
struct DecompiledProcedure {
    name: String,
    warp: bool,
    // (argument id, Scratch argument name, Rake parameter name, is boolean)
    parameters: Vec<(String, String, String, bool)>,
}

struct DecompileTarget {
    rake_name: String,
    scratch_name: String,
    is_stage: bool,
    blocks: Map<String, Value>,
    var_ids: HashMap<String, String>,
    var_names: HashMap<String, String>,
    list_ids: HashMap<String, String>,
    list_names: HashMap<String, String>,
    procedures: HashMap<String, DecompiledProcedure>,
    taken: HashSet<String>,
}

struct DecompileProject {
    builtins: Vec<BuiltinBlock>,
    messages: HashMap<String, String>,
    sprite_names: HashMap<String, String>,
    variables: HashMap<String, DecompiledVariable>,
    lists: HashMap<String, DecompiledList>,
    stage_var_ids: HashMap<String, String>,
    stage_var_names: HashMap<String, String>,
    stage_list_ids: HashMap<String, String>,
    stage_list_names: HashMap<String, String>,
}

const SPRITE_MENU_OPCODES: &[&str] = &[
    "motion_goto_menu",
    "motion_glideto_menu",
    "motion_pointtowards_menu",
    "control_create_clone_of_menu",
    "sensing_touchingobjectmenu",
    "sensing_distancetomenu",
    "sensing_of_object_menu",
];

/// Rendering state for one target. `types` holds the inferred type of each variable.
struct Renderer<'a> {
    project: &'a DecompileProject,
    target: &'a DecompileTarget,
    types: &'a HashMap<String, DTy>,
    notes: Vec<String>,
    warnings: Vec<String>,
    used_vars: BTreeSet<String>,
    used_lists: BTreeSet<String>,
    used_namespaces: BTreeSet<String>,
    arguments: HashMap<String, (String, bool)>,
    taken: HashSet<String>,
}

impl<'a> Renderer<'a> {
    fn new(
        project: &'a DecompileProject,
        target: &'a DecompileTarget,
        types: &'a HashMap<String, DTy>,
    ) -> Self {
        Self {
            project,
            target,
            types,
            notes: Vec::new(),
            warnings: Vec::new(),
            used_vars: BTreeSet::new(),
            used_lists: BTreeSet::new(),
            used_namespaces: BTreeSet::new(),
            arguments: HashMap::new(),
            taken: target.taken.clone(),
        }
    }

    fn block(&self, id: &str) -> Option<&'a Value> {
        self.target.blocks.get(id).filter(|block| block.is_object())
    }

    fn note(&mut self, message: String) {
        self.warnings.push(format!("{}: {message}", self.target.scratch_name));
        self.notes.push(message);
    }

    fn flush_notes(&mut self, indent: usize, out: &mut Vec<String>) {
        for note in self.notes.drain(..) {
            out.push(format!("{}// [decompile] {note}", "    ".repeat(indent)));
        }
    }

    fn lookup_variable(&self, name: &str, id: Option<&str>) -> Option<String> {
        id.and_then(|id| self.target.var_ids.get(id).or_else(|| self.project.stage_var_ids.get(id)))
            .or_else(|| self.target.var_names.get(name))
            .or_else(|| self.project.stage_var_names.get(name))
            .cloned()
    }

    fn lookup_list(&self, name: &str, id: Option<&str>) -> Option<String> {
        id.and_then(|id| self.target.list_ids.get(id).or_else(|| self.project.stage_list_ids.get(id)))
            .or_else(|| self.target.list_names.get(name))
            .or_else(|| self.project.stage_list_names.get(name))
            .cloned()
    }

    fn variable_expr(&mut self, name: &str, id: Option<&str>) -> DExpr {
        match self.lookup_variable(name, id) {
            Some(key) => {
                let ty = self.types.get(&key).copied().unwrap_or(DTy::Value);
                self.used_vars.insert(key.clone());
                DExpr::atom(self.project.variables[&key].rake.clone(), ty)
            }
            None => {
                self.note(format!("unknown variable {name:?} replaced with \"\""));
                DExpr::text_literal("")
            }
        }
    }

    fn list_name(&mut self, name: &str, id: Option<&str>) -> Option<String> {
        let key = self.lookup_list(name, id)?;
        self.used_lists.insert(key.clone());
        Some(self.project.lists[&key].rake.clone())
    }

    fn map_menu_value(&self, menu_opcode: &str, value: &str) -> String {
        if menu_opcode == "event_broadcast_menu" {
            return self.project.messages.get(value).cloned().unwrap_or_else(|| value.to_string());
        }
        if SPRITE_MENU_OPCODES.contains(&menu_opcode) {
            if let Some(name) = self.project.sprite_names.get(value) {
                return name.clone();
            }
        }
        value.to_string()
    }

    /// The literal a shadow block holds (numbers, text, colors, and menu options).
    fn shadow_literal(&self, block: &Value) -> DExpr {
        let opcode = block["opcode"].as_str().unwrap_or_default();
        let raw = block
            .get("fields")
            .and_then(Value::as_object)
            .and_then(|fields| fields.values().next())
            .and_then(|field| field.get(0))
            .map(json_scalar_text)
            .unwrap_or_default();
        if opcode.starts_with("math_") {
            return number_literal(&raw).unwrap_or_else(|| DExpr::text_literal(&raw));
        }
        DExpr::text_literal(&self.map_menu_value(opcode, &raw))
    }

    fn primitive(&mut self, items: &[Value]) -> Option<DExpr> {
        let code = items.first()?.as_i64()?;
        let raw = items.get(1).map(json_scalar_text).unwrap_or_default();
        Some(match code {
            4..=8 => number_literal(&raw).unwrap_or_else(|| DExpr::text_literal(&raw)),
            // Numbers typed into text slots stay numbers when they read back the same.
            10 => number_literal(&raw)
                .filter(|literal| literal.text == raw)
                .unwrap_or_else(|| DExpr::text_literal(&raw)),
            9 => DExpr::text_literal(&raw),
            11 => DExpr::text_literal(&self.map_menu_value("event_broadcast_menu", &raw)),
            12 => {
                let id = items.get(2).and_then(Value::as_str);
                self.variable_expr(&raw, id)
            }
            13 => {
                self.note(format!("list reporter {raw:?} is not supported yet; replaced with \"\""));
                DExpr::text_literal("")
            }
            _ => return None,
        })
    }

    fn slot(&mut self, value: &Value) -> Option<DExpr> {
        match value {
            Value::String(id) => {
                let block = self.block(id)?;
                if block["shadow"] == json!(true) {
                    Some(self.shadow_literal(block))
                } else {
                    Some(self.reporter(id))
                }
            }
            Value::Array(items) => self.primitive(items),
            _ => None,
        }
    }

    /// The (main, shadow) parts of a serialized input.
    fn input_parts(block: &'a Value, name: &str) -> (Option<&'a Value>, Option<&'a Value>) {
        let Some(input) = block.get("inputs").and_then(|inputs| inputs.get(name)).and_then(Value::as_array) else {
            return (None, None);
        };
        let main = input.get(1).filter(|value| !value.is_null());
        let shadow = input.get(2).filter(|value| !value.is_null());
        (main, shadow)
    }

    fn value_input(&mut self, block: &'a Value, name: &str) -> DExpr {
        let (main, shadow) = Self::input_parts(block, name);
        main.and_then(|value| self.slot(value))
            .or_else(|| shadow.and_then(|value| self.slot(value)))
            .unwrap_or_else(|| DExpr::text_literal(""))
    }

    fn bool_input(&mut self, block: &'a Value, name: &str) -> DExpr {
        let (main, _) = Self::input_parts(block, name);
        let Some(expr) = main.and_then(|value| self.slot(value)) else {
            return DExpr::atom("false", DTy::Bool);
        };
        if expr.ty == DTy::Bool {
            expr
        } else {
            self.note(format!("non-boolean value {} in a condition replaced with false", expr.text));
            DExpr::atom("false", DTy::Bool)
        }
    }

    fn find_builtin(&self, block: &Value) -> Option<&'a BuiltinBlock> {
        let opcode = block["opcode"].as_str()?;
        self.project.builtins.iter().find(|builtin| {
            builtin.opcode == opcode
                && !builtin.description.starts_with("Alias")
                && builtin.fields.iter().all(|(field, expected)| {
                    field_text(block, field).is_some_and(|actual| actual.eq_ignore_ascii_case(expected))
                })
        })
    }

    /// Render a catalog block as `Namespace.member(...)` or `list.member(...)`.
    fn builtin_call(&mut self, builtin: &BuiltinBlock, block: &'a Value) -> Option<String> {
        let mut receiver = None;
        let mut arguments = Vec::new();
        for spec in &builtin.arguments {
            match spec.kind {
                BuiltinInputKind::List => {
                    let (name, id) = field_ref(block, "LIST")?;
                    match self.list_name(&name, id.as_deref()) {
                        Some(list) => receiver = Some(list),
                        None => {
                            self.note(format!("unknown list {name:?}"));
                            return None;
                        }
                    }
                }
                _ if spec.menu.is_some() => {
                    let menu = spec.menu.as_ref().unwrap();
                    let (main, shadow) = Self::input_parts(block, spec.input);
                    let reporter = main
                        .and_then(Value::as_str)
                        .filter(|id| self.block(id).is_some_and(|inner| inner["shadow"] != json!(true)));
                    let expr = if let Some(id) = reporter {
                        self.reporter(id)
                    } else {
                        main.or(shadow)
                            .and_then(|value| self.slot(value))
                            .unwrap_or_else(|| DExpr::text_literal(&self.map_menu_value(menu.opcode, menu.default)))
                    };
                    arguments.push(expr.text);
                }
                BuiltinInputKind::Boolean => arguments.push(self.bool_input(block, spec.input).text),
                BuiltinInputKind::Color => {
                    let expr = self.value_input(block, spec.input);
                    if expr.ty == DTy::Text {
                        arguments.push(expr.text);
                    } else {
                        self.note(format!("color reporter {} replaced with \"#000000\"", expr.text));
                        arguments.push(rake_string("#000000"));
                    }
                }
                BuiltinInputKind::Note => arguments.push(self.value_input(block, spec.input).text),
                BuiltinInputKind::Value => arguments.push(self.value_input(block, spec.input).text),
            }
        }
        let (namespace, member) = builtin.name.rsplit_once('.')?;
        if let Some(list) = receiver {
            return Some(format!("{list}.{member}({})", arguments.join(", ")));
        }
        if let Some(extension_path) = namespace.strip_prefix("Scratch.Extension.") {
            let alias = extension_path.rsplit('.').next()?;
            self.used_namespaces
                .insert(format!("Extension.{extension_path} as {alias}"));
            return Some(format!("{alias}.{member}({})", arguments.join(", ")));
        }
        let short = namespace.strip_prefix("Scratch.").unwrap_or(namespace).to_string();
        self.used_namespaces.insert(short.clone());
        Some(format!("{short}.{member}({})", arguments.join(", ")))
    }

    fn operators_call(&mut self, member: &str, arguments: &[&DExpr], ty: DTy) -> DExpr {
        self.used_namespaces.insert("Operators".to_string());
        let arguments = arguments.iter().map(|argument| argument.text.as_str()).collect::<Vec<_>>();
        DExpr::atom(format!("Operators.{member}({})", arguments.join(", ")), ty)
    }

    fn equality_is_infix(left: &DExpr, right: &DExpr) -> bool {
        left.ty != DTy::Bool
            && right.ty != DTy::Bool
            && (left.ty == right.ty
                || left.ty == DTy::Value
                || right.ty == DTy::Value
                || (left.ty.numeric() && right.ty.numeric()))
    }

    /// Render a comparison, or `None` when it must use an `Operators` call.
    fn comparison(&mut self, block: &'a Value, negate: bool) -> Option<DExpr> {
        let opcode = block["opcode"].as_str()?;
        let left = self.value_input(block, "OPERAND1");
        let right = self.value_input(block, "OPERAND2");
        let operator = match (opcode, negate) {
            ("operator_equals", _) if !Self::equality_is_infix(&left, &right) => return None,
            ("operator_lt" | "operator_gt", _) if !(left.ty.numeric() && right.ty.numeric()) => return None,
            ("operator_equals", false) => "==",
            ("operator_equals", true) => "!=",
            ("operator_lt", false) => "<",
            ("operator_lt", true) => ">=",
            ("operator_gt", false) => ">",
            ("operator_gt", true) => "<=",
            _ => return None,
        };
        Some(make_binary_text(operator, left, right, DTy::Bool))
    }

    fn reporter(&mut self, id: &str) -> DExpr {
        let Some(block) = self.block(id) else {
            return DExpr::text_literal("");
        };
        let opcode = block["opcode"].as_str().unwrap_or_default();
        match opcode {
            "data_variable" => {
                let (name, var_id) = field_ref(block, "VARIABLE").unwrap_or_default();
                return self.variable_expr(&name, var_id.as_deref());
            }
            "argument_reporter_string_number" | "argument_reporter_boolean" => {
                let name = field_text(block, "VALUE").unwrap_or_default();
                if let Some((rake, is_bool)) = self.arguments.get(name).cloned() {
                    return DExpr::atom(rake, if is_bool { DTy::Bool } else { DTy::Value });
                }
                self.note(format!("argument {name:?} used outside its custom block"));
                return if opcode == "argument_reporter_boolean" {
                    DExpr::atom("false", DTy::Bool)
                } else {
                    DExpr::text_literal("")
                };
            }
            "operator_add" | "operator_subtract" | "operator_multiply" | "operator_divide"
            | "operator_mod" => {
                let left = self.value_input(block, "NUM1");
                let right = self.value_input(block, "NUM2");
                if left.ty.numeric() && right.ty.numeric() {
                    let (operator, ty) = match opcode {
                        "operator_add" => ("+", if left.ty == DTy::Int && right.ty == DTy::Int { DTy::Int } else { DTy::Float }),
                        "operator_subtract" => ("-", if left.ty == DTy::Int && right.ty == DTy::Int { DTy::Int } else { DTy::Float }),
                        "operator_multiply" => ("*", if left.ty == DTy::Int && right.ty == DTy::Int { DTy::Int } else { DTy::Float }),
                        "operator_mod" => ("%", if left.ty == DTy::Int && right.ty == DTy::Int { DTy::Int } else { DTy::Float }),
                        _ => ("/", DTy::Float),
                    };
                    return make_binary_text(operator, left, right, ty);
                }
                let member = match opcode {
                    "operator_add" => "add",
                    "operator_subtract" => "subtract",
                    "operator_multiply" => "multiply",
                    "operator_mod" => "modulo",
                    _ => "divide",
                };
                return self.operators_call(member, &[&left, &right], DTy::Float);
            }
            "operator_letter_of" => {
                let letter = self.value_input(block, "LETTER");
                let string = self.value_input(block, "STRING");
                if letter.ty.numeric() && matches!(string.ty, DTy::Text | DTy::Value) {
                    // Scratch counts letters from 1; Rake indexes text from 0.
                    let index = match letter.text.parse::<i64>() {
                        Ok(value) => format_number((value - 1) as f64),
                        // `s[i]` compiles to letterOf(i + 1, s); undo the `+ 1`.
                        Err(_) => match letter.text.strip_suffix(" + 1") {
                            Some(base) if letter.prec == binary_precedence("+") => base.to_string(),
                            _ => make_binary_text("-", letter.clone(), DExpr::atom("1", DTy::Int), letter.ty).text,
                        },
                    };
                    // Rake indexes require int; decompiled numeric variables are float.
                    let index = if letter.ty == DTy::Int { index } else { format!("int({index})") };
                    let object = if string.prec < 10 { format!("({})", string.text) } else { string.text };
                    return DExpr::atom(format!("{object}[{index}]"), DTy::Text);
                }
                return self.operators_call("letterOf", &[&letter, &string], DTy::Text);
            }
            "operator_equals" => {
                let is_text_literal = |name: &str| {
                    matches!(Self::input_parts(block, name), (Some(Value::Array(items)), _) if items.first() == Some(&json!(10)))
                };
                if is_text_literal("OPERAND1") && is_text_literal("OPERAND2") {
                    let left = self.value_input(block, "OPERAND1");
                    let right = self.value_input(block, "OPERAND2");
                    if left.text == "1" && (right.text == "1" || right.text == "0") {
                        return DExpr::atom(if right.text == "1" { "true" } else { "false" }, DTy::Bool);
                    }
                }
                if let Some(expr) = self.comparison(block, false) {
                    return expr;
                }
            }
            "operator_lt" | "operator_gt" => {
                if let Some(expr) = self.comparison(block, false) {
                    return expr;
                }
            }
            "operator_and" | "operator_or" => {
                let left = self.bool_input(block, "OPERAND1");
                let right = self.bool_input(block, "OPERAND2");
                let operator = if opcode == "operator_and" { "&&" } else { "||" };
                return make_binary_text(operator, left, right, DTy::Bool);
            }
            "operator_not" => {
                let (main, _) = Self::input_parts(block, "OPERAND");
                if let Some(inner) = main.and_then(Value::as_str).and_then(|inner| self.block(inner)) {
                    if matches!(inner["opcode"].as_str(), Some("operator_equals" | "operator_lt" | "operator_gt")) {
                        if let Some(expr) = self.comparison(inner, true) {
                            return expr;
                        }
                    }
                }
                let operand = self.bool_input(block, "OPERAND");
                return make_not_text(operand);
            }
            _ => {}
        }
        if let Some(builtin) = self.find_builtin(block).filter(|builtin| builtin.result.is_some()) {
            let ty = DTy::from_type(builtin.result.unwrap());
            if let Some(text) = self.builtin_call(builtin, block) {
                return DExpr::atom(text, ty);
            }
        }
        self.note(format!("unsupported reporter `{opcode}` replaced with a placeholder"));
        if block.get("inputs").is_some() && opcode.starts_with("operator_") && opcode != "operator_join" {
            DExpr::atom("false", DTy::Bool)
        } else {
            DExpr::text_literal("")
        }
    }

    fn render_stack(&mut self, first: Option<&str>, indent: usize, out: &mut Vec<String>) {
        let mut current = first.map(str::to_string);
        let mut guard = 0;
        while let Some(id) = current {
            guard += 1;
            if guard > 100_000 {
                break;
            }
            let Some(block) = self.block(&id) else { break };
            self.statement(block, indent, out);
            current = block["next"].as_str().map(str::to_string);
        }
    }

    fn substack(&self, block: &Value, name: &str) -> Option<String> {
        block.get("inputs")?.get(name)?.get(1)?.as_str().map(str::to_string)
    }

    fn statement(&mut self, block: &'a Value, indent: usize, out: &mut Vec<String>) {
        let pad = "    ".repeat(indent);
        let opcode = block["opcode"].as_str().unwrap_or_default();
        match opcode {
            "control_if" | "control_if_else" => {
                let condition = self.bool_input(block, "CONDITION");
                self.flush_notes(indent, out);
                out.push(format!("{pad}if {} {{", condition.text));
                let then_first = self.substack(block, "SUBSTACK");
                self.render_stack(then_first.as_deref(), indent + 1, out);
                if opcode == "control_if_else" {
                    out.push(format!("{pad}}} else {{"));
                    let else_first = self.substack(block, "SUBSTACK2");
                    self.render_stack(else_first.as_deref(), indent + 1, out);
                }
                out.push(format!("{pad}}}"));
            }
            "control_forever" | "control_repeat_until" | "control_while" => {
                let condition = match opcode {
                    "control_forever" => DExpr::atom("true", DTy::Bool),
                    "control_while" => self.bool_input(block, "CONDITION"),
                    _ => {
                        let (main, _) = Self::input_parts(block, "CONDITION");
                        let inner = main.and_then(Value::as_str).and_then(|id| self.block(id));
                        match inner {
                            Some(inner) if inner["opcode"] == json!("operator_not") => self.bool_input(inner, "OPERAND"),
                            _ => make_not_text(self.bool_input(block, "CONDITION")),
                        }
                    }
                };
                self.flush_notes(indent, out);
                out.push(format!("{pad}while {} {{", condition.text));
                let body = self.substack(block, "SUBSTACK");
                self.render_stack(body.as_deref(), indent + 1, out);
                out.push(format!("{pad}}}"));
            }
            "control_repeat" => {
                let times = self.value_input(block, "TIMES");
                let counter = unique_name("repeat_index", &mut self.taken);
                let condition = if times.ty.numeric() {
                    make_binary_text("<", DExpr::atom(counter.clone(), DTy::Float), times, DTy::Bool)
                } else {
                    let counter_expr = DExpr::atom(counter.clone(), DTy::Float);
                    self.operators_call("lessThan", &[&counter_expr, &times], DTy::Bool)
                };
                self.flush_notes(indent, out);
                out.push(format!("{pad}var {counter}: float = 0"));
                out.push(format!("{pad}while {} {{", condition.text));
                let body = self.substack(block, "SUBSTACK");
                self.render_stack(body.as_deref(), indent + 1, out);
                out.push(format!("{pad}    {counter} += 1"));
                out.push(format!("{pad}}}"));
            }
            "data_setvariableto" | "data_changevariableby" => {
                let (name, id) = field_ref(block, "VARIABLE").unwrap_or_default();
                let target = self.variable_expr(&name, id.as_deref());
                let value = self.value_input(block, "VALUE");
                let line = if opcode == "data_setvariableto" {
                    format!("{} = {}", target.text, value.text)
                } else if target.ty.numeric() && value.ty.numeric() {
                    format!("{} += {}", target.text, if value.prec <= 5 { format!("({})", value.text) } else { value.text })
                } else {
                    let sum = self.operators_call("add", &[&target, &value], DTy::Float);
                    format!("{} = {}", target.text, sum.text)
                };
                self.flush_notes(indent, out);
                out.push(format!("{pad}{line}"));
            }
            "event_broadcast" => {
                let (main, shadow) = Self::input_parts(block, "BROADCAST_INPUT");
                let literal = match main.or(shadow) {
                    Some(Value::Array(items)) if items.first() == Some(&json!(11)) => {
                        items.get(1).map(json_scalar_text)
                    }
                    Some(Value::String(id)) => self
                        .block(id)
                        .filter(|inner| inner["shadow"] == json!(true))
                        .and_then(|inner| field_text(inner, "BROADCAST_OPTION"))
                        .map(str::to_string),
                    _ => None,
                };
                if let Some(message) = literal {
                    let name = self.map_menu_value("event_broadcast_menu", &message);
                    self.flush_notes(indent, out);
                    out.push(format!("{pad}send {name}"));
                } else {
                    self.generic_statement(block, opcode, indent, out);
                }
            }
            "procedures_call" => {
                let proccode = block["mutation"]["proccode"].as_str().unwrap_or_default();
                let Some(procedure) = self.target.procedures.get(proccode) else {
                    self.note(format!("call to undefined custom block {proccode:?} skipped"));
                    self.flush_notes(indent, out);
                    return;
                };
                let arguments = procedure
                    .parameters
                    .iter()
                    .map(|(id, _, _, is_bool)| {
                        if *is_bool { self.bool_input(block, id).text } else { self.value_input(block, id).text }
                    })
                    .collect::<Vec<_>>();
                self.flush_notes(indent, out);
                out.push(format!("{pad}{}({})", procedure.name, arguments.join(", ")));
            }
            _ => self.generic_statement(block, opcode, indent, out),
        }
    }

    fn generic_statement(&mut self, block: &'a Value, opcode: &str, indent: usize, out: &mut Vec<String>) {
        let pad = "    ".repeat(indent);
        if let Some(builtin) = self.find_builtin(block).filter(|builtin| builtin.result.is_none()) {
            if let Some(call) = self.builtin_call(builtin, block) {
                self.flush_notes(indent, out);
                out.push(format!("{pad}{call}"));
                return;
            }
        }
        self.note(format!("unsupported block `{opcode}` skipped"));
        self.flush_notes(indent, out);
    }
}

struct RenderedScript {
    header: String,
    body: Vec<String>,
    green_flag: bool,
    commented: bool,
    procedure: bool,
}

fn project_targets(project: &Value) -> Result<Vec<&Value>, String> {
    let targets = project["targets"].as_array().ok_or("project.json has no targets array")?;
    let mut ordered = targets.iter().filter(|target| target["isStage"] == json!(true)).collect::<Vec<_>>();
    if ordered.len() != 1 {
        return Err("project.json must contain exactly one Stage target".to_string());
    }
    let mut sprites = targets.iter().filter(|target| target["isStage"] != json!(true)).collect::<Vec<_>>();
    sprites.sort_by_key(|target| target["layerOrder"].as_i64().unwrap_or(0));
    ordered.extend(sprites);
    Ok(ordered)
}

/// Decompile an `.sb3` archive into Rake sources, a scene, and assets.
pub fn decompile_sb3_bytes(bytes: &[u8]) -> Result<Decompilation, String> {
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes))
        .map_err(|error| format!("not a valid .sb3 archive: {error}"))?;
    let mut project = None;
    let mut assets = HashMap::new();
    for index in 0..archive.len() {
        let mut file = archive.by_index(index).map_err(|error| format!("cannot read .sb3 entry: {error}"))?;
        if file.is_dir() {
            continue;
        }
        let name = file.name().rsplit('/').next().unwrap_or_default().to_string();
        let mut contents = Vec::new();
        std::io::Read::read_to_end(&mut file, &mut contents)
            .map_err(|error| format!("cannot read {name} from the .sb3 archive: {error}"))?;
        if name == "project.json" {
            project = Some(
                serde_json::from_slice::<Value>(&contents)
                    .map_err(|error| format!("project.json is not valid JSON: {error}"))?,
            );
        } else {
            assets.insert(name, contents);
        }
    }
    let project = project.ok_or("the .sb3 archive has no project.json")?;
    decompile_project(&project, &assets)
}

/// Decompile a Scratch 3 project JSON with its asset files (keyed by `md5ext`).
pub fn decompile_project(project: &Value, assets: &HashMap<String, Vec<u8>>) -> Result<Decompilation, String> {
    let targets = project_targets(project)?;
    let mut warnings = Vec::new();

    // Sprite names become identifiers in the scene.
    let mut sprite_taken: HashSet<String> = ["stage".to_string()].into_iter().collect();
    let mut sprite_names = HashMap::new();
    let mut target_names = Vec::new();
    for target in &targets {
        let scratch = target["name"].as_str().unwrap_or("Sprite").to_string();
        if target["isStage"] == json!(true) {
            target_names.push(("stage".to_string(), scratch));
            continue;
        }
        let base = rake_identifier(&scratch, "Sprite");
        let rake = unique_name(&base, &mut sprite_taken);
        if rake != scratch {
            warnings.push(format!("sprite {scratch:?} is renamed to {rake}"));
        }
        sprite_names.insert(scratch.clone(), rake.clone());
        target_names.push((rake, scratch));
    }

    // Broadcast messages are shared by every target.
    let mut message_names = BTreeSet::new();
    for target in &targets {
        if let Some(broadcasts) = target["broadcasts"].as_object() {
            message_names.extend(broadcasts.values().filter_map(Value::as_str).map(str::to_string));
        }
        if let Some(blocks) = target["blocks"].as_object() {
            for block in blocks.values() {
                if let Some(name) = field_text(block, "BROADCAST_OPTION") {
                    message_names.insert(name.to_string());
                }
                if let Some(items) = block.as_array().filter(|items| items.first() == Some(&json!(11))) {
                    message_names.extend(items.get(1).and_then(Value::as_str).map(str::to_string));
                }
                if let Some(inputs) = block.get("inputs").and_then(Value::as_object) {
                    for input in inputs.values().filter_map(Value::as_array) {
                        for part in input.iter().skip(1).filter_map(Value::as_array) {
                            if part.first() == Some(&json!(11)) {
                                message_names.extend(part.get(1).and_then(Value::as_str).map(str::to_string));
                            }
                        }
                    }
                }
            }
        }
    }
    let mut global_taken: HashSet<String> = DECOMPILE_RESERVED.iter().map(|name| name.to_string()).collect();
    let mut message_taken = global_taken.clone();
    let mut messages = HashMap::new();
    for name in &message_names {
        let rake = unique_name(&rake_identifier(name, "message"), &mut message_taken);
        if &rake != name {
            warnings.push(format!("message {name:?} is renamed to {rake}"));
        }
        messages.insert(name.clone(), rake);
    }

    // Variables and lists. Every Rake module-level variable is a Stage variable, so
    // sprite-local ("for this sprite only") variables are promoted with unique names.
    let mut decompile = DecompileProject {
        builtins: builtin_blocks(),
        messages,
        sprite_names: sprite_names.clone(),
        variables: HashMap::new(),
        lists: HashMap::new(),
        stage_var_ids: HashMap::new(),
        stage_var_names: HashMap::new(),
        stage_list_ids: HashMap::new(),
        stage_list_names: HashMap::new(),
    };
    let mut target_data = Vec::new();
    for (index, target) in targets.iter().enumerate() {
        let is_stage = index == 0;
        let (rake_name, scratch_name) = target_names[index].clone();
        let mut data = DecompileTarget {
            rake_name: rake_name.clone(),
            scratch_name: scratch_name.clone(),
            is_stage,
            blocks: target["blocks"].as_object().cloned().unwrap_or_default(),
            var_ids: HashMap::new(),
            var_names: HashMap::new(),
            list_ids: HashMap::new(),
            list_names: HashMap::new(),
            procedures: HashMap::new(),
            taken: HashSet::new(),
        };
        let owner = (!is_stage).then(|| scratch_name.clone());
        if let Some(variables) = target["variables"].as_object() {
            for (id, entry) in variables {
                let scratch = entry.get(0).map(json_scalar_text).unwrap_or_default();
                let base = rake_identifier(&scratch, "variable");
                let base = if is_stage || !global_taken.contains(&base) { base } else { format!("{rake_name}_{base}") };
                let rake = unique_name(&base, &mut global_taken);
                let cloud = entry.get(2) == Some(&json!(true));
                if cloud {
                    warnings.push(format!("cloud variable {scratch:?} becomes a normal variable"));
                }
                if !is_stage {
                    warnings.push(format!(
                        "sprite-local variable {scratch:?} of {scratch_name:?} becomes the Stage variable {rake} (Rake has no sprite-local module variables yet)"
                    ));
                }
                let key = format!("v{index}:{id}");
                decompile.variables.insert(key.clone(), DecompiledVariable {
                    rake,
                    scratch: scratch.clone(),
                    initial: entry.get(1).cloned().unwrap_or(json!(0)),
                    owner: owner.clone(),
                    cloud,
                });
                if is_stage {
                    decompile.stage_var_ids.insert(id.clone(), key.clone());
                    decompile.stage_var_names.insert(scratch, key);
                } else {
                    data.var_ids.insert(id.clone(), key.clone());
                    data.var_names.insert(scratch, key);
                }
            }
        }
        if let Some(lists) = target["lists"].as_object() {
            for (id, entry) in lists {
                let scratch = entry.get(0).map(json_scalar_text).unwrap_or_default();
                let base = rake_identifier(&scratch, "list");
                let base = if is_stage || !global_taken.contains(&base) { base } else { format!("{rake_name}_{base}") };
                let rake = unique_name(&base, &mut global_taken);
                let items = entry.get(1).and_then(Value::as_array).map_or(0, Vec::len);
                if items > 0 {
                    warnings.push(format!("the {items} initial item(s) of list {scratch:?} are not restored"));
                }
                if !is_stage {
                    warnings.push(format!(
                        "sprite-local list {scratch:?} of {scratch_name:?} becomes the Stage list {rake}"
                    ));
                }
                let key = format!("l{index}:{id}");
                decompile.lists.insert(key.clone(), DecompiledList { rake, scratch: scratch.clone(), items, owner: owner.clone() });
                if is_stage {
                    decompile.stage_list_ids.insert(id.clone(), key.clone());
                    decompile.stage_list_names.insert(scratch, key);
                } else {
                    data.list_ids.insert(id.clone(), key.clone());
                    data.list_names.insert(scratch, key);
                }
            }
        }
        target_data.push(data);
    }

    // Custom blocks: names are unique within a target and never shadow a variable.
    for data in &mut target_data {
        let mut taken = global_taken.clone();
        let mut prototypes = data
            .blocks
            .values()
            .filter(|block| block["opcode"] == json!("procedures_prototype"))
            .collect::<Vec<_>>();
        prototypes.sort_by_key(|block| block["mutation"]["proccode"].as_str().unwrap_or_default().to_string());
        let mut procedures = HashMap::new();
        for prototype in prototypes {
            let mutation = &prototype["mutation"];
            let proccode = mutation["proccode"].as_str().unwrap_or_default().to_string();
            if procedures.contains_key(&proccode) {
                continue;
            }
            let parse_list = |key: &str| -> Vec<String> {
                mutation[key]
                    .as_str()
                    .and_then(|text| serde_json::from_str::<Vec<String>>(text).ok())
                    .unwrap_or_default()
            };
            let ids = parse_list("argumentids");
            let names = parse_list("argumentnames");
            let kinds = proccode
                .split_whitespace()
                .filter(|word| matches!(*word, "%s" | "%n" | "%b"))
                .map(|word| word == "%b")
                .collect::<Vec<_>>();
            let name = unique_name(&procedure_identifier(&proccode), &mut taken);
            let mut parameter_taken = taken.clone();
            let parameters = ids
                .iter()
                .enumerate()
                .map(|(position, id)| {
                    let scratch = names.get(position).cloned().unwrap_or_default();
                    let rake = unique_name(&rake_identifier(&scratch, "arg"), &mut parameter_taken);
                    (id.clone(), scratch, rake, kinds.get(position).copied().unwrap_or(false))
                })
                .collect();
            let warp = mutation["warp"] == json!("true") || mutation["warp"] == json!(true);
            procedures.insert(proccode, DecompiledProcedure { name, warp, parameters });
        }
        data.procedures = procedures;
        data.taken = taken;
    }

    // Infer `float` for variables that only ever hold numbers; everything else is a
    // dynamically typed `Scratch.Generics.Value`.
    let mut types: HashMap<String, DTy> = decompile
        .variables
        .iter()
        .map(|(key, variable)| {
            let numeric = match &variable.initial {
                Value::Number(_) => true,
                Value::String(text) => number_literal(text).is_some_and(|literal| literal.text == *text),
                _ => false,
            };
            (key.clone(), if numeric { DTy::Float } else { DTy::Value })
        })
        .collect();
    loop {
        let mut demoted = Vec::new();
        for data in &target_data {
            let mut renderer = Renderer::new(&decompile, data, &types);
            for block in data.blocks.values() {
                if block["opcode"] != json!("data_setvariableto") {
                    continue;
                }
                let Some((name, id)) = field_ref(block, "VARIABLE") else { continue };
                let Some(key) = renderer.lookup_variable(&name, id.as_deref()) else { continue };
                if types.get(&key) == Some(&DTy::Float) && !renderer.value_input(block, "VALUE").ty.numeric() {
                    demoted.push(key);
                }
            }
        }
        if demoted.is_empty() {
            break;
        }
        for key in demoted {
            types.insert(key, DTy::Value);
        }
    }

    let mut files = Vec::new();
    let mut flag_counter = 0;
    let mut scene_targets = Vec::new();
    for (index, data) in target_data.iter().enumerate() {
        let target = targets[index];
        let mut renderer = Renderer::new(&decompile, data, &types);
        let mut scripts = Vec::new();
        let mut tops = data
            .blocks
            .iter()
            .filter(|(_, block)| block["topLevel"] == json!(true) && block["shadow"] != json!(true))
            .collect::<Vec<_>>();
        tops.sort_by(|(left_id, left), (right_id, right)| {
            let key = |block: &Value| (block["y"].as_f64().unwrap_or(0.0), block["x"].as_f64().unwrap_or(0.0));
            key(left).partial_cmp(&key(right)).unwrap_or(std::cmp::Ordering::Equal).then(left_id.cmp(right_id))
        });
        for (_, hat) in tops {
            let opcode = hat["opcode"].as_str().unwrap_or_default();
            let next = hat["next"].as_str();
            let mut procedure = false;
            let mut green_flag = false;
            let mut commented = false;
            renderer.arguments.clear();
            let header = match opcode {
                "event_whenflagclicked" => {
                    green_flag = true;
                    renderer.used_namespaces.insert("Events".to_string());
                    "event Events.greenFlag {".to_string()
                }
                "event_whenkeypressed" => {
                    renderer.used_namespaces.insert("Events".to_string());
                    format!("event Events.keyPressed({}) {{", rake_string(field_text(hat, "KEY_OPTION").unwrap_or("space")))
                }
                "event_whenbackdropswitchesto" => {
                    renderer.used_namespaces.insert("Events".to_string());
                    format!("event Events.backdropSwitchedTo({}) {{", rake_string(field_text(hat, "BACKDROP").unwrap_or("")))
                }
                "event_whenthisspriteclicked" => {
                    renderer.used_namespaces.insert("Events".to_string());
                    "event Events.spriteClicked {".to_string()
                }
                "control_start_as_clone" => {
                    renderer.used_namespaces.insert("Events".to_string());
                    "event Events.cloneStart {".to_string()
                }
                "event_whenbroadcastreceived" => {
                    let name = field_text(hat, "BROADCAST_OPTION").unwrap_or("message1");
                    format!("event {} {{", renderer.map_menu_value("event_broadcast_menu", name))
                }
                "procedures_definition" => {
                    let prototype = hat["inputs"]["custom_block"][1]
                        .as_str()
                        .and_then(|id| data.blocks.get(id));
                    let proccode = prototype
                        .and_then(|prototype| prototype["mutation"]["proccode"].as_str())
                        .unwrap_or_default();
                    match data.procedures.get(proccode) {
                        Some(info) => {
                            procedure = true;
                            let parameters = info
                                .parameters
                                .iter()
                                .map(|(_, scratch, rake, is_bool)| {
                                    renderer.arguments.insert(scratch.clone(), (rake.clone(), *is_bool));
                                    format!("{rake}: {}", if *is_bool { "bool" } else { "Scratch.Generics.Value" })
                                })
                                .collect::<Vec<_>>();
                            format!(
                                "// Scratch custom block: {}\n{}fn {}({}) {{",
                                rake_string(proccode),
                                if info.warp { "warp " } else { "" },
                                info.name,
                                parameters.join(", ")
                            )
                        }
                        None => {
                            commented = true;
                            format!("// custom block {proccode:?} without a prototype")
                        }
                    }
                }
                _ => {
                    // A loose reporter or stack, or a hat Rake has no event for yet.
                    if !opcode.contains("when") {
                        if opcode.starts_with("procedures_") || opcode.starts_with("argument_") {
                            continue;
                        }
                        renderer.warnings.push(format!("{}: loose `{opcode}` script skipped", data.scratch_name));
                        continue;
                    }
                    commented = true;
                    renderer.warnings.push(format!(
                        "{}: unsupported hat `{opcode}`; its script is kept as a comment",
                        data.scratch_name
                    ));
                    format!("[decompile] unsupported hat `{opcode}`:")
                }
            };
            let mut body = Vec::new();
            renderer.render_stack(next, 1, &mut body);
            renderer.flush_notes(1, &mut body);
            scripts.push(RenderedScript { header, body, green_flag, commented, procedure });
        }

        let mut declared_vars: BTreeSet<String> = renderer.used_vars.clone();
        let mut declared_lists: BTreeSet<String> = renderer.used_lists.clone();
        for (key, variable) in &decompile.variables {
            if (data.is_stage && variable.owner.is_none()) || variable.owner.as_deref() == Some(data.scratch_name.as_str()) {
                declared_vars.insert(key.clone());
            }
        }
        for (key, list) in &decompile.lists {
            if (data.is_stage && list.owner.is_none()) || list.owner.as_deref() == Some(data.scratch_name.as_str()) {
                declared_lists.insert(key.clone());
            }
        }

        // Module-level variable initializers run at the green flag, and Rake allows only
        // one greenFlag handler next to them, so later green flag scripts are started
        // from the first one with a broadcast.
        let green_flags = scripts.iter().filter(|script| script.green_flag && !script.commented).count();
        if !declared_vars.is_empty() && green_flags > 1 {
            let mut relays = Vec::new();
            for script in scripts.iter_mut().filter(|script| script.green_flag && !script.commented).skip(1) {
                let mut taken: HashSet<String> = decompile.messages.values().cloned().collect();
                taken.extend(DECOMPILE_RESERVED.iter().map(|name| name.to_string()));
                loop {
                    flag_counter += 1;
                    let name = format!("rake_green_flag_{flag_counter}");
                    if !taken.contains(&name) {
                        script.header = format!(
                            "// [decompile] green flag script, started by the first greenFlag handler\nevent {name} {{"
                        );
                        relays.push(name);
                        break;
                    }
                }
            }
            if let Some(first) = scripts.iter_mut().find(|script| script.green_flag && !script.commented) {
                let mut body = relays.iter().map(|name| format!("    send {name}")).collect::<Vec<_>>();
                body.append(&mut first.body);
                first.body = body;
            }
        }

        let mut source = String::new();
        source.push_str(&format!(
            "// Decompiled from the Scratch {} {:?} by `rake decompile` (experimental).\n",
            if data.is_stage { "Stage" } else { "sprite" },
            data.scratch_name
        ));
        let mut imports = renderer.used_namespaces.iter().cloned().collect::<Vec<_>>();
        imports.sort();
        if !imports.is_empty() || !declared_lists.is_empty() {
            source.push('\n');
        }
        for namespace in &imports {
            source.push_str(&format!("use Scratch.{namespace}\n"));
        }
        if !declared_lists.is_empty() {
            source.push_str("use Scratch.Generics.List as SList\n");
        }
        let mut declarations = Vec::new();
        for key in &declared_vars {
            let variable = &decompile.variables[key];
            let ty = types.get(key).copied().unwrap_or(DTy::Value);
            let initial = match (&variable.initial, ty) {
                (Value::Bool(value), _) => value.to_string(),
                (value, DTy::Float) => number_literal(&json_scalar_text(value)).map_or("0".to_string(), |literal| literal.text),
                (Value::Number(number), _) => number_literal(&number.to_string()).map_or("0".to_string(), |literal| literal.text),
                (value, _) => rake_string(&json_scalar_text(value)),
            };
            let annotation = if ty == DTy::Float { "float" } else { "Scratch.Generics.Value" };
            let mut comment = Vec::new();
            if variable.rake != variable.scratch {
                comment.push(format!("Scratch name: {}", rake_string(&variable.scratch)));
            }
            if let Some(owner) = &variable.owner {
                comment.push(format!("local to {}", rake_string(owner)));
            }
            if variable.cloud {
                comment.push("cloud variable".to_string());
            }
            let comment = if comment.is_empty() { String::new() } else { format!(" // {}", comment.join("; ")) };
            declarations.push((variable.rake.clone(), format!("var {}: {annotation} = {initial}{comment}", variable.rake)));
        }
        for key in &declared_lists {
            let list = &decompile.lists[key];
            let mut comment = Vec::new();
            if list.rake != list.scratch {
                comment.push(format!("Scratch name: {}", rake_string(&list.scratch)));
            }
            if let Some(owner) = &list.owner {
                comment.push(format!("local to {}", rake_string(owner)));
            }
            if list.items > 0 {
                comment.push(format!("{} initial item(s) not restored", list.items));
            }
            let comment = if comment.is_empty() { String::new() } else { format!(" // {}", comment.join("; ")) };
            declarations.push((list.rake.clone(), format!("var {}: Scratch.Generics.List = SList.new(){comment}", list.rake)));
        }
        declarations.sort();
        if !declarations.is_empty() {
            source.push('\n');
            for (_, line) in declarations {
                source.push_str(&line);
                source.push('\n');
            }
        }
        scripts.sort_by_key(|script| !script.procedure);
        for script in scripts {
            source.push('\n');
            if script.commented {
                source.push_str(&format!("// {}\n", script.header));
                for line in script.body {
                    source.push_str(&format!("// {line}\n"));
                }
            } else {
                source.push_str(&script.header);
                source.push('\n');
                for line in script.body {
                    source.push_str(&line);
                    source.push('\n');
                }
                source.push_str("}\n");
            }
        }
        let has_scripts = source.lines().any(|line| !line.trim().is_empty() && !line.trim_start().starts_with("//"));
        let script_file = format!("src/{}.rk", if data.is_stage { "stage".to_string() } else { data.rake_name.clone() });
        warnings.extend(renderer.warnings);
        if has_scripts {
            files.push(DecompiledFile { path: script_file.clone(), contents: source.into_bytes() });
        }

        // Costumes and sounds.
        let asset_dir = if data.is_stage { "Stage".to_string() } else { data.rake_name.clone() };
        let mut file_taken = HashSet::new();
        let mut export_asset = |entry: &Value, kind: &str, allowed: &[&str], warnings: &mut Vec<String>| -> Option<(String, String)> {
            let scratch_name = entry["name"].as_str().unwrap_or(kind).to_string();
            let md5ext = entry["md5ext"]
                .as_str()
                .map(str::to_string)
                .or_else(|| Some(format!("{}.{}", entry["assetId"].as_str()?, entry["dataFormat"].as_str()?)))?;
            let extension = md5ext.rsplit('.').next().unwrap_or_default().to_ascii_lowercase();
            if !allowed.contains(&extension.as_str()) {
                warnings.push(format!("{}: {kind} {scratch_name:?} uses unsupported format {extension:?} and is skipped", data.scratch_name));
                return None;
            }
            let Some(bytes) = assets.get(&md5ext) else {
                warnings.push(format!("{}: {kind} file {md5ext} is missing from the archive and is skipped", data.scratch_name));
                return None;
            };
            let stem = safe_file_stem(&scratch_name, kind);
            let mut unique = stem.clone();
            let mut suffix = 2;
            while !file_taken.insert(unique.to_lowercase()) {
                unique = format!("{stem}_{suffix}");
                suffix += 1;
            }
            if unique != scratch_name {
                warnings.push(format!(
                    "{}: {kind} {scratch_name:?} is renamed to {unique:?} (names come from file names)",
                    data.scratch_name
                ));
            }
            let path = format!("assets/{asset_dir}/{unique}.{extension}");
            files.push(DecompiledFile { path: path.clone(), contents: bytes.clone() });
            Some((path, extension))
        };
        let mut costumes = Vec::new();
        let mut current_costume = 0;
        let selected = target["currentCostume"].as_u64().unwrap_or(0) as usize;
        for (position, costume) in target["costumes"].as_array().into_iter().flatten().enumerate() {
            let Some((path, extension)) = export_asset(costume, "costume", &["svg", "png", "jpg", "jpeg", "gif", "bmp"], &mut warnings) else {
                continue;
            };
            if position == selected {
                current_costume = costumes.len();
            }
            let center_x = costume["rotationCenterX"].as_f64().unwrap_or(0.0);
            let center_y = costume["rotationCenterY"].as_f64().unwrap_or(0.0);
            let resolution = costume["bitmapResolution"].as_u64().unwrap_or(1);
            let mut entry = format!("(\"../{path}\", {}, {}", format_number(center_x), format_number(center_y));
            if extension != "svg" && resolution == 2 {
                entry.push_str(", 2");
            }
            entry.push(')');
            costumes.push(entry);
        }
        let mut sounds = Vec::new();
        for sound in target["sounds"].as_array().into_iter().flatten() {
            if let Some((path, _)) = export_asset(sound, "sound", &["wav", "mp3"], &mut warnings) {
                sounds.push(rake_string(&format!("../{path}")));
            }
        }

        let mut properties = Vec::new();
        let costume_key = if data.is_stage { "backdrops" } else { "costumes" };
        if !costumes.is_empty() {
            properties.push(format!("{costume_key}: [{}]", costumes.join(", ")));
            properties.push(format!(
                "{}: {current_costume}",
                if data.is_stage { "current_backdrop" } else { "current_costume" }
            ));
        }
        if !sounds.is_empty() {
            properties.push(format!("sounds: [{}]", sounds.join(", ")));
        }
        if has_scripts {
            properties.push(format!("scripts: [{}]", rake_string(&format!("../{script_file}"))));
        }
        let number = |key: &str, default: f64| format_number(target[key].as_f64().unwrap_or(default));
        let volume = target["volume"].as_f64().unwrap_or(100.0).clamp(0.0, 100.0);
        if data.is_stage {
            properties.push(format!("volume: {}", format_number(volume)));
            let tempo = target["tempo"].as_f64().unwrap_or(60.0).clamp(20.0, 500.0);
            properties.push(format!("tempo: {}", format_number(tempo)));
        } else {
            properties.push(format!("position: ({}, {})", number("x", 0.0), number("y", 0.0)));
            let size = target["size"].as_f64().filter(|size| *size > 0.0).unwrap_or(100.0);
            properties.push(format!("size: {}", format_number(size)));
            let direction = target["direction"].as_f64().unwrap_or(90.0).clamp(-360.0, 360.0);
            properties.push(format!("direction: {}", format_number(direction)));
            properties.push(format!("visible: {}", target["visible"].as_bool().unwrap_or(true)));
            properties.push(format!("layer: {}", number("layerOrder", index as f64)));
            properties.push(format!("draggable: {}", target["draggable"].as_bool().unwrap_or(false)));
            let style = target["rotationStyle"].as_str().filter(|style| ROTATION_STYLES.contains(style)).unwrap_or("all around");
            properties.push(format!("rotation_style: {}", rake_string(style)));
            properties.push(format!("volume: {}", format_number(volume)));
        }
        scene_targets.push((data.is_stage, data.rake_name.clone(), properties));
    }

    let mut scene = String::from("// Decompiled by `rake decompile` (experimental).\nscene Main {\n");
    for (position, (is_stage, name, properties)) in scene_targets.iter().enumerate() {
        if position > 0 {
            scene.push('\n');
        }
        if *is_stage {
            scene.push_str("    stage {\n");
        } else {
            scene.push_str(&format!("    sprite {name} {{\n"));
        }
        for property in properties {
            scene.push_str(&format!("        {property}\n"));
        }
        scene.push_str("    }\n");
    }
    scene.push_str("}\n");
    let scene_path = "scenes/main.rks".to_string();
    files.push(DecompiledFile { path: scene_path.clone(), contents: scene.into_bytes() });
    files.push(DecompiledFile {
        path: "rake-ide.json".to_string(),
        contents: serde_json::to_vec_pretty(&json!({"name": "decompiled", "mainScene": scene_path})).unwrap(),
    });
    Ok(Decompilation { files, scene_path, warnings })
}

/// Write a decompiled project under `directory`, creating folders as needed.
pub fn write_decompilation(directory: &Path, decompilation: &Decompilation) -> Result<(), String> {
    for file in &decompilation.files {
        let path = directory.join(&file.path);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
        }
        fs::write(&path, &file.contents).map_err(|error| format!("cannot write {}: {error}", path.display()))?;
    }
    Ok(())
}
