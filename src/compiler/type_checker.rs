struct TypeChecker {
    runtime: Runtime,
    scopes: Vec<HashMap<String, Symbol>>,
    symbols: Vec<Symbol>,
    scratch_names: Vec<String>,
    reserved_source_names: std::collections::HashSet<String>,
    next_id: usize,
    functions: HashMap<String, FunctionInfo>,
    classes: HashMap<String, ClassInfo>,
    enums: HashMap<String, EnumInfo>,
    objects: HashMap<String, ObjectInstance>,
    namespaces: std::collections::HashSet<String>,
    imports: HashMap<String, HashMap<String, String>>,
    current_function: Option<FunctionInfo>,
    current_method_class: Option<u32>,
    next_object_ordinal: u32,
    used_proccodes: std::collections::HashSet<String>,
    /// Root-relative path of each library module (the entry file is `""`).
    module_names: HashMap<u32, String>,
    /// The module whose code is being checked.
    current_file: u32,
    /// The module that declared each namespace, function, class, and enum.
    declaration_files: HashMap<String, u32>,
    /// Top-level declarations of each module, for `from "..." use *`.
    module_top_level: HashMap<u32, Vec<String>>,
    /// Modules each module imports directly with `from`.
    visible_files: HashMap<u32, std::collections::HashSet<u32>>,
    /// Import names (by scope key) that came from a `*` import.
    glob_imports: std::collections::HashSet<(String, String)>,
}

impl TypeChecker {
    fn new(runtime: Runtime) -> Self {
        Self {
            runtime,
            scopes: vec![HashMap::new()],
            symbols: Vec::new(),
            scratch_names: Vec::new(),
            reserved_source_names: std::collections::HashSet::new(),
            next_id: 0,
            functions: HashMap::new(),
            classes: HashMap::new(),
            enums: HashMap::new(),
            objects: HashMap::new(),
            namespaces: std::collections::HashSet::new(),
            imports: HashMap::new(),
            current_function: None,
            current_method_class: None,
            next_object_ordinal: 0,
            used_proccodes: std::collections::HashSet::new(),
            module_names: HashMap::new(),
            current_file: 0,
            declaration_files: HashMap::new(),
            module_top_level: HashMap::new(),
            visible_files: HashMap::new(),
            glob_imports: std::collections::HashSet::new(),
        }
    }

    fn with_modules(mut self, module_names: HashMap<u32, String>) -> Self {
        self.module_names = module_names;
        self
    }

    fn check_builtin_runtime(&self, builtin: &BuiltinBlock, token: &Token) -> Result<(), String> {
        if let Some(extension) = builtin.extension {
            if !self.runtime.supports_extension(extension) {
                return Err(diagnostic(
                    token,
                    format!(
                        "Scratch extension {extension:?} used by {:?} is not available in the {} runtime",
                        builtin.name,
                        self.runtime.as_str()
                    ),
                ));
            }
        }
        Ok(())
    }

    fn check(mut self, statements: &mut Vec<Stmt>) -> Result<Vec<Symbol>, String> {
        self.reserve_source_names(statements);
        self.collect_declaration_files(statements, true);
        self.collect_namespace_paths(statements)?;
        self.collect_imports(statements)?;
        self.register_enums(statements)?;
        self.register_classes(statements)?;
        self.register_functions(statements)?;
        self.validate_imports(statements)?;
        self.check_statements(statements, false)?;
        self.check_function_declarations(statements)?;
        validate_recursive_return_functions(statements)?;
        Ok(self.symbols)
    }
}
