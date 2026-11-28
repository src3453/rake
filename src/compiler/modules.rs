// Library modules.
//
// `from "lib/math" use Math.clamp` loads `<project root>/lib/math.rk` and makes the
// declarations of that file visible to the importing file. Every module of a program
// is parsed separately (so `use` aliases stay file-local) but by one shared parser and
// type checker, so a program still compiles into one Scratch target.

/// The file that marks a Rake project root. A future build system may name the root
/// itself; until then the nearest ancestor directory containing this file is used.
pub const PROJECT_MANIFEST: &str = "rake-ide.json";

/// Where library module sources come from. The CLI reads the file system; the
/// language server answers from open editor buffers first.
pub trait SourceProvider {
    /// Read `<root>/<relative>`. `relative` is normalized, uses `/`, and ends in `.rk`.
    fn read_module(&self, root: &Path, relative: &str) -> Result<String, String>;
}

/// Reads library modules from disk, rejecting paths whose letter case differs from
/// the files on disk so projects behave the same on case-insensitive file systems.
pub struct FileSystemSources;

impl SourceProvider for FileSystemSources {
    fn read_module(&self, root: &Path, relative: &str) -> Result<String, String> {
        check_module_path_case(root, relative)?;
        fs::read_to_string(root.join(relative)).map_err(|error| error.to_string())
    }
}

fn check_module_path_case(root: &Path, relative: &str) -> Result<(), String> {
    let mut directory = root.to_path_buf();
    for segment in relative.split('/') {
        let entries = fs::read_dir(&directory).map_err(|error| error.to_string())?;
        let mut case_mismatch = None;
        let mut found = false;
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name == segment {
                found = true;
                break;
            }
            if name.eq_ignore_ascii_case(segment) {
                case_mismatch = Some(name.to_string());
            }
        }
        if !found {
            return Err(match case_mismatch {
                Some(actual) => format!(
                    "the path differs in letter case from {actual:?} on disk; module paths are case-sensitive"
                ),
                None => "file not found".to_string(),
            });
        }
        directory.push(segment);
    }
    Ok(())
}

/// The project root for a source file: the nearest ancestor directory that contains
/// `rake-ide.json`, or the file's own directory when there is none.
pub fn find_project_root(source_path: &Path) -> PathBuf {
    let absolute = if source_path.is_absolute() {
        source_path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(source_path))
            .unwrap_or_else(|_| source_path.to_path_buf())
    };
    let start = absolute.parent().unwrap_or(Path::new("")).to_path_buf();
    let mut directory = Some(start.as_path());
    while let Some(candidate) = directory {
        if candidate.join(PROJECT_MANIFEST).is_file() {
            return candidate.to_path_buf();
        }
        directory = candidate.parent();
    }
    start
}

/// Normalize a `from` path: project-root relative, `/`-separated, `.rk` appended when
/// missing, `.` and `..` resolved, and never escaping the root.
fn normalize_module_path(path: &str) -> Result<String, String> {
    if path.is_empty() {
        return Err("library path is empty".to_string());
    }
    if path.contains('\\') {
        return Err("library paths use '/' as the separator".to_string());
    }
    if path.starts_with('/') || path.contains(':') {
        return Err("library paths are relative to the project root; remove the leading '/' or drive".to_string());
    }
    let mut segments: Vec<&str> = Vec::new();
    for segment in path.split('/') {
        match segment {
            "" => return Err("library paths cannot contain empty segments".to_string()),
            "." => {}
            ".." => {
                if segments.pop().is_none() {
                    return Err("library paths cannot leave the project root".to_string());
                }
            }
            segment => segments.push(segment),
        }
    }
    if segments.is_empty() {
        return Err("library path does not name a file".to_string());
    }
    let mut normalized = segments.join("/");
    if !normalized.ends_with(".rk") {
        normalized.push_str(".rk");
    }
    Ok(normalized)
}

