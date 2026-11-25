struct TypeChecker {
    scopes: Vec<HashMap<String, Symbol>>,
    symbols: Vec<Symbol>,
    scratch_names: Vec<String>,
    reserved_source_names: std::collections::HashSet<String>,
    next_id: usize,
    functions: HashMap<String, FunctionInfo>,
    classes: HashMap<String, ClassInfo>,
    objects: HashMap<String, ObjectInstance>,
    namespaces: std::collections::HashSet<String>,
    imports: HashMap<String, HashMap<String, String>>,
    current_function: Option<FunctionInfo>,
    current_method_class: Option<u32>,
    next_object_ordinal: u32,
    used_proccodes: std::collections::HashSet<String>,
    event_names: std::collections::HashSet<String>,
}

impl TypeChecker {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            symbols: Vec::new(),
            scratch_names: Vec::new(),
            reserved_source_names: std::collections::HashSet::new(),
            next_id: 0,
            functions: HashMap::new(),
            classes: HashMap::new(),
            objects: HashMap::new(),
            namespaces: std::collections::HashSet::new(),
            imports: HashMap::new(),
            current_function: None,
            current_method_class: None,
            next_object_ordinal: 0,
            used_proccodes: std::collections::HashSet::new(),
            event_names: std::collections::HashSet::new(),
        }
    }

    fn check(mut self, statements: &mut Vec<Stmt>) -> Result<Vec<Symbol>, String> {
        self.reserve_source_names(statements);
        self.collect_namespace_paths(statements)?;
        self.collect_imports(statements)?;
        self.register_classes(statements)?;
        self.register_functions(statements)?;
        self.validate_imports(statements)?;
        self.check_statements(statements, false)?;
        self.check_function_declarations(statements)?;
        validate_recursive_return_functions(statements)?;
        Ok(self.symbols)
    }
}
