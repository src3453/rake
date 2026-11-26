#[cfg(test)]
mod scene_tests {
    use super::*;

    fn temp_project(name: &str) -> PathBuf {
        let directory = std::env::temp_dir().join(format!("rake-scene-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&directory);
        fs::create_dir_all(directory.join("scenes")).unwrap();
        fs::create_dir_all(directory.join("src")).unwrap();
        fs::create_dir_all(directory.join("assets")).unwrap();
        directory
    }

    const SQUARE_SVG: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\" width=\"64\" height=\"32\"><rect width=\"64\" height=\"32\"/></svg>";

    #[test]
    fn md5_matches_known_digests() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"The quick brown fox jumps over the lazy dog"), "9e107d9d372bb6826bd81d3542a419d6");
        assert_eq!(md5_hex(DEFAULT_SPRITE_SVG), DEFAULT_SPRITE_ID);
        assert_eq!(md5_hex(DEFAULT_BACKDROP_SVG), DEFAULT_BACKDROP_ID);
    }

    #[test]
    fn parses_stage_and_sprites_with_all_properties() {
        let source = r#"
scene Game {
    runtime: "turbowarp"
    stage {
        backdrops: ["bg.svg", ("night.png", 240, 180, 2)]
        current_backdrop: "night"
        scripts: ["stage.rk"]
        tempo: 90
    }
    sprite Player {
        costume: "player.svg"
        position: (-10, 20.5)
        size: 50
        direction: -90
        visible: false
        layer: 3
        draggable: true
        rotation_style: "left-right"
        scripts: ["player.rk"]
        sounds: ["pop.wav"]
    }
    sprite Enemy {}
}"#;
        let scene = parse_scene(source, None).expect("scene should parse");
        assert_eq!(scene.name, "Game");
        assert_eq!(scene.runtime, Runtime::TurboWarp);
        assert_eq!(scene.stage.backdrops.len(), 2);
        assert_eq!(scene.stage.current_backdrop, 1);
        assert_eq!(scene.stage.backdrops[1].center, Some((240.0, 180.0)));
        assert_eq!(scene.stage.backdrops[1].resolution, Some(2));
        assert_eq!(scene.stage.tempo, 90.0);
        let player = &scene.sprites[0];
        assert_eq!((player.x, player.y, player.size, player.direction), (-10.0, 20.5, 50.0, -90.0));
        assert!(!player.visible && player.draggable);
        assert_eq!(player.layer, Some(3));
        assert_eq!(player.rotation_style, "left-right");
        assert_eq!(scene.sprites[1].name, "Enemy");
        assert_eq!(scene.referenced_paths().len(), 6);
    }

    #[test]
    fn reports_scene_errors_at_their_source_position() {
        let cases = [
            ("sprite A {}", "1:1: a scene file must start with"),
            ("scene A { sprite P {} sprite P {} }", "1:30: sprite \"P\" is already declared"),
            ("scene A {\n  sprite P { speed: 3 }\n}", "2:14: unknown property `speed` for a sprite"),
            ("scene A { sprite P { size: \"big\" } }", "1:28: `size` expects a number"),
            ("scene A { runtime: \"scratchx\" }", "`runtime` must be \"scratch\" or \"turbowarp\""),
            ("scene A { runtime: true }", "`runtime` expects \"scratch\" or \"turbowarp\""),
            ("scene A { sprite P { costume: \"a.txt\" } }", "unsupported file type"),
            ("scene A { sprite P { scripts: [\"a.js\"] } }", "must be a .rk source file"),
            ("scene A { stage {} stage {} }", "only one `stage` block"),
            ("scene A { sprite P { costumes: [\"a.svg\"], current_costume: 2 } }", "index 2 is out of range"),
            ("scene A { sprite P { costume: \"a.svg\", costumes: [\"b.svg\"] } }", "use either `costume` or `costumes`"),
            ("scene A {} scene B {}", "exactly one scene"),
            ("scene A { sprite Stage {} }", "reserved by Scratch"),
            ("scene A { stage { position: (1, 2) } }", "unknown property `position` for the stage"),
        ];
        for (source, expected) in cases {
            let error = parse_scene(source, None).expect_err(source);
            assert!(error.contains(expected), "{source}: {error}");
        }
    }

    #[test]
    fn reports_missing_referenced_files() {
        let directory = temp_project("missing");
        let scene_path = directory.join("scenes/main.rks");
        let source = "scene Main {\n  sprite Player { scripts: [\"../src/missing.rk\"] }\n}";
        let error = parse_scene(source, Some(&scene_path)).unwrap_err();
        assert!(error.starts_with("2:29:"), "{error}");
        assert!(error.contains("does not exist"), "{error}");
    }

    #[test]
    fn compiles_each_target_with_its_scripts_assets_and_shared_variables() {
        let directory = temp_project("compile");
        fs::write(directory.join("assets/player.svg"), SQUARE_SVG).unwrap();
        fs::write(
            directory.join("src/player.rk"),
            "var score: int = 0\nfn bump() -> int { return 1 }\nevent reset { score = bump() }\nevent Scratch.Events.greenFlag { Scratch.Motion.move(10) send reset }",
        )
        .unwrap();
        fs::write(
            directory.join("src/stage.rk"),
            "var score: int = 0\nfn bump() -> int { return 2 }\nevent reset { score += bump() }",
        )
        .unwrap();
        fs::write(
            directory.join("scenes/main.rks"),
            "scene Main {\n  stage { scripts: [\"../src/stage.rk\"] }\n  sprite Player {\n    costume: \"../assets/player.svg\"\n    position: (10, -20)\n    scripts: [\"../src/player.rk\"]\n  }\n  sprite Hidden { visible: false, layer: -1 }\n}",
        )
        .unwrap();

        let compilation = compile_scene(&directory.join("scenes/main.rks")).expect("scene should compile");
        let targets = compilation.project["targets"].as_array().unwrap();
        assert_eq!(targets.len(), 3);
        let stage = &targets[0];
        let player = &targets[1];
        assert_eq!(player["name"], "Player");
        assert_eq!((player["x"].clone(), player["y"].clone()), (json!(10), json!(-20)));
        assert_eq!(player["costumes"][0]["name"], "player");
        assert_eq!(player["costumes"][0]["rotationCenterX"], json!(32));
        assert_eq!(player["costumes"][0]["rotationCenterY"], json!(16));
        assert_eq!(targets[2]["costumes"][0]["assetId"], DEFAULT_SPRITE_ID);
        // The explicit layer -1 puts Hidden behind Player.
        assert_eq!(targets[2]["layerOrder"], json!(1));
        assert_eq!(player["layerOrder"], json!(2));

        // `score` is declared in both sources and becomes one shared Stage variable.
        let stage_names = stage["variables"]
            .as_object()
            .unwrap()
            .values()
            .map(|entry| entry[0].as_str().unwrap().to_string())
            .collect::<Vec<_>>();
        assert_eq!(stage_names.iter().filter(|name| *name == "score").count(), 1);
        // Both targets broadcast/receive the same message through one broadcast ID.
        assert_eq!(stage["broadcasts"].as_object().unwrap().len(), 1);
        let opcodes = |target: &Value| {
            target["blocks"]
                .as_object()
                .unwrap()
                .values()
                .map(|block| block["opcode"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        assert!(opcodes(player).contains(&"motion_movesteps".to_string()));
        assert!(opcodes(stage).contains(&"event_whenbroadcastreceived".to_string()));

        // Every block reference resolves inside the target that owns it.
        for target in targets {
            let blocks = target["blocks"].as_object().unwrap();
            for block in blocks.values() {
                for key in ["next", "parent"] {
                    if let Some(id) = block[key].as_str() {
                        assert!(blocks.contains_key(id), "dangling {key} {id}");
                    }
                }
            }
        }
        let asset_names = compilation.assets.iter().map(|(name, _)| name.clone()).collect::<Vec<_>>();
        assert!(asset_names.contains(&format!("{}.svg", md5_hex(SQUARE_SVG.as_bytes()))));
        assert!(asset_names.contains(&format!("{DEFAULT_BACKDROP_ID}.svg")));
        assert!(asset_names.contains(&format!("{DEFAULT_SPRITE_ID}.svg")));
    }

    #[test]
    fn rejects_sprite_blocks_in_stage_scripts_and_duplicate_custom_blocks() {
        let directory = temp_project("stage-errors");
        fs::write(directory.join("src/move.rk"), "Scratch.Motion.move(10)").unwrap();
        fs::write(directory.join("src/a.rk"), "fn work() {}\nwork()").unwrap();
        fs::write(directory.join("src/b.rk"), "fn work() {}\nwork()").unwrap();
        fs::write(directory.join("scenes/stage.rks"), "scene S { stage { scripts: [\"../src/move.rk\"] } }").unwrap();
        fs::write(directory.join("scenes/dup.rks"), "scene D { sprite P { scripts: [\"../src/a.rk\", \"../src/b.rk\"] } }").unwrap();

        let error = compile_scene(&directory.join("scenes/stage.rks")).err().unwrap();
        assert!(error.file.ends_with("move.rk"));
        assert!(error.message.contains("motion_movesteps"), "{}", error.message);

        let error = compile_scene(&directory.join("scenes/dup.rks")).err().unwrap();
        assert!(error.file.ends_with("b.rk"));
        assert!(error.message.contains("custom block \"work\""), "{}", error.message);
    }

    #[test]
    fn scene_runtime_controls_script_compilation_and_extension_metadata() {
        let directory = temp_project("runtime-extensions");
        fs::write(
            directory.join("src/player.rk"),
            "use Scratch.Extension.Pen as Pen\nuse Scratch.Extension.Music as Music\nPen.clear()\nMusic.playNoteForBeats(60, 1)",
        ).unwrap();
        fs::write(
            directory.join("scenes/main.rks"),
            "scene Main { runtime: \"turbowarp\" sprite Player { scripts: [\"../src/player.rk\"] } }",
        ).unwrap();

        let compilation = compile_scene(&directory.join("scenes/main.rks")).expect("TurboWarp profile compiles");
        assert_eq!(compilation.scene.runtime, Runtime::TurboWarp);
        assert_eq!(compilation.project["extensions"], json!(["music", "pen"]));
        let overlaid = compile_scene_for_runtime(&directory.join("scenes/main.rks"), Some(Runtime::Scratch))
            .expect("CLI runtime override compiles");
        assert_eq!(overlaid.scene.runtime, Runtime::Scratch);
        assert_eq!(overlaid.project["extensions"], json!(["music", "pen"]));
    }

    #[test]
    fn script_errors_point_at_the_failing_source_file() {
        let directory = temp_project("script-error");
        fs::write(directory.join("src/bad.rk"), "var x: int = true").unwrap();
        fs::write(directory.join("scenes/main.rks"), "scene M { sprite P { scripts: [\"../src/bad.rk\"] } }").unwrap();
        let error = compile_scene(&directory.join("scenes/main.rks")).err().unwrap();
        assert!(error.file.ends_with("bad.rk"));
        assert!(error.message.starts_with("1:"), "{}", error.message);
    }

    #[test]
    fn reads_bitmap_and_sound_metadata() {
        let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 13, b'I', b'H', b'D', b'R'];
        png.extend_from_slice(&120u32.to_be_bytes());
        png.extend_from_slice(&80u32.to_be_bytes());
        assert_eq!(image_size(&png, "png"), Some((120.0, 80.0)));
        assert_eq!(svg_size("<svg viewBox=\"0 0 30 40\">"), Some((30.0, 40.0)));
        assert_eq!(svg_size("<svg width=\"12px\" height='8'>"), Some((12.0, 8.0)));

        let mut wav = b"RIFF\0\0\0\0WAVEfmt ".to_vec();
        wav.extend_from_slice(&16u32.to_le_bytes());
        wav.extend_from_slice(&[1, 0, 1, 0]);
        wav.extend_from_slice(&22050u32.to_le_bytes());
        wav.extend_from_slice(&44100u32.to_le_bytes());
        wav.extend_from_slice(&[2, 0, 16, 0]);
        wav.extend_from_slice(b"data");
        wav.extend_from_slice(&200u32.to_le_bytes());
        assert_eq!(wav_info(&wav), Some((22050, 100)));
    }
}