/// The file a `from` path names: `<root>/<normalized path>.rk`.
pub fn library_file_path(root: &Path, path: &str) -> Result<PathBuf, String> {
    Ok(root.join(normalize_module_path(path)?))
}

/// The location of a compiled `.rk` file, used to resolve its `from` imports.
pub struct ModuleContext<'a> {
    /// The file being compiled.
    pub entry_path: &'a Path,
    /// The project root that `from` paths are relative to.
    pub root: &'a Path,
    pub sources: &'a dyn SourceProvider,
}

/// A compile error and the file it belongs to. `file` is `None` for the entry
/// source itself (which may not exist on disk).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SourceError {
    pub file: Option<PathBuf>,
    pub message: String,
}

impl std::fmt::Display for SourceError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.file {
            Some(file) => write!(formatter, "{}:{}", file.display(), self.message),
            None => formatter.write_str(&self.message),
        }
    }
}

struct LoadedModule {
    /// Root-relative path (`lib/math.rk`); empty for an entry outside the root.
    relative: String,
    path: Option<PathBuf>,
    tokens: Vec<Token>,
    /// `from` path as written -> module ID.
    imports: HashMap<String, u32>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VisitState {
    Visiting,
    Done,
}

/// Loads the entry file and, transitively, every library module it imports.
struct ModuleLoader<'a> {
    context: Option<&'a ModuleContext<'a>>,
    modules: Vec<LoadedModule>,
    by_relative: HashMap<String, u32>,
    states: HashMap<u32, VisitState>,
    stack: Vec<u32>,
    /// Modules ordered so that every module comes after the modules it imports.
    order: Vec<u32>,
    /// Editor analysis: leave imports that cannot be loaded unresolved.
    lenient_imports: bool,
}

/// The `from "path" use` clauses at the top level of a token stream.
fn scan_module_imports(tokens: &[Token]) -> Vec<Token> {
    let mut imports = Vec::new();
    let mut depth = 0usize;
    for (index, token) in tokens.iter().enumerate() {
        match token.text.as_str() {
            "{" if token.kind == TokenKind::Symbol => depth += 1,
            "}" if token.kind == TokenKind::Symbol => depth = depth.saturating_sub(1),
            "from" if depth == 0 && token.kind == TokenKind::Identifier => {
                let path = tokens.get(index + 1);
                let keyword = tokens.get(index + 2);
                if path.is_some_and(|path| path.kind == TokenKind::String)
                    && keyword.is_some_and(|keyword| keyword.text == "use")
                {
                    imports.push(path.unwrap().clone());
                }
            }
            _ => {}
        }
    }
    imports
}

fn tag_tokens(tokens: &mut [Token], file: u32) {
    for token in tokens {
        token.file = file;
    }
}

impl<'a> ModuleLoader<'a> {
    fn new(context: Option<&'a ModuleContext<'a>>) -> Self {
        Self {
            context,
            modules: Vec::new(),
            by_relative: HashMap::new(),
            states: HashMap::new(),
            stack: Vec::new(),
            order: Vec::new(),
            lenient_imports: false,
        }
    }

    /// Load the entry source and its libraries, then parse them into one program.
    fn load(&mut self, source: &str) -> Result<Vec<Stmt>, String> {
        let entry_relative = self
            .context
            .and_then(|context| {
                let entry = absolute_path(context.entry_path);
                let root = absolute_path(context.root);
                entry.strip_prefix(&root).ok().map(|relative| {
                    relative
                        .components()
                        .map(|component| component.as_os_str().to_string_lossy().to_string())
                        .collect::<Vec<_>>()
                        .join("/")
                })
            })
            .unwrap_or_default();
        let tokens = expand_macros(lex(source)?)?;
        if !entry_relative.is_empty() {
            self.by_relative.insert(entry_relative.clone(), 0);
        }
        self.modules.push(LoadedModule {
            relative: entry_relative,
            path: self.context.map(|context| context.entry_path.to_path_buf()),
            tokens,
            imports: HashMap::new(),
        });
        self.visit(0)?;

        let mut parser = Parser::new(Vec::new());
        parser.lenient_imports = self.lenient_imports;
        let mut statements = Vec::new();
        for id in self.order.clone() {
            let module = &mut self.modules[id as usize];
            let tokens = std::mem::take(&mut module.tokens);
            let imports = module.imports.clone();
            statements.extend(parser.parse_module(tokens, id, imports)?);
        }
        Ok(statements)
    }

