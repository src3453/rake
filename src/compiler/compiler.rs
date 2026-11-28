fn emit_function_definitions(
    builder: &mut ScratchBuilder,
    statements: &[Stmt],
) -> Result<(), String> {
    for statement in statements {
        match statement {
            Stmt::Namespace { body, .. } => emit_function_definitions(builder, body)?,
            Stmt::Function { .. } => builder.procedure_definition(statement)?,
            _ => {}
        }
    }
    Ok(())
}

fn has_module_runtime_statements(statements: &[Stmt]) -> bool {
    statements.iter().any(|statement| match statement {
        Stmt::Use { .. }
        | Stmt::Namespace { .. }
        | Stmt::Enum { .. }
        | Stmt::Class { .. }
        | Stmt::Function { .. }
        | Stmt::Event { .. } => false,
        Stmt::VarDecl {
            symbol: Some(symbol),
            object_fields,
            initializer,
            ..
        } if symbol.storage == StorageClass::ProjectList
            || (symbol.storage == StorageClass::ObjectBinding
                && object_fields.is_empty()
                && !matches!(&initializer.kind, ExprKind::ObjectInit { constructor: Some(_), .. })) =>
        {
            false
        }
        _ => true,
    })
}

fn emit_green_flag_script(builder: &mut ScratchBuilder, statements: &[Stmt]) -> Result<(), String> {
    let hat = builder.block("event_whenflagclicked", None, true, json!({}));
    if let Some(first) = builder.sequence(statements, &hat, None)? {
        builder.blocks.get_mut(&hat).unwrap()["next"] = json!(first);
    } else {
        builder.blocks.remove(&hat);
        builder.top_level_count -= 1;
    }
    Ok(())
}

pub struct Compilation {
    pub project: Value,
    pub assets: Vec<(String, Vec<u8>)>,
    pub block_count: usize,
    /// A debug dump of the typed, function-call-lowered syntax tree used by the backend.
    pub ir: String,
}

pub fn compile_source(source: &str) -> Result<(Value, Vec<(String, Vec<u8>)>, usize), String> {
    compile_source_for_runtime(source, Runtime::Scratch)
}

pub fn compile_source_for_runtime(source: &str, runtime: Runtime) -> Result<(Value, Vec<(String, Vec<u8>)>, usize), String> {
    let (project, assets, block_count, _) = compile_source_internal(source, false, runtime)?;
    Ok((project, assets, block_count))
}

pub fn compile_source_with_ir(source: &str) -> Result<Compilation, String> {
    compile_source_with_ir_for_runtime(source, Runtime::Scratch)
}

pub fn compile_source_with_ir_for_runtime(source: &str, runtime: Runtime) -> Result<Compilation, String> {
    let (project, assets, block_count, ir) = compile_source_internal(source, true, runtime)?;
    Ok(Compilation {
        project,
        assets,
        block_count,
        ir: ir.expect("IR requested"),
    })
}

/// Scripts, variables, and broadcasts generated from one `.rk` source for one Scratch target.
struct CompiledUnit {
    blocks: Map<String, Value>,
    broadcasts: Map<String, Value>,
    stage_variables: Map<String, Value>,
    stage_lists: Map<String, Value>,
    target_variables: Map<String, Value>,
    block_count: usize,
    ir: Option<String>,
    /// Custom blocks that came from library modules.
    library_procedures: Vec<LibraryProcedure>,
}

fn compile_unit(source: &str, emit_ir: bool, runtime: Runtime) -> Result<CompiledUnit, String> {
    compile_unit_in(source, None, emit_ir, runtime).map_err(|error| error.message)
}

/// Compile one `.rk` source, loading its `from` libraries when `modules` locates it.
fn compile_unit_in(
    source: &str,
    modules: Option<&ModuleContext>,
    emit_ir: bool,
    runtime: Runtime,
) -> Result<CompiledUnit, SourceError> {
    let mut loader = ModuleLoader::new(modules);
    let result = loader.load(source).and_then(|statements| {
        let module_names = loader.module_names();
        compile_statements(statements, &module_names, emit_ir, runtime)
    });
    result.map_err(|message| loader.source_error(&message))
}

