impl TypeChecker {
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
                } => {
                    let mut additions = Vec::new();
                    if !members.is_empty() {
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
                    let scope_imports = self.imports.entry(scope.clone()).or_default();
                    for (local, target) in additions {
                        if scope_imports.insert(local.clone(), target).is_some() {
                            return Err(diagnostic(
                                alias_token.as_ref().unwrap_or(token),
                                format!(
                                    "import name {local:?} is already declared in this namespace"
                                ),
                            ));
                        }
                    }
                }
                _ => {}
            }
        }
        Ok(())
    }

    fn validate_imports(&self, statements: &[Stmt]) -> Result<(), String> {
        for statement in statements {
            match statement {
                Stmt::Namespace { body, .. } => self.validate_imports(body)?,
                Stmt::Use {
                    token,
                    namespace,
                    members,
                    ..
                } => {
                    if members.is_empty() {
                        let final_path_is_symbol = self.functions.contains_key(namespace)
                            || self.classes.contains_key(namespace)
                            || builtin_block(namespace).is_some()
                            || builtin_event(namespace).is_some()
                            || builtin_type_path(namespace).is_some();
                        if !self.namespaces.contains(namespace) && !final_path_is_symbol {
                            return Err(diagnostic(
                                token,
                                format!("unknown namespace or symbol {namespace:?}"),
                            ));
                        }
                    } else {
                        if !self.namespaces.contains(namespace) {
                            return Err(diagnostic(
                                token,
                                format!("unknown namespace {namespace:?}"),
                            ));
                        }
                        for (member_token, member) in members {
                            let target = format!("{namespace}.{member}");
                            if !self.functions.contains_key(&target)
                                && !self.classes.contains_key(&target)
                                && builtin_block(&target).is_none()
                                && builtin_event(&target).is_none()
                                && builtin_type_path(&target).is_none()
                            {
                                return Err(diagnostic(
                                    member_token,
                                    format!("unknown function, class, or event {target:?}"),
                                ));
                            }
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
        scopes.push(String::new());
        for lexical_scope in scopes {
            if let Some(imports) = self.imports.get(&lexical_scope) {
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
            .unwrap_or("");
        if !name.contains('.') {
            let local = if scope.is_empty() {
                name.to_string()
            } else {
                format!("{scope}.{name}")
            };
            if self.functions.contains_key(&local) || self.classes.contains_key(&local) {
                return local;
            }
        }
        self.import_for(name, scope)
            .unwrap_or_else(|| name.to_string())
    }
}
