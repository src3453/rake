use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

#[derive(Debug, PartialEq, Eq)]
struct CliOptions {
    source_path: PathBuf,
    sb3_path: Option<PathBuf>,
    ir_path: Option<PathBuf>,
    json_path: Option<PathBuf>,
    json_errors: bool,
    print_scene: bool,
    runtime: Option<rake::Runtime>,
}

fn usage() {
    eprintln!(
        "Usage: rake <source.rk|scene.rks> [-o <output.sb3>] [--runtime <scratch|turbowarp>] [--emit-ir <output.ir>] [--emit-json <output.json>] [--json-errors] [--print-scene]"
    );
    eprintln!(
        "A .rks scene compiles the Stage, its Sprites, their assets, and every referenced .rk script into one project."
    );
    eprintln!(
        "Specify at least one output option. IR and JSON can be emitted without creating an SB3 archive."
    );
    eprintln!("       rake decompile <project.sb3> -o <output-directory> [--force]");
    eprintln!(
        "Experimental: turn an existing .sb3 into a Rake project (scenes/main.rks, src/*.rk, assets/)."
    );
}

#[derive(Debug, PartialEq, Eq)]
struct DecompileOptions {
    input: PathBuf,
    output: PathBuf,
    force: bool,
}

fn parse_decompile_args<I>(args: I) -> Result<DecompileOptions, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let mut input = None;
    let mut output = None;
    let mut force = false;
    while let Some(argument) = args.next() {
        match argument.as_str() {
            "-o" | "--output" => {
                let path = take_path(&mut args, &argument)?;
                set_once(&mut output, path, &argument)?;
            }
            "--force" => force = true,
            _ if input.is_none() && !argument.starts_with('-') => {
                input = Some(PathBuf::from(argument))
            }
            _ => return Err(format!("unexpected argument {argument:?}")),
        }
    }
    Ok(DecompileOptions {
        input: input.ok_or("missing .sb3 file to decompile")?,
        output: output.ok_or("missing output directory (-o <directory>)")?,
        force,
    })
}

fn run_decompile<I>(args: I) -> ExitCode
where
    I: IntoIterator<Item = String>,
{
    let options = match parse_decompile_args(args) {
        Ok(options) => options,
        Err(error) => {
            usage();
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    let non_empty = fs::read_dir(&options.output)
        .map(|mut entries| entries.next().is_some())
        .unwrap_or(false);
    if non_empty && !options.force {
        eprintln!(
            "error: {} is not empty; choose another directory or pass --force to overwrite files in it",
            options.output.display()
        );
        return ExitCode::FAILURE;
    }
    let bytes = match fs::read(&options.input) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", options.input.display());
            return ExitCode::FAILURE;
        }
    };
    let decompilation = match rake::decompile_sb3_bytes(&bytes) {
        Ok(decompilation) => decompilation,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = rake::write_decompilation(&options.output, &decompilation) {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    for warning in &decompilation.warnings {
        eprintln!("warning: {warning}");
    }
    println!(
        "wrote {} ({} files, {} warnings); compile it with: rake {} -o <output.sb3>",
        options.output.display(),
        decompilation.files.len(),
        decompilation.warnings.len(),
        options.output.join(&decompilation.scene_path).display()
    );
    ExitCode::SUCCESS
}

fn take_path<I>(args: &mut I, option: &str) -> Result<PathBuf, String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing path after {option}"))
}

fn set_once<T>(slot: &mut Option<T>, value: T, option: &str) -> Result<(), String> {
    if slot.is_some() {
        return Err(format!("{option} may only be specified once"));
    }
    *slot = Some(value);
    Ok(())
}

fn parse_args<I>(args: I) -> Result<CliOptions, String>
where
    I: IntoIterator<Item = String>,
{
    let mut args = args.into_iter();
    let source_path = args
        .next()
        .map(PathBuf::from)
        .ok_or_else(|| "missing source file".to_string())?;
    let mut options = CliOptions {
        source_path,
        sb3_path: None,
        ir_path: None,
        json_path: None,
        json_errors: false,
        print_scene: false,
        runtime: None,
    };

    while let Some(option) = args.next() {
        match option.as_str() {
            "-o" | "--output" => {
                let path = take_path(&mut args, &option)?;
                set_once(&mut options.sb3_path, path, &option)?;
            }
            "--emit-ir" | "--dump-ir" => {
                let path = take_path(&mut args, &option)?;
                set_once(&mut options.ir_path, path, &option)?;
            }
            "--emit-json" | "--dump-json" => {
                let path = take_path(&mut args, &option)?;
                set_once(&mut options.json_path, path, &option)?;
            }
            "--runtime" => {
                let value = args.next().ok_or("missing runtime after --runtime")?;
                let runtime = rake::Runtime::parse(&value).ok_or_else(|| {
                    format!("unknown runtime {value:?}; expected \"scratch\" or \"turbowarp\"")
                })?;
                set_once(&mut options.runtime, runtime, &option)?;
            }
            "--json-errors" => options.json_errors = true,
            "--print-scene" => options.print_scene = true,
            _ => return Err(format!("unexpected argument {option:?}")),
        }
    }

    if options.sb3_path.is_none()
        && options.ir_path.is_none()
        && options.json_path.is_none()
        && !options.print_scene
    {
        return Err(
            "missing output option (-o, --emit-ir, --emit-json, or --print-scene)".to_string(),
        );
    }
    Ok(options)
}

fn write_text(path: &std::path::Path, contents: &str) -> Result<(), String> {
    if let Some(parent) = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)
            .map_err(|error| format!("cannot create {}: {error}", parent.display()))?;
    }
    fs::write(path, contents).map_err(|error| format!("cannot write {}: {error}", path.display()))
}