    fn visit(&mut self, id: u32) -> Result<(), String> {
        self.states.insert(id, VisitState::Visiting);
        self.stack.push(id);
        let Some(context) = self.context else {
            // Without a file location, `from` imports stay unresolved and the
            // parser reports them at the import site.
            self.stack.pop();
            self.states.insert(id, VisitState::Done);
            self.order.push(id);
            return Ok(());
        };
        for path_token in scan_module_imports(&self.modules[id as usize].tokens) {
            let written = path_token.text.clone();
            match self.load_import(&path_token, context) {
                Ok(dependency) => {
                    self.modules[id as usize].imports.insert(written, dependency);
                }
                Err(_) if self.lenient_imports => {}
                Err(message) => return Err(message),
            }
        }
        self.stack.pop();
        self.states.insert(id, VisitState::Done);
        self.order.push(id);
        Ok(())
    }

    /// Resolve, read, and recursively load the library named by one `from` path.
    fn load_import(&mut self, path_token: &Token, context: &ModuleContext) -> Result<u32, String> {
        let written = path_token.text.clone();
        let relative = normalize_module_path(&written)
            .map_err(|message| diagnostic(path_token, format!("invalid library path {written:?}: {message}")))?;
        let dependency = match self.by_relative.get(&relative).copied() {
            Some(dependency) => {
                if self.states.get(&dependency) == Some(&VisitState::Visiting) {
                    let start = self.stack.iter().position(|item| *item == dependency).unwrap_or(0);
                    let mut cycle = self.stack[start..]
                        .iter()
                        .map(|item| self.display_name(*item))
                        .collect::<Vec<_>>();
                    cycle.push(self.display_name(dependency));
                    return Err(diagnostic(
                        path_token,
                        format!("circular library import: {}", cycle.join(" -> ")),
                    ));
                }
                dependency
            }
            None => {
                let dependency = self.modules.len() as u32;
                let path = context.root.join(&relative);
                let source = context.sources.read_module(context.root, &relative).map_err(|error| {
                    diagnostic(
                        path_token,
                        format!("cannot load library {written:?} ({}): {error}", path.display()),
                    )
                })?;
                let mut tokens = lex(&source).map_err(|message| module_diagnostic(dependency, message))?;
                tag_tokens(&mut tokens, dependency);
                self.modules.push(LoadedModule {
                    relative: relative.clone(),
                    path: Some(path),
                    tokens: Vec::new(),
                    imports: HashMap::new(),
                });
                self.by_relative.insert(relative, dependency);
                let tokens = expand_macros(tokens).map_err(|message| module_diagnostic(dependency, message))?;
                self.modules[dependency as usize].tokens = tokens;
                self.visit(dependency)?;
                dependency
            }
        };
        Ok(dependency)
    }

    fn display_name(&self, id: u32) -> String {
        let module = &self.modules[id as usize];
        if module.relative.is_empty() {
            "<entry>".to_string()
        } else {
            module.relative.clone()
        }
    }

    /// Root-relative names of the library modules, by module ID (the entry is `""`).
    fn module_names(&self) -> HashMap<u32, String> {
        self.modules
            .iter()
            .enumerate()
            .map(|(id, module)| (id as u32, module.relative.clone()))
            .collect()
    }

    /// Attribute a diagnostic to the module it was raised in.
    fn source_error(&self, message: &str) -> SourceError {
        let (file, message) = split_module_diagnostic(message);
        SourceError {
            file: if file == 0 {
                None
            } else {
                self.modules.get(file as usize).and_then(|module| module.path.clone())
            },
            message: message.to_string(),
        }
    }
}

