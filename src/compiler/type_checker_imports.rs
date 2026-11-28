/// The key of one file's imports in one namespace scope. `use` declarations are
/// file-local, so two modules may both import into the top-level scope.
fn import_scope_key(file: u32, scope: &str) -> String {
    format!("{file}:{scope}")
}

impl TypeChecker {
    /// Record which module declared every namespace, function, class, and enum.
    fn collect_declaration_files(&mut self, statements: &[Stmt], top_level: bool) {
        for statement in statements {
            let (token, name) = match statement {
                Stmt::Namespace { name_token, path, body } => {
                    self.collect_declaration_files(body, false);
                    (name_token, path)
                }
                Stmt::Function { name_token, name, owner_class: None, .. }
                | Stmt::Class { name_token, name, .. }
                | Stmt::Enum { name_token, name, .. } => (name_token, name),
                Stmt::Use { token, module: Some(module), .. } => {
                    self.visible_files.entry(token.file).or_default().insert(module.id);
                    continue;
                }
                _ => continue,
            };
            self.declaration_files.entry(name.clone()).or_insert(token.file);
            if top_level {
                self.module_top_level
                    .entry(token.file)
                    .or_default()
                    .push(name.clone());
            }
        }
    }

    /// The library path of a module for diagnostics.
    fn module_display_name(&self, file: u32) -> String {
        self.module_names
            .get(&file)
            .filter(|name| !name.is_empty())
            .map(|name| name.trim_end_matches(".rk").to_string())
            .unwrap_or_else(|| "<entry>".to_string())
    }

    /// The module that declared `path` or the namespace containing it.
    fn declaring_file(&self, path: &str) -> Option<u32> {
        let mut candidate = path;
        loop {
            if let Some(file) = self.declaration_files.get(candidate) {
                return Some(*file);
            }
            candidate = candidate.rsplit_once('.')?.0;
        }
    }

    fn file_can_see(&self, from: u32, declared_in: u32) -> bool {
        from == declared_in
            || self
                .visible_files
                .get(&from)
                .is_some_and(|files| files.contains(&declared_in))
    }

    /// Reject references to declarations of a library the current file does not
    /// import directly. Transitively loaded libraries are not visible.
    fn ensure_visible(&self, path: &str, token: &Token) -> Result<(), String> {
        let Some(declared_in) = self.declaring_file(path) else {
            return Ok(());
        };
        if self.file_can_see(self.current_file, declared_in) {
            return Ok(());
        }
        let library = self.module_display_name(declared_in);
        Err(diagnostic(
            token,
            format!(
                "{path:?} is declared in library {library:?}, which this file does not import; add `from {library:?} use ...`"
            ),
        ))
    }

    fn collect_namespace_paths(&mut self, statements: &[Stmt]) -> Result<(), String> {
        for block in builtin_blocks() {
            let parts = block.name.split('.').collect::<Vec<_>>();
            for depth in 1..parts.len() {
                self.namespaces.insert(parts[..depth].join("."));
            }
        }
        for statement in statements {
            if let Stmt::Namespace {
                name_token,
                path,
                body,
                ..
            } = statement
            {
                if !self.namespaces.insert(path.clone()) {
                    return Err(diagnostic(
                        name_token,
                        format!("namespace {path:?} is already declared"),
                    ));
                }
                self.collect_namespace_paths(body)?;
            }
        }
        Ok(())
    }

    /// Full paths of every namespace, declaration, and built-in directly inside `path`.
    fn direct_members(&self, path: &str) -> Vec<String> {
        let prefix = format!("{path}.");
        let is_direct = |name: &str| {
            name.strip_prefix(&prefix)
                .is_some_and(|leaf| !leaf.is_empty() && !leaf.contains('.'))
        };
        let mut members = std::collections::BTreeSet::new();
        for name in self.namespaces.iter().chain(self.declaration_files.keys()) {
            if is_direct(name) {
                members.insert(name.clone());
            }
        }
        for block in builtin_blocks() {
            if is_direct(block.name) {
                members.insert(block.name.to_string());
            }
        }
        for event in builtin_event_descriptions() {
            if is_direct(event.name) {
                members.insert(event.name.to_string());
            }
        }
        for ty in ["Scratch.Generics.Value", "Scratch.Generics.List"] {
            if is_direct(ty) {
                members.insert(ty.to_string());
            }
        }
        members.into_iter().collect()
    }

