#[cfg(test)]
mod tests {
    use super::*;

    fn source_project(source: &str) -> (Value, Vec<(String, Vec<u8>)>) {
        let (project, assets, _) = compile_source(source).expect("compile source");
        (project, assets)
    }

    #[test]
    fn emits_sb3_with_control_flow_and_standard_blocks() {
        let (project, assets) = source_project(
            "var count: int = 0\nvar enabled: bool = true\nif enabled && count < 3 { count += 1 }\nwhile count < 3 { count = count + 1 }",
        );
        assert_eq!(project["targets"][0]["name"], "Stage");
        assert_eq!(project["targets"][1]["name"], "Sprite1");
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let opcodes: Vec<&str> = blocks
            .values()
            .map(|block| block["opcode"].as_str().unwrap())
            .collect();
        for opcode in [
            "event_whenflagclicked",
            "control_if",
            "control_repeat_until",
            "operator_and",
            "operator_lt",
            "operator_add",
        ] {
            assert!(opcodes.contains(&opcode), "missing opcode {opcode}");
        }
        assert_eq!(assets.len(), 2);
    }

    #[test]
    fn warp_functions_disable_screen_refresh_on_custom_blocks() {
        let (project, _) = source_project("warp fn fast() {}\nfn normal() {}");
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let warp_values: Vec<_> = blocks
            .values()
            .filter(|block| block["opcode"] == "procedures_definition")
            .map(|definition| {
                let prototype = definition["inputs"]["custom_block"][1]
                    .as_str()
                    .unwrap();
                (
                    blocks[prototype]["mutation"]["proccode"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                    blocks[prototype]["mutation"]["warp"]
                        .as_str()
                        .unwrap()
                        .to_string(),
                )
            })
            .collect();
        assert_eq!(warp_values.len(), 2);
        assert!(warp_values.contains(&("fast".to_string(), "true".to_string())));
        assert!(warp_values.contains(&("normal".to_string(), "false".to_string())));
    }

    #[test]
    fn emits_independent_scratch_event_scripts() {
        let source = r#"
event greenFlag() main {
    Scratch.Looks.say("start")
}
event keyPressed("space") onSpace {
    Scratch.Looks.say("space")
}
event broadcastReceived("reset") onReset {
    Scratch.Looks.say("reset")
}
event backdropSwitchedTo("backdrop1") onBackdrop {
    Scratch.Looks.say("backdrop")
}
event spriteClicked() onClick {}
event cloneStart() onClone {}
"#;
        let (project, _) = source_project(source);
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let hats: Vec<_> = blocks
            .values()
            .filter(|block| block["topLevel"] == true)
            .collect();
        assert_eq!(hats.len(), 6);

        let by_opcode = |opcode: &str| {
            hats.iter()
                .find(|block| block["opcode"] == opcode)
                .copied()
                .unwrap_or_else(|| panic!("missing event hat {opcode}"))
        };
        let green_flag = by_opcode("event_whenflagclicked");
        assert!(!green_flag["next"].is_null());
        let key = by_opcode("event_whenkeypressed");
        assert_eq!(key["fields"]["KEY_OPTION"][0], "space");
        assert!(!key["next"].is_null());
        let broadcast = by_opcode("event_whenbroadcastreceived");
        assert_eq!(broadcast["fields"]["BROADCAST_OPTION"][0], "reset");
        let broadcast_id = project["targets"][0]["broadcasts"]
            .as_object()
            .unwrap()
            .iter()
            .find(|(_, value)| *value == "reset")
            .unwrap()
            .0;
        assert_eq!(
            broadcast["fields"]["BROADCAST_OPTION"][1].as_str(),
            Some(broadcast_id.as_str())
        );
        let backdrop = by_opcode("event_whenbackdropswitchesto");
        assert_eq!(backdrop["fields"]["BACKDROP"][0], "backdrop1");
        by_opcode("event_whenthisspriteclicked");
        by_opcode("control_start_as_clone");

        let mut y_coordinates: Vec<_> = hats
            .iter()
            .map(|block| block["y"].as_i64().unwrap())
            .collect();
        y_coordinates.sort_unstable();
        y_coordinates.dedup();
        assert_eq!(y_coordinates.len(), hats.len());
    }

    #[test]
    fn merges_module_runtime_statements_into_single_explicit_green_flag() {
        let (project, _) = source_project(
            "var score: int = 0\nevent greenFlag() main { score += 1 }",
        );
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let hats: Vec<_> = blocks
            .values()
            .filter(|block| block["topLevel"] == true)
            .collect();
        assert_eq!(hats.len(), 1);
        assert_eq!(hats[0]["opcode"], "event_whenflagclicked");
        let first = hats[0]["next"].as_str().unwrap();
        assert_eq!(blocks[first]["opcode"], "data_setvariableto");
        let second = blocks[first]["next"].as_str().unwrap();
        assert_eq!(blocks[second]["opcode"], "data_setvariableto");
        assert!(compile_source(
            "var score: int = 0\nevent greenFlag() main {}\nevent greenFlag() secondary {}"
        )
        .unwrap_err()
        .contains("multiple greenFlag handlers"));
    }

    #[test]
    fn validates_event_types_values_and_handler_names() {
        assert!(compile_source("event unknown() main {}")
            .unwrap_err()
            .contains("unknown event type"));
        assert!(compile_source("event greenFlag(1) main {}")
            .unwrap_err()
            .contains("expects no values"));
        assert!(compile_source("event keyPressed(1) onSpace {}")
            .unwrap_err()
            .contains("value must be a string literal"));
        assert!(compile_source("event greenFlag() main {}\nevent spriteClicked() main {}")
            .unwrap_err()
            .contains("event handler \"main\" is already declared"));
        assert!(compile_source("ns Game { event greenFlag() main {} }")
            .unwrap_err()
            .contains("must be declared at module scope"));
    }

    #[test]
    fn compiles_the_documented_event_example() {
        source_project(include_str!("../../examples/events.rk"));
    }

    #[test]
    fn rejects_const_assignment_and_non_boolean_conditions() {
        assert!(
            compile_source("const score: int = 0\nscore = 1")
                .unwrap_err()
                .contains("cannot assign to const")
        );
        assert!(
            compile_source("var score: int = 0\nif score { score = 1 }")
                .unwrap_err()
                .contains("if condition must have type bool")
        );
    }

    #[test]
    fn rejects_unknown_names_and_resolves_shadowed_bindings() {
        assert!(
            compile_source("var score = missing + 1")
                .unwrap_err()
                .contains("unknown variable \"missing\"")
        );
        let (project, _) = source_project(
            "var value: int = 1\nif true { var value: int = 2\nvalue = value + 1 }\nvalue = value + 1",
        );
        let project_variables = project["targets"][0]["variables"].as_object().unwrap();
        let project_names: Vec<&str> = project_variables
            .values()
            .map(|entry| entry[0].as_str().unwrap())
            .collect();
        let target_variables = project["targets"][1]["variables"].as_object().unwrap();
        let target_names: Vec<&str> = target_variables
            .values()
            .map(|entry| entry[0].as_str().unwrap())
            .collect();
        assert!(project_names.contains(&"value"));
        assert!(target_names.contains(&"value_2"));
    }

    #[test]
    fn namespaces_imports_and_scratch_rotation_blocks_are_lowered() {
        let (project, _) = source_project(
            "use Scratch.Motion as motion\nuse Scratch.Motion.{move, rotate}\nns Gameplay { fn turn_and_move(steps: int) { move(steps)\nrotate(15)\nmotion.rotateCW(30)\nScratch.Motion.rotateCCW(45) } }\nGameplay.turn_and_move(10)",
        );
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let opcodes: Vec<&str> = blocks
            .values()
            .map(|block| block["opcode"].as_str().unwrap())
            .collect();
        assert!(opcodes.contains(&"motion_movesteps"));
        assert_eq!(
            opcodes
                .iter()
                .filter(|opcode| **opcode == "motion_turnright")
                .count(),
            2
        );
        assert!(opcodes.contains(&"motion_turnleft"));
        assert!(opcodes.contains(&"procedures_definition"));
        assert!(opcodes.contains(&"procedures_call"));

        let (aliased_project, _) =
            source_project("use Scratch.Motion.rotateCW as clockwise\nclockwise(90)");
        let aliased_blocks = aliased_project["targets"][1]["blocks"].as_object().unwrap();
        assert!(
            aliased_blocks
                .values()
                .any(|block| block["opcode"] == "motion_turnright")
        );

        let (group_alias_project, _) =
            source_project("use Scratch.Motion as motion.{rotate}\nmotion.rotate(45)");
        let group_alias_blocks = group_alias_project["targets"][1]["blocks"]
            .as_object()
            .unwrap();
        assert!(
            group_alias_blocks
                .values()
                .any(|block| block["opcode"] == "motion_turnright")
        );
    }

    #[test]
    fn scratch_lists_generate_native_list_metadata_and_blocks() {
        let source = format!(
            "{}\ninventory.clear()",
            include_str!("../../examples/scratch_lists.rk")
        );
        let (project, _) = source_project(&source);
        let lists = project["targets"][0]["lists"].as_object().unwrap();
        assert_eq!(lists.len(), 1);
        let (list_id, list_data) = lists.iter().next().unwrap();
        assert_eq!(list_data[0], "inventory");
        assert_eq!(list_data[1], json!([]));
        assert_eq!(list_data.as_array().unwrap().len(), 2);

        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let mut opcodes = Vec::new();
        for block in blocks.values() {
            let opcode = block["opcode"].as_str().unwrap();
            if matches!(
                opcode,
                "data_addtolist"
                    | "data_insertatlist"
                    | "data_replaceitemoflist"
                    | "data_itemoflist"
                    | "data_lengthoflist"
                    | "data_listcontainsitem"
                    | "data_itemnumoflist"
                    | "data_deleteoflist"
                    | "data_deletealloflist"
                    | "data_showlist"
                    | "data_hidelist"
            ) {
                opcodes.push(opcode);
                assert_eq!(block["fields"]["LIST"][0], "inventory");
                assert_eq!(block["fields"]["LIST"][1], *list_id);
            }
        }
        for opcode in [
            "data_addtolist",
            "data_insertatlist",
            "data_replaceitemoflist",
            "data_itemoflist",
            "data_lengthoflist",
            "data_listcontainsitem",
            "data_itemnumoflist",
            "data_deleteoflist",
            "data_deletealloflist",
            "data_showlist",
            "data_hidelist",
        ] {
            assert!(opcodes.contains(&opcode), "missing opcode {opcode}");
        }
    }

    #[test]
    fn scratch_list_handles_are_static_and_generics_value_is_a_scratch_type() {
        let source = "use Scratch.Generics.List as SList\nuse Scratch.Generics.Value as SValue\nvar items: SList = SList.new()\nfn first() -> SValue { return items.get(1) }\nvar value: SValue = first()";
        let (project, _) = source_project(source);
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        assert!(
            blocks
                .values()
                .any(|block| block["opcode"] == "data_itemoflist")
        );
        assert!(
            blocks
                .values()
                .any(|block| block["opcode"] == "procedures_definition")
        );

        assert!(
            compile_source(
                "use Scratch.Generics.List as SList\nfn local() { var items: SList = SList.new() }"
            )
            .unwrap_err()
            .contains("module-scope")
        );
        assert!(
            compile_source("use Scratch.Generics.List as SList\nvar items: SList = SList.new()\nvar alias: SList = items")
                .unwrap_err()
                .contains("can only be created with Scratch.Generics.List.new()")
        );
        assert!(
            compile_source("use Scratch.Generics.List as SList\nfn takes(items: SList) {}")
                .unwrap_err()
                .contains("cannot be function parameters")
        );
        assert!(compile_source("scratch list items").is_err());
    }

    #[test]
    fn scratch_list_namespace_imports_validate_and_lower() {
        let (project, _) = source_project(
            "use Scratch.Generics.List as lists\nuse Scratch.Generics.Value as Value\nvar values: lists = lists.new()\nvar item: Value = values.get(1)\nvalues.set(1, item)",
        );
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let opcodes: Vec<&str> = blocks
            .values()
            .map(|block| block["opcode"].as_str().unwrap())
            .collect();
        assert!(opcodes.contains(&"data_itemoflist"));
        assert!(opcodes.contains(&"data_replaceitemoflist"));
    }

    #[test]
    fn class_methods_private_members_and_self_returning_new_compile() {
        let source = r#"
class Counter {
    public var value: int = 0
    private var secret: int = 1

    fn new() {
        self.secret = 2
        return self
    }

    public fn increase(amount: int) {
        self.value += amount
        self.hidden()
    }

    public fn getValue() -> int {
        return self.value
    }

    private warp fn hidden() {
        self.secret += 1
    }
}

var counter: Counter = Counter.new()
counter.increase(3)
var result: int = counter.getValue()
"#;
        let (project, _) = source_project(source);
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let definitions = blocks
            .values()
            .filter(|block| block["opcode"] == "procedures_definition")
            .collect::<Vec<_>>();
        assert_eq!(definitions.len(), 4);
        let proccodes = definitions
            .iter()
            .map(|definition| {
                let prototype = definition["inputs"]["custom_block"][1]
                    .as_str()
                    .unwrap();
                blocks[prototype]["mutation"]["proccode"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(proccodes.len(), 4);
        assert!(
            blocks
                .values()
                .any(|block| block["opcode"] == "procedures_call")
        );
    }

    #[test]
    fn private_class_members_are_rejected_outside_the_class() {
        let source = "class Secret { private var value: int = 0 private fn hide() {} }\nvar secret = Secret()\nsecret.value = 1";
        assert!(compile_source(source).unwrap_err().contains("field \"value\" is private"));

        let source = "class Secret { private var value: int = 0 }\nvar secret = Secret()\nvar outside = secret.value";
        assert!(compile_source(source).unwrap_err().contains("field \"value\" is private"));

        let source = "class Secret { private fn hide() {} }\nvar secret = Secret()\nsecret.hide()";
        assert!(compile_source(source).unwrap_err().contains("method \"hide\" is private"));
    }

    #[test]
    fn class_instance_annotations_resolve_and_methods_are_specialized_per_instance() {
        let source = "class Value { var count: int = 0 fn new() { return self } fn add() { self.count += 1 } }\nvar first: Value = Value.new()\nvar second: Value = Value.new()\nfirst.add()\nsecond.add()";
        let (project, _) = source_project(source);
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let proccodes = blocks
            .values()
            .filter(|block| block["opcode"] == "procedures_definition")
            .map(|definition| {
                let prototype = definition["inputs"]["custom_block"][1]
                    .as_str()
                    .unwrap();
                blocks[prototype]["mutation"]["proccode"]
                    .as_str()
                    .unwrap()
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(proccodes.len(), 4);
    }

    #[test]
    fn generated_field_names_avoid_later_list_resource_names() {
        let source = "use Scratch.Generics.List as SList\nclass Entry { var score: int = 0 fn new() { return self } }\nvar entry: Entry = Entry.new()\nvar entry_score: SList = SList.new()";
        let (project, _) = source_project(source);
        let target = &project["targets"][0];
        assert!(target["lists"].as_object().unwrap().values().any(|list| list[0] == "entry_score"));
        assert!(target["variables"].as_object().unwrap().values().any(|variable| variable[0] == "entry_score_2"));
    }

    #[test]
    fn specialized_method_custom_block_names_avoid_user_function_names() {
        let source = "fn __rake_method_0_0_0_1() {}\nclass Entry { fn new() { return self } fn work() {} }\nvar entry: Entry = Entry.new()";
        let (project, _) = source_project(source);
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        let proccodes = blocks
            .values()
            .filter(|block| block["opcode"] == "procedures_definition")
            .map(|definition| {
                let prototype = definition["inputs"]["custom_block"][1]
                    .as_str()
                    .unwrap();
                blocks[prototype]["mutation"]["proccode"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect::<std::collections::HashSet<_>>();
        assert_eq!(proccodes.len(), 3);
        assert!(proccodes.contains("__rake_method_0_0_0_1"));
        assert!(proccodes.contains("__rake_method_0_0_0_2"));
    }

    #[test]
    fn namespaced_classes_support_typed_instances_and_methods() {
        let source = "ns Model { class Counter { fn new() { return self } fn increment() {} } }\nvar counter: Model.Counter = Model.Counter.new()\ncounter.increment()";
        let (project, _) = source_project(source);
        let blocks = project["targets"][1]["blocks"].as_object().unwrap();
        assert_eq!(
            blocks
                .values()
                .filter(|block| block["opcode"] == "procedures_definition")
                .count(),
            2
        );
    }

    #[test]
    fn renders_source_spanned_errors_with_ariadne() {
        let source = "// 東京\nvar score: int = 0\nif score { score = 1 }";
        let error = compile_source(source).unwrap_err();
        let rendered = render_diagnostic("example.src", source, &error);
        assert!(rendered.contains("example.src"));
        assert!(rendered.contains("example.src:3:4"));
        assert!(rendered.contains("if condition must have type bool"));
    }

    #[test]
    fn writes_project_json_and_assets_into_sb3_archive() {
        let (project, assets) = source_project("var value: int = 3");
        let path = std::env::temp_dir().join(format!("rake-test-{}.sb3", std::process::id()));
        write_sb3(&path, &project, &assets).expect("write archive");
        let file = File::open(&path).expect("open archive");
        let mut archive = ZipArchive::new(file).expect("read zip");
        let mut text = String::new();
        archive
            .by_name("project.json")
            .unwrap()
            .read_to_string(&mut text)
            .unwrap();
        let saved: Value = serde_json::from_str(&text).unwrap();
        assert_eq!(saved["meta"]["agent"], "Rake compiler");
        assert_eq!(archive.len(), assets.len() + 1);
        drop(archive);
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn emits_typed_lowered_ir_and_raw_project_json() {
        let compilation = compile_source_with_ir("var value: int = 3\nvalue += 1")
            .expect("compile source with IR");
        assert!(
            compilation
                .ir
                .starts_with("Rake IR (typed syntax tree after function-call lowering)")
        );
        assert!(compilation.ir.contains("VarDecl"));
        assert!(compilation.ir.contains("Assign"));

        let path = std::env::temp_dir().join(format!("rake-json-test-{}.json", std::process::id()));
        write_project_json(&path, &compilation.project).expect("write project JSON");
        let text = fs::read_to_string(&path).expect("read project JSON");
        let saved: Value = serde_json::from_str(&text).expect("parse project JSON");
        assert_eq!(saved, compilation.project);
        assert!(
            text.contains("\n  \"targets\""),
            "JSON should be pretty-printed"
        );
        fs::remove_file(path).unwrap();
    }
}
