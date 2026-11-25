#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Type {
    Int,
    Float,
    Bool,
    Text,
    Any,
    ScratchValue,
    ScratchList,
    Class(u32),
}

impl Type {
    fn parse(path: &str, token: &Token) -> Result<Self, String> {
        match path {
            "int" => Ok(Self::Int),
            "float" => Ok(Self::Float),
            "bool" => Ok(Self::Bool),
            "text" => Ok(Self::Text),
            "Scratch.Generics.Value" => Ok(Self::ScratchValue),
            "Scratch.Generics.List" => Ok(Self::ScratchList),
            _ => Err(diagnostic(
                token,
                format!(
                    "unknown type {path:?}; expected int, float, bool, text, Scratch.Generics.Value, or Scratch.Generics.List",
                ),
            )),
        }
    }

    fn name(self) -> &'static str {
        match self {
            Self::Int => "int",
            Self::Float => "float",
            Self::Bool => "bool",
            Self::Text => "text",
            Self::Any => "value",
            Self::ScratchValue => "Scratch.Generics.Value",
            Self::ScratchList => "Scratch.Generics.List",
            Self::Class(_) => "class instance",
        }
    }

    fn numeric(self) -> bool {
        matches!(self, Self::Int | Self::Float)
    }
}

fn builtin_type_path(path: &str) -> Option<Type> {
    match path {
        "Scratch.Generics.Value" => Some(Type::ScratchValue),
        "Scratch.Generics.List" => Some(Type::ScratchList),
        _ => None,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorageClass {
    Project,
    Target,
    ProjectList,
    ProcedureArgument,
    ObjectBinding,
}

#[derive(Clone, Debug)]
struct Symbol {
    source_name: String,
    scratch_name: String,
    ty: Type,
    mutable: bool,
    id: String,
    semantic_id: String,
    argument_id: Option<String>,
    storage: StorageClass,
    initial_value: Value,
}

#[derive(Clone, Debug)]
struct FieldDeclaration {
    token: Token,
    name: String,
    ty: Type,
    mutable: bool,
    initializer: Expr,
}

#[derive(Clone, Debug)]
struct ClassInfo {
    name: String,
    id: u32,
    fields: Vec<FieldDeclaration>,
}

#[derive(Clone, Debug)]
struct ObjectInstance {
    class_id: u32,
    fields: HashMap<String, Symbol>,
}

#[derive(Clone, Debug)]
struct FunctionArgument {
    name: String,
    id: String,
    ty: Type,
}

#[derive(Clone, Debug)]
struct FunctionInfo {
    name: String,
    warp: bool,
    symbol_id: String,
    signature: String,
    proccode: String,
    arguments: Vec<FunctionArgument>,
    return_type: Option<Type>,
    return_symbol: Option<Symbol>,
    completed_symbol: Option<Symbol>,
}