    fn collect_imports(&mut self, statements: &[Stmt]) -> Result<(), String> {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => self.collect_imports(body)?,
                Stmt::Use {
                    token,
                    namespace,
                    alias,
                    alias_token,
                    members,
                    scope,
                    glob,
                    module,
                } => {
                    let mut additions = Vec::new();
                    if *glob {
                        let targets = if namespace.is_empty() {
                            let module = module.as_ref().expect("parser requires from for use *");
                            self.module_top_level
                                .get(&module.id)
                                .cloned()
                                .unwrap_or_default()
                        } else {
                            self.direct_members(namespace)
                        };
                        for target in targets {
                            let leaf = target.rsplit('.').next().unwrap_or(&target).to_string();
                            additions.push((leaf, target));
                        }
                    } else if !members.is_empty() {
                        for (_, member) in members {
                            let local = if let Some(alias) = alias {
                                format!("{alias}.{member}")
                            } else {
                                member.clone()
                            };
                            additions.push((local, format!("{namespace}.{member}")));
                        }
                    } else if let Some(alias) = alias {
                        additions.push((alias.clone(), namespace.clone()));
                    } else {
                        let name = namespace
                            .rsplit('.')
                            .next()
                            .unwrap_or(namespace)
                            .to_string();
                        additions.push((name, namespace.clone()));
                    }
                    let key = import_scope_key(token.file, scope);
                    for (local, target) in additions {
                        let previous = self
                            .imports
                            .entry(key.clone())
                            .or_default()
                            .insert(local.clone(), target.clone());
                        let entry = (key.clone(), local.clone());
                        let previous_was_glob = self.glob_imports.contains(&entry);
                        // The same declaration may be imported again when a `*` import is involved.
                        let allowed = match &previous {
                            None => true,
                            Some(previous) => previous == &target && (*glob || previous_was_glob),
                        };
                        if !allowed {
                            return Err(diagnostic(
                                alias_token.as_ref().unwrap_or(token),
                                format!("import name {local:?} is already declared in this namespace"),
                            ));
                        }
                        if *glob && previous.is_none() {
                            self.glob_imports.insert(entry);
                        } else if !*glob {
                            self.glob_imports.remove(&entry);
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn is_importable_symbol(&self, path: &str) -> bool {
        self.functions.contains_key(path)
            || self.classes.contains_key(path)
            || self.enums.contains_key(path)
            || builtin_block(path).is_some()
            || builtin_event(path).is_some()
            || builtin_type_path(path).is_some()
    }

    fn validate_imports(&self, statements: &[Stmt]) -> Result<(), String> {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => self.validate_imports(body)?,
                Stmt::Use {
                    token,
                    namespace,
                    members,
                    glob,
                    module,
                    ..
                } => {
                    let file = token.file;
                    if namespace.is_empty() {
                        // `from "..." use *` imports the library's top level.
                        continue;
                    }
                    if *glob || !members.is_empty() {
                        if !self.namespaces.contains(namespace) {
                            return Err(diagnostic(
                                token,
                                format!("unknown namespace {namespace:?}"),
                            ));
                        }
                    } else if !self.namespaces.contains(namespace)
                        && !self.is_importable_symbol(namespace)
                    {
                        return Err(diagnostic(
                            token,
                            format!("unknown namespace or symbol {namespace:?}"),
                        ));
                    }
                    if let Some(declared_in) = self.declaring_file(namespace) {
                        if let Some(module) = module {
                            if declared_in != module.id {
                                return Err(diagnostic(
                                    &module.token,
                                    format!(
                                        "library {:?} does not declare {namespace:?}",
                                        module.path
                                    ),
                                ));
                            }
                        } else if !self.file_can_see(file, declared_in) {
                            let library = self.module_display_name(declared_in);
                            return Err(diagnostic(
                                token,
                                format!(
                                    "{namespace:?} is declared in library {library:?}, which this file does not import; write `from {library:?} use ...`"
                                ),
                            ));
                        }
                    } else if let Some(module) = module {
                        return Err(diagnostic(
                            &module.token,
                            format!("library {:?} does not declare {namespace:?}", module.path),
                        ));
                    }
                    for (member_token, member) in members {
                        let target = format!("{namespace}.{member}");
                        if !self.is_importable_symbol(&target) {
                            return Err(diagnostic(
                                member_token,
                                format!("unknown function, class, enum, or event {target:?}"),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn import_for(&self, name: &str, scope: &str) -> Option<String> {
        let mut scopes = vec![scope.to_string()];
        let mut parent = scope;
        while let Some((prefix, _)) = parent.rsplit_once('.') {
            scopes.push(prefix.to_string());
            parent = prefix;
        }
        if !scope.is_empty() {
            scopes.push(String::new());
        }
        for lexical_scope in scopes {
            let key = import_scope_key(self.current_file, &lexical_scope);
            if let Some(imports) = self.imports.get(&key) {
                if let Some(target) = imports.get(name) {
                    return Some(target.clone());
                }
                if let Some((prefix, suffix)) = name.split_once('.') {
                    if let Some(target) = imports.get(prefix) {
                        return Some(format!("{target}.{suffix}"));
                    }
                }
            }
        }
        None
    }

    fn resolve_name(&self, name: &str) -> String {
        let scope = self
            .current_function
            .as_ref()
            .and_then(|function| function.name.rsplit_once('.').map(|(scope, _)| scope))
            .or_else(|| {
                self.current_method_class.and_then(|class_id| {
                    self.classes
                        .values()
                        .find(|class| class.id == class_id)
                        .and_then(|class| class.name.rsplit_once('.').map(|(scope, _)| scope))
                })
            })
            .unwrap_or("");
        if !name.contains('.') {
            let local = if scope.is_empty() {
                name.to_string()
            } else {
                format!("{scope}.{name}")
            };
            if self.functions.contains_key(&local)
                || self.classes.contains_key(&local)
                || self.enums.contains_key(&local)
            {
                return local;
            }
        }
        self.import_for(name, scope)
            .unwrap_or_else(|| name.to_string())
    }
}