fn absolute_path(path: &Path) -> PathBuf {
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map(|directory| directory.join(path))
            .unwrap_or_else(|_| path.to_path_buf())
    };
    // Resolve `.`/`..` lexically; the entry file may not exist yet (unsaved buffers).
    let mut normalized = PathBuf::new();
    for component in absolute.components() {
        match component {
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                normalized.pop();
            }
            component => normalized.push(component),
        }
    }
    normalized
}

/// Drop library functions that nothing outside them reaches, so importing a library
/// adds only the custom blocks a program actually uses. Returns the IDs of the
/// removed functions' helper variables; drop them after lowering, which numbers new
/// variables from the symbol count.
fn prune_unused_library_functions(statements: &mut Vec<Stmt>) -> std::collections::HashSet<String> {
    fn is_library_function(statement: &Stmt) -> bool {
        matches!(statement, Stmt::Function { name_token, owner_class: None, .. } if name_token.file != 0)
    }
    fn library_functions<'s>(statements: &'s [Stmt], output: &mut HashMap<String, &'s Stmt>) {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => library_functions(body, output),
                Stmt::Function { name, .. } if is_library_function(statement) => {
                    output.insert(name.clone(), statement);
                }
                _ => {}
            }
        }
    }
    fn root_calls(statements: &[Stmt], calls: &mut Vec<String>) {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => root_calls(body, calls),
                statement if is_library_function(statement) => {}
                statement => collect_statement_calls(std::slice::from_ref(statement), calls),
            }
        }
    }
    let mut functions = HashMap::new();
    library_functions(statements, &mut functions);
    if functions.is_empty() {
        return std::collections::HashSet::new();
    }
    let mut pending = Vec::new();
    root_calls(statements, &mut pending);
    let mut reachable = std::collections::HashSet::new();
    while let Some(name) = pending.pop() {
        if !reachable.insert(name.clone()) {
            continue;
        }
        if let Some(function) = functions.get(&name) {
            collect_statement_calls(std::slice::from_ref(*function), &mut pending);
        }
    }
    let removed_symbols = functions
        .iter()
        .filter(|(name, _)| !reachable.contains(*name))
        .filter_map(|(_, function)| match function {
            Stmt::Function { info: Some(info), .. } => Some(info),
            _ => None,
        })
        .flat_map(|info| [info.return_symbol.as_ref(), info.completed_symbol.as_ref()])
        .flatten()
        .map(|symbol| symbol.id.clone())
        .collect::<std::collections::HashSet<_>>();
    fn retain(statements: &mut Vec<Stmt>, reachable: &std::collections::HashSet<String>) {
        statements.retain(|statement| match statement {
            Stmt::Function { name, .. } if is_library_function(statement) => reachable.contains(name),
            _ => true,
        });
        for statement in statements.iter_mut() {
            if let Stmt::Namespace { body, .. } = statement {
                retain(body, reachable);
            }
        }
    }
    retain(statements, &reachable);
    removed_symbols
}

/// A custom block that came from a library module. Scene builds keep one copy per
/// target when several scripts import the same library.
#[derive(Clone, Debug)]
struct LibraryProcedure {
    proccode: String,
    module: String,
}

fn library_procedures(statements: &[Stmt], modules: &HashMap<u32, String>, output: &mut Vec<LibraryProcedure>) {
    for statement in statements {
        match statement {
            Stmt::Namespace { body, .. } => library_procedures(body, modules, output),
            Stmt::Function {
                name_token,
                owner_class: None,
                info: Some(info),
                ..
            } if name_token.file != 0 => output.push(LibraryProcedure {
                proccode: info.proccode.clone(),
                module: modules.get(&name_token.file).cloned().unwrap_or_default(),
            }),
            _ => {}
        }
    }
}
