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

pub struct Compilation {
    pub project: Value,
    pub assets: Vec<(String, Vec<u8>)>,
    pub block_count: usize,
    /// A debug dump of the typed, function-call-lowered syntax tree used by the backend.
    pub ir: String,
}

pub fn compile_source(source: &str) -> Result<(Value, Vec<(String, Vec<u8>)>, usize), String> {
    let (project, assets, block_count, _) = compile_source_internal(source, false)?;
    Ok((project, assets, block_count))
}

pub fn compile_source_with_ir(source: &str) -> Result<Compilation, String> {
    let (project, assets, block_count, ir) = compile_source_internal(source, true)?;
    Ok(Compilation {
        project,
        assets,
        block_count,
        ir: ir.expect("IR requested"),
    })
}

fn compile_source_internal(
    source: &str,
    emit_ir: bool,
) -> Result<(Value, Vec<(String, Vec<u8>)>, usize, Option<String>), String> {
    let tokens = lex(source)?;
    let mut statements = Parser::new(tokens).parse()?;
    let mut symbols = TypeChecker::new().check(&mut statements)?;
    lower_function_calls(&mut statements, &mut symbols)?;
    let ir = emit_ir.then(|| {
        format!("Rake IR (typed syntax tree after function-call lowering)\n{statements:#?}\n")
    });
    let mut builder = ScratchBuilder::new();
    let hat = builder.block("event_whenflagclicked", None, true, json!({}));
    let first = builder.sequence(&statements, &hat, None)?;
    builder.blocks.get_mut(&hat).unwrap()["next"] = json!(first);
    emit_function_definitions(&mut builder, &statements)?;
    let block_count = builder.blocks.len();

    let backdrop_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"480\" height=\"360\" viewBox=\"0 0 480 360\"><rect width=\"480\" height=\"360\" fill=\"#ffffff\"/></svg>";
    let sprite_svg = b"<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"100\" height=\"100\" viewBox=\"0 0 100 100\"><circle cx=\"50\" cy=\"50\" r=\"40\" fill=\"#4c97ff\" stroke=\"#3373cc\" stroke-width=\"4\"/></svg>";
    // These IDs are the MD5 hashes of the exact built-in SVG bytes above.
    let backdrop_id = "ff3f2e0196df3c7d286c4c13e441b003";
    let sprite_id = "c5091827e2b0bff8cb6133200ffa1fee";
    let backdrop_costume = json!({"assetId": backdrop_id, "name": "backdrop1", "dataFormat": "svg", "md5ext": format!("{backdrop_id}.svg"), "rotationCenterX": 240, "rotationCenterY": 180});
    let sprite_costume = json!({"assetId": sprite_id, "name": "costume1", "dataFormat": "svg", "md5ext": format!("{sprite_id}.svg"), "bitmapResolution": 1, "rotationCenterX": 50, "rotationCenterY": 50});

    let mut stage_variables = Map::new();
    let mut stage_lists = Map::new();
    let mut target_variables = Map::new();
    for symbol in &symbols {
        match symbol.storage {
            StorageClass::Project => {
                stage_variables.insert(
                    symbol.id.clone(),
                    json!([symbol.scratch_name, symbol.initial_value.clone()]),
                );
            }
            StorageClass::Target => {
                target_variables.insert(
                    symbol.id.clone(),
                    json!([symbol.scratch_name, symbol.initial_value.clone()]),
                );
            }
            StorageClass::ProjectList => {
                stage_lists.insert(
                    symbol.id.clone(),
                    json!([symbol.scratch_name, symbol.initial_value.clone()]),
                );
            }
            StorageClass::ProcedureArgument | StorageClass::ObjectBinding => {}
        }
    }
    let stage = json!({
        "isStage": true, "name": "Stage", "variables": stage_variables, "lists": stage_lists, "broadcasts": builder.broadcasts.clone(), "blocks": {}, "comments": {},
        "currentCostume": 0, "costumes": [backdrop_costume], "sounds": [], "volume": 100, "layerOrder": 0, "tempo": 60,
        "videoTransparency": 50, "videoState": "on", "textToSpeechLanguage": null
    });
    let sprite = json!({
        "isStage": false, "name": "Sprite1", "variables": target_variables, "lists": {}, "broadcasts": {}, "blocks": builder.blocks, "comments": {},
        "currentCostume": 0, "costumes": [sprite_costume], "sounds": [], "volume": 100, "layerOrder": 1,
        "visible": true, "x": 0, "y": 0, "size": 100, "direction": 90, "draggable": false, "rotationStyle": "all around"
    });
    let project = json!({
        "targets": [stage, sprite], "monitors": [], "extensions": [],
        "meta": {"semver": "3.0.0", "vm": "15.3.0", "agent": "Rake compiler"}
    });
    let assets = vec![
        (format!("{backdrop_id}.svg"), backdrop_svg.to_vec()),
        (format!("{sprite_id}.svg"), sprite_svg.to_vec()),
    ];
    Ok((project, assets, block_count, ir))
}
