#[cfg(test)]
mod module_tests {
    use super::*;

    /// A project directory with `rake-ide.json` and the given files.
    fn project(name: &str, files: &[(&str, &str)]) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("rake-modules-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(&directory).unwrap();
        fs::write(directory.join(PROJECT_MANIFEST), "{\"name\": \"test\"}").unwrap();
        for (path, contents) in files {
            let path = directory.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, contents).unwrap();
        }
        directory
    }

    const MATH: &str = "ns MathUtil {\n    fn clamp(value: int, low: int, high: int) -> int {\n        if value < low { return low }\n        if value > high { return high }\n        return value\n    }\n    fn unused(value: int) -> int { return value }\n    enum Dir { Left, Right }\n}\n";

    fn compile(directory: &Path, file: &str) -> Result<Value, SourceError> {
        compile_file_for_runtime(&directory.join(file), Runtime::Scratch).map(|(project, _, _)| project)
    }

    fn proccodes(project: &Value, target: usize) -> Vec<String> {
        let mut codes = project["targets"][target]["blocks"]
            .as_object()
            .unwrap()
            .values()
            .filter(|block| block["opcode"] == json!("procedures_prototype"))
            .map(|block| block["mutation"]["proccode"].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        codes.sort();
        codes
    }

    fn variable_names(project: &Value, target: usize) -> Vec<String> {
        let mut names = project["targets"][target]["variables"]
            .as_object()
            .unwrap()
            .values()
            .map(|entry| entry[0].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        names.sort();
        names
    }

    fn error_text(error: SourceError) -> String {
        format!("{} @ {}", error.message, error.file.map(|file| file.display().to_string()).unwrap_or_default())
    }

    #[test]
    fn normalizes_root_relative_library_paths() {
        assert_eq!(normalize_module_path("lib/math").unwrap(), "lib/math.rk");
        assert_eq!(normalize_module_path("./lib/../lib/math.rk").unwrap(), "lib/math.rk");
        for invalid in ["", "/lib/math", "C:/lib", "lib\\math", "lib//math", "../outside", "lib/.."] {
            assert!(normalize_module_path(invalid).is_err(), "{invalid:?} should be rejected");
        }
    }

    #[test]
    fn finds_the_project_root_from_the_manifest() {
        let directory = project("root", &[("src/deep/main.rk", "")]);
        assert_eq!(find_project_root(&directory.join("src/deep/main.rk")), directory);
        let loose = std::env::temp_dir().join(format!("rake-modules-loose-{}", std::process::id()));
        fs::create_dir_all(&loose).unwrap();
        assert_eq!(find_project_root(&loose.join("main.rk")), loose);
    }

    #[test]
    fn imports_library_members_and_emits_only_used_functions() {
        let directory = project(
            "basic",
            &[
                ("lib/math.rk", MATH),
                (
                    "src/main.rk",
                    "from \"lib/math\" use MathUtil.clamp\nfrom \"lib/math\" use MathUtil as M\nvar a: int = clamp(42, 0, 10)\nvar b: int = M.clamp(-1, 0, 10)\nvar d: M.Dir = M.Dir.Right\n",
                ),
            ],
        );
        let project = compile(&directory, "src/main.rk").unwrap();
        assert_eq!(proccodes(&project, 1), vec!["MathUtil.clamp %n %n %n"]);
        let variables = variable_names(&project, 1);
        assert!(variables.contains(&"__rake_lib_return_MathUtil.clamp".to_string()));
        assert!(!variables.iter().any(|name| name.contains("unused")), "{variables:?}");
    }

    #[test]
    fn glob_imports_namespace_members_and_library_top_level() {
        let directory = project(
            "glob",
            &[
                ("lib/math.rk", MATH),
                ("src/members.rk", "from \"lib/math\" use MathUtil.*\nuse Scratch.Events.*\nvar d: Dir = Dir.Left\nevent greenFlag { Scratch.Looks.say(clamp(3, 1, 2)) }\n"),
                ("src/top.rk", "from \"lib/math\" use *\nvar a: int = MathUtil.clamp(3, 1, 2)\n"),
                ("src/both.rk", "from \"lib/math\" use MathUtil.*\nfrom \"lib/math\" use MathUtil.clamp\nvar a: int = clamp(3, 1, 2)\n"),
                ("src/bare.rk", "use *\n"),
            ],
        );
        compile(&directory, "src/members.rk").map_err(error_text).unwrap();
        compile(&directory, "src/top.rk").map_err(error_text).unwrap();
        compile(&directory, "src/both.rk").map_err(error_text).unwrap();
        let error = compile(&directory, "src/bare.rk").unwrap_err();
        assert!(error.message.contains("`use *` needs a library"), "{}", error.message);
    }

    #[test]
    fn only_direct_imports_are_visible() {
        let directory = project(
            "visibility",
            &[
                ("lib/math.rk", MATH),
                ("lib/geo.rk", "from \"lib/math\" use MathUtil.clamp\nns Geo { fn limit(v: int) -> int { return clamp(v, 0, 10) } }\n"),
                ("src/ok.rk", "from \"lib/geo\" use Geo.limit\nvar a: int = limit(42)\n"),
                ("src/transitive.rk", "from \"lib/geo\" use Geo.limit\nvar a: int = MathUtil.clamp(1, 2, 3)\n"),
                ("src/use_without_from.rk", "from \"lib/geo\" use Geo\nuse MathUtil.clamp\n"),
                ("src/wrong_library.rk", "from \"lib/geo\" use MathUtil.clamp\n"),
                ("src/type.rk", "from \"lib/geo\" use Geo\nvar d: MathUtil.Dir = MathUtil.Dir.Left\n"),
            ],
        );
        compile(&directory, "src/ok.rk").map_err(error_text).unwrap();
        let error = compile(&directory, "src/transitive.rk").unwrap_err();
        assert!(error.message.starts_with("2:"), "{}", error.message);
        assert!(error.message.contains("which this file does not import"), "{}", error.message);
        let error = compile(&directory, "src/use_without_from.rk").unwrap_err();
        assert!(error.message.contains("does not import"), "{}", error.message);
        let error = compile(&directory, "src/wrong_library.rk").unwrap_err();
        assert!(error.message.contains("does not declare \"MathUtil.clamp\""), "{}", error.message);
        let error = compile(&directory, "src/type.rk").unwrap_err();
        assert!(error.message.contains("unknown type \"MathUtil.Dir\""), "{}", error.message);
    }

    #[test]
    fn reports_errors_in_the_library_file() {
        let directory = project(
            "library-error",
            &[
                ("lib/bad.rk", "ns Bad {\n    fn f() -> int { return true }\n}\n"),
                ("lib/script.rk", "event Scratch.Events.greenFlag { }\n"),
                ("src/main.rk", "from \"lib/bad\" use Bad.f\nvar a: int = f()\n"),
                ("src/script.rk", "from \"lib/script\" use *\n"),
                ("src/missing.rk", "\nfrom \"lib/missing\" use *\n"),
                ("src/case.rk", "from \"Lib/bad\" use *\n"),
            ],
        );
        let error = compile(&directory, "src/main.rk").unwrap_err();
        assert_eq!(error.file.as_deref(), Some(directory.join("lib/bad.rk").as_path()));
        assert!(error.message.starts_with("2:"), "{}", error.message);
        let error = compile(&directory, "src/script.rk").unwrap_err();
        assert_eq!(error.file.as_deref(), Some(directory.join("lib/script.rk").as_path()));
        assert!(error.message.contains("library modules may contain only"), "{}", error.message);
        let error = compile(&directory, "src/missing.rk").unwrap_err();
        assert_eq!(error.file.as_deref(), Some(directory.join("src/missing.rk").as_path()));
        assert!(error.message.starts_with("2:6:") && error.message.contains("cannot load library"), "{}", error.message);
        let error = compile(&directory, "src/case.rk").unwrap_err();
        assert!(error.message.contains("letter case"), "{}", error.message);
    }

    #[test]
    fn rejects_circular_imports() {
        let directory = project(
            "cycle",
            &[
                ("lib/a.rk", "from \"lib/b\" use *\nns A { }\n"),
                ("lib/b.rk", "from \"lib/a\" use *\nns B { }\n"),
                ("src/main.rk", "from \"lib/a\" use *\n"),
            ],
        );
        let error = compile(&directory, "src/main.rk").unwrap_err();
        assert!(error.message.contains("circular library import: lib/a.rk -> lib/b.rk -> lib/a.rk"), "{}", error.message);
    }

    #[test]
    fn from_requires_a_file_location_and_stays_a_contextual_keyword() {
        let error = compile_source("from \"lib/math\" use *").unwrap_err();
        assert!(error.contains("need the source file's location"), "{error}");
        let error = compile_source("ns Game { from \"lib/math\" use * }").unwrap_err();
        assert!(error.contains("top level"), "{error}");
        // `from` is still an ordinary identifier elsewhere.
        compile_source("fn twice(from: int) -> int { return from * 2 }\nvar x: int = twice(3)").unwrap();
    }

    #[test]
    fn scene_targets_share_one_copy_of_library_blocks() {
        let directory = project(
            "scene",
            &[
                ("lib/math.rk", MATH),
                ("src/a.rk", "from \"lib/math\" use MathUtil.clamp\nevent Scratch.Events.greenFlag { Scratch.Looks.say(clamp(5, 0, 3)) }\n"),
                ("src/b.rk", "from \"lib/math\" use *\nevent Scratch.Events.greenFlag { Scratch.Looks.say(MathUtil.clamp(-5, 0, 3)) }\n"),
                ("scenes/main.rks", "scene Main {\n    stage { }\n    sprite Player { scripts: [\"../src/a.rk\", \"../src/b.rk\"] }\n}\n"),
            ],
        );
        let compilation = compile_scene(&directory.join("scenes/main.rks")).map_err(|error| error.message).unwrap();
        assert_eq!(proccodes(&compilation.project, 1), vec!["MathUtil.clamp %n %n %n"]);
        let variables = variable_names(&compilation.project, 1);
        assert_eq!(
            variables.iter().filter(|name| name.starts_with("__rake_lib_return_MathUtil.clamp")).count(),
            1,
            "{variables:?}"
        );
        // Both callers target the single remaining definition's argument IDs.
        let blocks = compilation.project["targets"][1]["blocks"].as_object().unwrap();
        let prototype = blocks.values().find(|block| block["opcode"] == json!("procedures_prototype")).unwrap();
        let argument_ids = prototype["mutation"]["argumentids"].as_str().unwrap();
        let calls = blocks.values().filter(|block| block["opcode"] == json!("procedures_call")).collect::<Vec<_>>();
        assert_eq!(calls.len(), 2);
        for call in calls {
            assert_eq!(call["mutation"]["argumentids"].as_str().unwrap(), argument_ids);
        }
    }

    #[test]
    fn analysis_keeps_working_with_library_imports() {
        let directory = project(
            "analysis",
            &[("lib/math.rk", MATH), ("src/main.rk", "")],
        );
        let source = "from \"lib/math\" use MathUtil.clamp\nvar total: int = clamp(1, 2, 3)\n";
        let entry = directory.join("src/main.rk");
        let context = ModuleContext { entry_path: &entry, root: &directory, sources: &FileSystemSources };
        let occurrences = analyze_symbols_in_project(source, Some(&context)).unwrap();
        assert!(occurrences.iter().any(|occurrence| occurrence.name == "total" && occurrence.declaration));
        assert!(occurrences.iter().all(|occurrence| occurrence.line <= 1));
        // Without a location, imports stay unresolved instead of failing analysis.
        assert!(analyze_symbols(source).is_ok());
    }
}
