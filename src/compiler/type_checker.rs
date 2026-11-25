struct TypeChecker {
    scopes: Vec<HashMap<String, Symbol>>,
    symbols: Vec<Symbol>,
    scratch_names: Vec<String>,
    next_id: usize,
    functions: HashMap<String, FunctionInfo>,
    classes: HashMap<String, ClassInfo>,
    objects: HashMap<String, ObjectInstance>,
    namespaces: std::collections::HashSet<String>,
    imports: HashMap<String, HashMap<String, String>>,
    current_function: Option<FunctionInfo>,
}

impl TypeChecker {
    fn new() -> Self {
        Self {
            scopes: vec![HashMap::new()],
            symbols: Vec::new(),
            scratch_names: Vec::new(),
            next_id: 0,
            functions: HashMap::new(),
            classes: HashMap::new(),
            objects: HashMap::new(),
            namespaces: std::collections::HashSet::new(),
            imports: HashMap::new(),
            current_function: None,
        }
    }

    fn check(mut self, statements: &mut [Stmt]) -> Result<Vec<Symbol>, String> {
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