fn compile_statements(
    mut statements: Vec<Stmt>,
    module_names: &HashMap<u32, String>,
    emit_ir: bool,
    runtime: Runtime,
) -> Result<CompiledUnit, String> {
    let mut symbols = TypeChecker::new(runtime)
        .with_modules(module_names.clone())
        .check(&mut statements)?;
    let unused_library_symbols = prune_unused_library_functions(&mut statements);
    let mut library_procedure_list = Vec::new();
    library_procedures(&statements, module_names, &mut library_procedure_list);
    lower_function_calls(&mut statements, &mut symbols)?;
    symbols.retain(|symbol| !unused_library_symbols.contains(&symbol.id));
    let ir = emit_ir.then(|| {
        format!("Rake IR (typed syntax tree after function-call lowering)
{statements:#?}
")
    });
    let mut builder = ScratchBuilder::new();
    let events = statements
        .iter()
        .filter(|statement| matches!(statement, Stmt::Event { .. }))
        .collect::<Vec<_>>();
    let has_module_runtime = has_module_runtime_statements(&statements);
    if events.is_empty() {
        emit_green_flag_script(&mut builder, &statements)?;
    } else if has_module_runtime {
        let green_flags = events
            .iter()
            .filter(|event| matches!(event, Stmt::Event { kind: Some(EventKind::GreenFlag), .. }))
            .collect::<Vec<_>>();
        if green_flags.len() > 1 {
            let Stmt::Event { type_token, .. } = green_flags[1] else {
                unreachable!()
            };
            return Err(diagnostic(
                type_token,
                "module-level executable statements cannot be combined with multiple greenFlag handlers; move them into one handler or remove the duplicate handlers",
            ));
        }
        if green_flags.is_empty() {
            let legacy_statements = statements
                .iter()
                .filter(|statement| !matches!(statement, Stmt::Event { .. }))
                .cloned()
                .collect::<Vec<_>>();
            emit_green_flag_script(&mut builder, &legacy_statements)?;
        }
        for event in events {
            let leading = if matches!(event, Stmt::Event { kind: Some(EventKind::GreenFlag), .. }) {
                statements
                    .iter()
                    .filter(|statement| !matches!(statement, Stmt::Event { .. }))
                    .cloned()
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            builder.emit_event_script(event, &leading)?;
        }
    } else {
        for event in events {
            builder.emit_event_script(event, &[])?;
        }
    }
    emit_function_definitions(&mut builder, &statements)?;
    let block_count = builder.blocks.len();

    let mut stage_variables = Map::new();
    let mut stage_lists = Map::new();
    let mut target_variables = Map::new();
    for symbol in &symbols {
        let entry = json!([symbol.scratch_name, symbol.initial_value.clone()]);
        match symbol.storage {
            StorageClass::Project => {
                stage_variables.insert(symbol.id.clone(), entry);
            }
            StorageClass::Target => {
                target_variables.insert(symbol.id.clone(), entry);
            }
            StorageClass::ProjectList => {
                stage_lists.insert(symbol.id.clone(), entry);
            }
            StorageClass::ProcedureArgument | StorageClass::ObjectBinding => {}
        }
    }
    Ok(CompiledUnit {
        blocks: builder.blocks,
        broadcasts: builder.broadcasts,
        stage_variables,
        stage_lists,
        target_variables,
        block_count,
        ir,
        library_procedures: library_procedure_list,
    })
}

const DEFAULT_BACKDROP_SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"480\" height=\"360\" viewBox=\"0 0 480 360\"><rect width=\"480\" height=\"360\" fill=\"#ffffff\"/></svg>";
const DEFAULT_SPRITE_SVG: &[u8] = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\" viewBox=\"0 0 100 100\"><circle cx=\"50\" cy=\"50\" r=\"40\" fill=\"#4c97ff\" stroke=\"#3373cc\" stroke-width=\"4\"/></svg>";
// These IDs are the MD5 hashes of the exact built-in SVG bytes above.
const DEFAULT_BACKDROP_ID: &str = "ff3f2e0196df3c7d286c4c13e441b003";
const DEFAULT_SPRITE_ID: &str = "c5091827e2b0bff8cb6133200ffa1fee";

fn default_backdrop_costume() -> Value {
    json!({"assetId": DEFAULT_BACKDROP_ID, "name": "backdrop1", "dataFormat": "svg", "md5ext": format!("{DEFAULT_BACKDROP_ID}.svg"), "bitmapResolution": 1, "rotationCenterX": 240, "rotationCenterY": 180})
}

fn default_sprite_costume() -> Value {
    json!({"assetId": DEFAULT_SPRITE_ID, "name": "costume1", "dataFormat": "svg", "md5ext": format!("{DEFAULT_SPRITE_ID}.svg"), "bitmapResolution": 1, "rotationCenterX": 50, "rotationCenterY": 50})
}

fn stage_target(variables: Map<String, Value>, lists: Map<String, Value>, broadcasts: Map<String, Value>, blocks: Map<String, Value>, costumes: Vec<Value>, current_costume: usize, sounds: Vec<Value>) -> Value {
    json!({
        "isStage": true, "name": "Stage", "variables": variables, "lists": lists, "broadcasts": broadcasts, "blocks": blocks, "comments": {},
        "currentCostume": current_costume, "costumes": costumes, "sounds": sounds, "volume": 100, "layerOrder": 0, "tempo": 60,
        "videoTransparency": 50, "videoState": "on", "textToSpeechLanguage": null
    })
}

fn project_document(targets: Vec<Value>) -> Value {
    let mut extensions = std::collections::BTreeSet::new();
    for target in &targets {
        if let Some(blocks) = target.get("blocks").and_then(Value::as_object) {
            for block in blocks.values() {
                if let Some(opcode) = block.get("opcode").and_then(Value::as_str) {
                    if opcode.starts_with("pen_") {
                        extensions.insert("pen");
                    } else if opcode.starts_with("music_") {
                        extensions.insert("music");
                    }
                }
            }
        }
    }
    json!({
        "targets": targets, "monitors": [], "extensions": extensions,
        "meta": {"semver": "3.0.0", "vm": "15.3.0", "agent": "Rake compiler"}
    })
}

type ProjectOutput = (Value, Vec<(String, Vec<u8>)>, usize, Option<String>);

/// Compile `source` as the file at `context.entry_path`, loading the libraries its
/// `from "..." use` declarations name relative to `context.root`.
pub fn compile_source_in_project(
    source: &str,
    context: &ModuleContext,
    runtime: Runtime,
) -> Result<(Value, Vec<(String, Vec<u8>)>, usize), SourceError> {
    let unit = compile_unit_in(source, Some(context), false, runtime)?;
    let (project, assets, block_count, _) = single_target_project(unit);
    Ok((project, assets, block_count))
}

/// Read and compile a `.rk` file. Its project root is the nearest ancestor directory
/// with `rake-ide.json` (or the file's directory), and `from` paths resolve from there.
pub fn compile_file_for_runtime(
    path: &Path,
    runtime: Runtime,
) -> Result<(Value, Vec<(String, Vec<u8>)>, usize), SourceError> {
    let compilation = compile_file_internal(path, false, runtime)?;
    Ok((compilation.0, compilation.1, compilation.2))
}

/// Like [`compile_file_for_runtime`], also returning the typed IR dump.
pub fn compile_file_with_ir_for_runtime(path: &Path, runtime: Runtime) -> Result<Compilation, SourceError> {
    let (project, assets, block_count, ir) = compile_file_internal(path, true, runtime)?;
    Ok(Compilation {
        project,
        assets,
        block_count,
        ir: ir.expect("IR requested"),
    })
}

fn compile_file_internal(path: &Path, emit_ir: bool, runtime: Runtime) -> Result<ProjectOutput, SourceError> {
    let source = fs::read_to_string(path).map_err(|error| SourceError {
        file: Some(path.to_path_buf()),
        message: format!("1:1: cannot read {}: {error}", path.display()),
    })?;
    let root = find_project_root(path);
    let context = ModuleContext {
        entry_path: path,
        root: &root,
        sources: &FileSystemSources,
    };
    let unit = compile_unit_in(&source, Some(&context), emit_ir, runtime).map_err(|error| SourceError {
        file: Some(error.file.unwrap_or_else(|| path.to_path_buf())),
        message: error.message,
    })?;
    Ok(single_target_project(unit))
}

fn compile_source_internal(
    source: &str,
    emit_ir: bool,
    runtime: Runtime,
) -> Result<ProjectOutput, String> {
    let unit = compile_unit(source, emit_ir, runtime)?;
    Ok(single_target_project(unit))
}

/// Wrap one compiled unit in a project with the Stage and a single sprite.
fn single_target_project(unit: CompiledUnit) -> ProjectOutput {
    let stage = stage_target(
        unit.stage_variables,
        unit.stage_lists,
        unit.broadcasts,
        Map::new(),
        vec![default_backdrop_costume()],
        0,
        Vec::new(),
    );
    let sprite = json!({
        "isStage": false, "name": "Sprite1", "variables": unit.target_variables, "lists": {}, "broadcasts": {}, "blocks": unit.blocks, "comments": {},
        "currentCostume": 0, "costumes": [default_sprite_costume()], "sounds": [], "volume": 100, "layerOrder": 1,
        "visible": true, "x": 0, "y": 0, "size": 100, "direction": 90, "draggable": false, "rotationStyle": "all around"
    });
    let project = project_document(vec![stage, sprite]);
    let assets = vec![
        (format!("{DEFAULT_BACKDROP_ID}.svg"), DEFAULT_BACKDROP_SVG.to_vec()),
        (format!("{DEFAULT_SPRITE_ID}.svg"), DEFAULT_SPRITE_SVG.to_vec()),
    ];
    (project, assets, unit.block_count, unit.ir)
}
