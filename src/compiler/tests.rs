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