/// Print a compile error either as an Ariadne report or as one JSON object for tools.
fn report_error(file: &std::path::Path, message: &str, json_errors: bool) {
    if json_errors {
        let mut parts = message.splitn(3, ':');
        let line = parts.next().and_then(|part| part.parse::<usize>().ok());
        let column = parts.next().and_then(|part| part.parse::<usize>().ok());
        let (line, column, detail) = match (line, column) {
            (Some(line), Some(column)) => {
                (line, column, parts.next().unwrap_or("").trim().to_string())
            }
            _ => (1, 1, message.to_string()),
        };
        eprintln!(
            "{}",
            serde_json::json!({"file": file.to_string_lossy(), "line": line, "column": column, "message": detail})
        );
        return;
    }
    let source = fs::read_to_string(file).unwrap_or_default();
    eprint!(
        "{}",
        rake::render_diagnostic(&file.to_string_lossy(), &source, message)
    );
}

fn run() -> ExitCode {
    let mut raw_args = env::args().skip(1);
    let Some(first_arg) = raw_args.next() else {
        usage();
        eprintln!("error: missing source file");
        return ExitCode::FAILURE;
    };
    if first_arg == "--help" || first_arg == "-h" {
        usage();
        return ExitCode::SUCCESS;
    }
    if first_arg == "decompile" {
        return run_decompile(raw_args);
    }
    let args = std::iter::once(first_arg).chain(raw_args);
    let options = match parse_args(args) {
        Ok(options) => options,
        Err(error) => {
            usage();
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };

    let extension = options
        .source_path
        .extension()
        .and_then(|extension| extension.to_str())
        .unwrap_or_default()
        .to_string();
    if extension != "rk" && extension != "rks" {
        eprintln!("error: Rake sources must use the .rk extension and scenes the .rks extension");
        return ExitCode::FAILURE;
    }
    let is_scene = extension == "rks";
    if is_scene && options.ir_path.is_some() {
        eprintln!("error: --emit-ir is only available for single .rk sources");
        return ExitCode::FAILURE;
    }

    if options.print_scene {
        if !is_scene {
            eprintln!("error: --print-scene requires a .rks scene file");
            return ExitCode::FAILURE;
        }
        let source = match fs::read_to_string(&options.source_path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!(
                    "error: cannot read {}: {error}",
                    options.source_path.display()
                );
                return ExitCode::FAILURE;
            }
        };
        return match rake::parse_scene(&source, Some(&options.source_path)) {
            Ok(scene) => {
                println!("{}", rake::scene_document_json(&scene));
                ExitCode::SUCCESS
            }
            Err(message) => {
                report_error(&options.source_path, &message, options.json_errors);
                ExitCode::FAILURE
            }
        };
    }

    let compilation = if is_scene {
        rake::compile_scene_for_runtime(&options.source_path, options.runtime)
            .map(|compilation| {
                (
                    compilation.project,
                    compilation.assets,
                    compilation.block_count,
                    None,
                )
            })
            .map_err(|error| (error.file, error.message))
    } else {
        let source = match fs::read_to_string(&options.source_path) {
            Ok(source) => source,
            Err(error) => {
                eprintln!(
                    "error: cannot read {}: {error}",
                    options.source_path.display()
                );
                return ExitCode::FAILURE;
            }
        };
        if options.ir_path.is_some() {
            rake::compile_source_with_ir_for_runtime(&source, options.runtime.unwrap_or_default())
                .map(|compilation| {
                    (
                        compilation.project,
                        compilation.assets,
                        compilation.block_count,
                        Some(compilation.ir),
                    )
                })
        } else {
            rake::compile_source_for_runtime(&source, options.runtime.unwrap_or_default())
                .map(|(project, assets, block_count)| (project, assets, block_count, None))
        }
        .map_err(|message| (options.source_path.clone(), message))
    };
    let (project, assets, block_count, ir) = match compilation {
        Ok(compiled) => compiled,
        Err((file, message)) => {
            report_error(&file, &message, options.json_errors);
            return ExitCode::FAILURE;
        }
    };

    if let Some(path) = &options.sb3_path {
        if let Err(error) = rake::write_sb3(path, &project, &assets) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
        println!("wrote {} ({} Scratch blocks)", path.display(), block_count);
    }
    if let Some(path) = &options.ir_path {
        if let Err(error) = write_text(path, ir.as_deref().unwrap_or_default()) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
        println!("wrote {} (typed, lowered IR)", path.display());
    }
    if let Some(path) = &options.json_path {
        if let Err(error) = rake::write_project_json(path, &project) {
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
        println!("wrote {} (project JSON)", path.display());
    }
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    run()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn accepts_each_output_mode_independently_or_together() {
        let json_only = parse_args(args(&["program.rk", "--emit-json", "program.json"]))
            .expect("JSON-only output should be accepted");
        assert_eq!(json_only.json_path, Some(PathBuf::from("program.json")));
        assert_eq!(json_only.sb3_path, None);
        assert_eq!(json_only.ir_path, None);

        let all_outputs = parse_args(args(&[
            "program.rk",
            "--emit-ir",
            "program.ir",
            "-o",
            "program.sb3",
            "--emit-json",
            "program.json",
        ]))
        .expect("combined outputs should be accepted");
        assert_eq!(all_outputs.ir_path, Some(PathBuf::from("program.ir")));
        assert_eq!(all_outputs.sb3_path, Some(PathBuf::from("program.sb3")));
        assert_eq!(all_outputs.json_path, Some(PathBuf::from("program.json")));
        assert!(!all_outputs.json_errors);

        let scene = parse_args(args(&["scene.rks", "-o", "scene.sb3", "--json-errors"]))
            .expect("scenes and JSON errors should be accepted");
        assert!(scene.json_errors);
        assert_eq!(scene.runtime, None);
        let runtime = parse_args(args(&[
            "program.rk",
            "--runtime",
            "turbowarp",
            "--emit-json",
            "program.json",
        ]))
        .expect("runtime profile should be accepted");
        assert_eq!(runtime.runtime, Some(rake::Runtime::TurboWarp));
        assert!(
            parse_args(args(&[
                "program.rk",
                "--runtime",
                "scratchx",
                "-o",
                "program.sb3"
            ]))
            .unwrap_err()
            .contains("unknown runtime")
        );
    }

    #[test]
    fn parses_decompile_arguments() {
        let options = parse_decompile_args(args(&["game.sb3", "-o", "out", "--force"]))
            .expect("decompile arguments should be accepted");
        assert_eq!(options.input, PathBuf::from("game.sb3"));
        assert_eq!(options.output, PathBuf::from("out"));
        assert!(options.force);
        assert!(
            parse_decompile_args(args(&["game.sb3"]))
                .unwrap_err()
                .contains("missing output directory")
        );
        assert!(
            parse_decompile_args(args(&["-o", "out"]))
                .unwrap_err()
                .contains("missing .sb3")
        );
    }

    #[test]
    fn requires_an_output_and_rejects_duplicate_output_options() {
        assert!(
            parse_args(args(&["program.rk"]))
                .unwrap_err()
                .contains("missing output option")
        );
        assert!(
            parse_args(args(&[
                "program.rk",
                "--emit-json",
                "first.json",
                "--emit-json",
                "second.json",
            ]))
            .unwrap_err()
            .contains("may only be specified once")
        );
    }
}
