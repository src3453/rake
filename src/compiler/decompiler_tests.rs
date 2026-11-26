#[cfg(test)]
mod decompiler_tests {
    use super::*;

    const SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"20\" height=\"20\"><circle cx=\"10\" cy=\"10\" r=\"10\"/></svg>";

    fn temp_dir(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("rake-decompile-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        directory
    }

    /// Decompile, write the project, and compile the generated scene again.
    fn decompile_and_recompile(name: &str, project: &Value, assets: &HashMap<String, Vec<u8>>) -> (Decompilation, SceneCompilation) {
        let decompilation = decompile_project(project, assets).expect("project should decompile");
        let directory = temp_dir(name);
        write_decompilation(&directory, &decompilation).expect("files should be written");
        let compiled = compile_scene(&directory.join(&decompilation.scene_path)).unwrap_or_else(|error| {
            let source = fs::read_to_string(&error.file).unwrap_or_default();
            panic!("decompiled project should compile: {}: {}\n{source}", error.file.display(), error.message)
        });
        (decompilation, compiled)
    }

    fn file_text(decompilation: &Decompilation, path: &str) -> String {
        let file = decompilation
            .files
            .iter()
            .find(|file| file.path == path)
            .unwrap_or_else(|| panic!("missing {path}"));
        String::from_utf8(file.contents.clone()).unwrap()
    }

    fn sample_project() -> (Value, HashMap<String, Vec<u8>>) {
        let svg_id = md5_hex(SVG.as_bytes());
        let costume = json!({
            "name": "costume1", "assetId": svg_id, "md5ext": format!("{svg_id}.svg"), "dataFormat": "svg",
            "rotationCenterX": 10, "rotationCenterY": 10, "bitmapResolution": 1
        });
        let stage_blocks = json!({
            "s1": {"opcode": "event_whenbroadcastreceived", "next": "s2", "parent": null, "inputs": {},
                   "fields": {"BROADCAST_OPTION": ["game over", "bc1"]}, "shadow": false, "topLevel": true, "x": 0, "y": 0},
            "s2": {"opcode": "control_stop", "next": null, "parent": "s1", "inputs": {},
                   "fields": {"STOP_OPTION": ["all", null]}, "shadow": false, "topLevel": false, "mutation": {"tagName": "mutation", "children": [], "hasnext": "false"}}
        });
        let sprite_blocks = json!({
            // when flag clicked: set スコア to 0; forever { if key space { change スコア by 1; move speed }; if スコア > 10 { broadcast game over } }
            "a1": {"opcode": "event_whenflagclicked", "next": "a2", "parent": null, "inputs": {}, "fields": {}, "shadow": false, "topLevel": true, "x": 0, "y": 0},
            "a2": {"opcode": "data_setvariableto", "next": "a3", "parent": "a1", "inputs": {"VALUE": [1, [10, "0"]]},
                   "fields": {"VARIABLE": ["スコア", "var1"]}, "shadow": false, "topLevel": false},
            "a3": {"opcode": "control_forever", "next": null, "parent": "a2", "inputs": {"SUBSTACK": [2, "a4"]}, "fields": {}, "shadow": false, "topLevel": false},
            "a4": {"opcode": "control_if", "next": "a9", "parent": "a3", "inputs": {"CONDITION": [2, "a5"], "SUBSTACK": [2, "a7"]}, "fields": {}, "shadow": false, "topLevel": false},
            "a5": {"opcode": "sensing_keypressed", "next": null, "parent": "a4", "inputs": {"KEY_OPTION": [1, "a6"]}, "fields": {}, "shadow": false, "topLevel": false},
            "a6": {"opcode": "sensing_keyoptions", "next": null, "parent": "a5", "inputs": {}, "fields": {"KEY_OPTION": ["space", null]}, "shadow": true, "topLevel": false},
            "a7": {"opcode": "data_changevariableby", "next": "a8", "parent": "a4", "inputs": {"VALUE": [1, [4, "1"]]},
                   "fields": {"VARIABLE": ["スコア", "var1"]}, "shadow": false, "topLevel": false},
            "a8": {"opcode": "motion_movesteps", "next": null, "parent": "a7", "inputs": {"STEPS": [3, [12, "speed", "var2"], [4, "10"]]}, "fields": {}, "shadow": false, "topLevel": false},
            "a9": {"opcode": "control_if", "next": "a12", "parent": "a4", "inputs": {"CONDITION": [2, "a10"], "SUBSTACK": [2, "a11"]}, "fields": {}, "shadow": false, "topLevel": false},
            "a10": {"opcode": "operator_gt", "next": null, "parent": "a9", "inputs": {"OPERAND1": [3, [12, "スコア", "var1"], [10, ""]], "OPERAND2": [1, [10, "10"]]}, "fields": {}, "shadow": false, "topLevel": false},
            "a11": {"opcode": "event_broadcast", "next": null, "parent": "a9", "inputs": {"BROADCAST_INPUT": [1, [11, "game over", "bc1"]]}, "fields": {}, "shadow": false, "topLevel": false},
            "a12": {"opcode": "pen_clear", "next": null, "parent": "a9", "inputs": {}, "fields": {}, "shadow": false, "topLevel": false},
            // when flag clicked: repeat 3 { say "hi"; jump 10 }; add "apple" to items
            "b1": {"opcode": "event_whenflagclicked", "next": "b2", "parent": null, "inputs": {}, "fields": {}, "shadow": false, "topLevel": true, "x": 0, "y": 300},
            "b2": {"opcode": "control_repeat", "next": "b5", "parent": "b1", "inputs": {"TIMES": [1, [6, "3"]], "SUBSTACK": [2, "b3"]}, "fields": {}, "shadow": false, "topLevel": false},
            "b3": {"opcode": "looks_say", "next": "b4", "parent": "b2", "inputs": {"MESSAGE": [1, [10, "hi \"there\""]]}, "fields": {}, "shadow": false, "topLevel": false},
            "b4": {"opcode": "procedures_call", "next": null, "parent": "b3", "inputs": {"argA": [1, [10, "10"]]}, "fields": {}, "shadow": false, "topLevel": false,
                   "mutation": {"tagName": "mutation", "children": [], "proccode": "jump %s", "argumentids": "[\"argA\"]", "warp": "false"}},
            "b5": {"opcode": "data_addtolist", "next": null, "parent": "b2", "inputs": {"ITEM": [1, [10, "apple"]]}, "fields": {"LIST": ["items", "list1"]}, "shadow": false, "topLevel": false},
            // define jump (height) — run without screen refresh
            "c1": {"opcode": "procedures_definition", "next": "c4", "parent": null, "inputs": {"custom_block": [1, "c2"]}, "fields": {}, "shadow": false, "topLevel": true, "x": 400, "y": 0},
            "c2": {"opcode": "procedures_prototype", "next": null, "parent": "c1", "inputs": {"argA": [1, "c3"]}, "fields": {}, "shadow": true, "topLevel": false,
                   "mutation": {"tagName": "mutation", "children": [], "proccode": "jump %s", "argumentids": "[\"argA\"]",
                                "argumentnames": "[\"height\"]", "argumentdefaults": "[\"\"]", "warp": "true"}},
            "c3": {"opcode": "argument_reporter_string_number", "next": null, "parent": "c2", "inputs": {}, "fields": {"VALUE": ["height", null]}, "shadow": true, "topLevel": false},
            "c4": {"opcode": "motion_changeyby", "next": null, "parent": "c1", "inputs": {"DY": [3, "c5", [4, "10"]]}, "fields": {}, "shadow": false, "topLevel": false},
            "c5": {"opcode": "argument_reporter_string_number", "next": null, "parent": "c4", "inputs": {}, "fields": {"VALUE": ["height", null]}, "shadow": false, "topLevel": false},
            // when timer > 10 (no Rake event yet)
            "d1": {"opcode": "event_whengreaterthan", "next": "d2", "parent": null, "inputs": {"VALUE": [1, [4, "10"]]}, "fields": {"WHENGREATERTHANMENU": ["TIMER", null]}, "shadow": false, "topLevel": true, "x": 400, "y": 400},
            "d2": {"opcode": "looks_hide", "next": null, "parent": "d1", "inputs": {}, "fields": {}, "shadow": false, "topLevel": false}
        });
        let project = json!({
            "targets": [
                {"isStage": true, "name": "Stage", "variables": {"var1": ["スコア", 0]},
                 "lists": {"list1": ["items", ["a", "b"]]}, "broadcasts": {"bc1": "game over"}, "blocks": stage_blocks,
                 "comments": {}, "currentCostume": 0, "costumes": [costume.clone()], "sounds": [], "volume": 100, "layerOrder": 0, "tempo": 60},
                {"isStage": false, "name": "Player 1", "variables": {"var2": ["speed", 5]}, "lists": {}, "broadcasts": {},
                 "blocks": sprite_blocks, "comments": {}, "currentCostume": 0, "costumes": [costume], "sounds": [], "volume": 100,
                 "layerOrder": 1, "visible": true, "x": -20, "y": 35.5, "size": 80, "direction": 90, "draggable": false, "rotationStyle": "left-right"}
            ],
            "monitors": [], "extensions": ["pen"], "meta": {"semver": "3.0.0"}
        });
        let mut assets = HashMap::new();
        assets.insert(format!("{svg_id}.svg"), SVG.as_bytes().to_vec());
        (project, assets)
    }

    #[test]
    fn decompiles_a_scratch_project_into_a_compilable_rake_project() {
        let (project, assets) = sample_project();
        let (decompilation, compiled) = decompile_and_recompile("sample", &project, &assets);

        let player = file_text(&decompilation, "src/Player_1.rk");
        for expected in [
            "var variable: float = 0 // Scratch name: \"スコア\"",
            "var speed: float = 5 // local to \"Player 1\"",
            "var items: Scratch.Generics.List = SList.new() // 2 initial item(s) not restored",
            "// Scratch custom block: \"jump %s\"\nwarp fn jump(height: Scratch.Generics.Value) {",
            "    Motion.changeYBy(height)",
            "while true {",
            "if Sensing.keyPressed(\"space\") {",
            "variable += 1",
            "Motion.move(speed)",
            "if variable > 10 {",
            "send game_over",
            "use Scratch.Extension.Pen as Pen",
            "Pen.clear()",
            "var repeat_index: float = 0",
            "while repeat_index < 3 {",
            "Looks.say(\"hi \\\"there\\\"\")",
            "jump(10)",
            "items.add(\"apple\")",
            "// [decompile] unsupported hat `event_whengreaterthan`:",
            "//     Looks.hide()",
            // Two green flag scripts next to module variables: the second is relayed.
            "send rake_green_flag_1",
            "event rake_green_flag_1 {",
        ] {
            assert!(player.contains(expected), "missing {expected:?} in\n{player}");
        }
        let stage = file_text(&decompilation, "src/stage.rk");
        assert!(stage.contains("event game_over {\n    Control.stopAll()\n}"), "{stage}");

        let scene = file_text(&decompilation, "scenes/main.rks");
        assert!(scene.contains("sprite Player_1 {"), "{scene}");
        assert!(scene.contains("costumes: [(\"../assets/Player_1/costume1.svg\", 10, 10)]"), "{scene}");
        assert!(scene.contains("position: (-20, 35.5)"), "{scene}");
        assert!(scene.contains("rotation_style: \"left-right\""), "{scene}");

        assert!(!decompilation.warnings.iter().any(|warning| warning.contains("pen_clear")));
        assert!(decompilation.warnings.iter().any(|warning| warning.contains("sprite-local variable \"speed\"")));

        // The recompiled project keeps the targets, variables, and custom block.
        let targets = compiled.project["targets"].as_array().unwrap();
        assert_eq!(targets.len(), 2);
        assert_eq!(targets[1]["name"], json!("Player_1"));
        let blocks = targets[1]["blocks"].as_object().unwrap();
        let opcodes = blocks.values().filter_map(|block| block["opcode"].as_str()).collect::<HashSet<_>>();
        for opcode in ["procedures_definition", "sensing_keypressed", "data_addtolist", "event_broadcast", "motion_changeyby", "pen_clear"] {
            assert!(opcodes.contains(opcode), "missing {opcode}");
        }
        assert_eq!(compiled.project["extensions"], json!(["pen"]));
        let prototype = blocks.values().find(|block| block["opcode"] == json!("procedures_prototype")).unwrap();
        assert_eq!(prototype["mutation"]["warp"], json!("true"));
    }

    #[test]
    fn decompiles_sb3_archives() {
        let (project, assets) = sample_project();
        let directory = temp_dir("archive");
        fs::create_dir_all(&directory).unwrap();
        let archive = directory.join("sample.sb3");
        let assets = assets.into_iter().collect::<Vec<_>>();
        write_sb3(&archive, &project, &assets).unwrap();
        let decompilation = decompile_sb3_bytes(&fs::read(&archive).unwrap()).expect("archive should decompile");
        assert!(decompilation.files.iter().any(|file| file.path == "assets/Player_1/costume1.svg"));
        assert!(decompile_sb3_bytes(b"not a zip").unwrap_err().contains("not a valid .sb3"));
    }

    #[test]
    fn rake_examples_survive_a_compile_decompile_compile_round_trip() {
        let examples = Path::new(env!("CARGO_MANIFEST_DIR")).join("examples");
        let mut names = fs::read_dir(&examples)
            .unwrap()
            .filter_map(|entry| entry.ok().map(|entry| entry.path()))
            .filter(|path| path.extension().is_some_and(|extension| extension == "rk"))
            .collect::<Vec<_>>();
        names.sort();
        assert!(!names.is_empty());
        for path in names {
            let source = fs::read_to_string(&path).unwrap();
            let (project, assets, _) = compile_source(&source).unwrap();
            let assets = assets.into_iter().collect::<HashMap<_, _>>();
            let name = path.file_stem().unwrap().to_string_lossy().to_string();
            let (decompilation, _) = decompile_and_recompile(&format!("example-{name}"), &project, &assets);
            assert!(
                decompilation.warnings.iter().all(|warning| !warning.contains("unsupported")),
                "{name}: {:?}",
                decompilation.warnings
            );
        }
    }

    #[test]
    fn identifiers_are_sanitized_for_rake() {
        assert_eq!(rake_identifier("my score", "variable"), "my_score");
        assert_eq!(rake_identifier("1st", "variable"), "variable_1st");
        assert_eq!(rake_identifier("スコア", "variable"), "variable");
        assert_eq!(rake_identifier("while", "variable"), "while_");
        assert_eq!(procedure_identifier("move player %s steps %b"), "movePlayerSteps");
        assert_eq!(number_literal("1.50").unwrap().text, "1.5");
        assert_eq!(number_literal("-3").unwrap().text, "-3");
        assert!(number_literal("abc").is_none());
    }
}
