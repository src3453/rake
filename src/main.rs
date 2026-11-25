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
}

fn usage() {
    eprintln!(
        "Usage: rake <source.rk> [-o <output.sb3>] [--emit-ir <output.ir>] [--emit-json <output.json>]"
    );
    eprintln!(
        "Specify at least one output option. IR and JSON can be emitted without creating an SB3 archive."
    );
}

fn take_path<I>(args: &mut I, option: &str) -> Result<PathBuf, String>
where
    I: Iterator<Item = String>,
{
    args.next()
        .map(PathBuf::from)
        .ok_or_else(|| format!("missing path after {option}"))
}

fn set_once(slot: &mut Option<PathBuf>, value: PathBuf, option: &str) -> Result<(), String> {
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
            _ => return Err(format!("unexpected argument {option:?}")),
        }
    }

    if options.sb3_path.is_none() && options.ir_path.is_none() && options.json_path.is_none() {
        return Err("missing output option (-o, --emit-ir, or --emit-json)".to_string());
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
    let args = std::iter::once(first_arg).chain(raw_args);
    let options = match parse_args(args) {
        Ok(options) => options,
        Err(error) => {
            usage();
            eprintln!("error: {error}");
            return ExitCode::FAILURE;
        }
    };

    if options
        .source_path
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("rk")
    {
        eprintln!("error: Rake source files must use the .rk extension");
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
    let compilation = if options.ir_path.is_some() {
        rake::compile_source_with_ir(&source).map(|compilation| {
            (
                compilation.project,
                compilation.assets,
                compilation.block_count,
                Some(compilation.ir),
            )
        })
    } else {
        rake::compile_source(&source)
            .map(|(project, assets, block_count)| (project, assets, block_count, None))
    };
    let (project, assets, block_count, ir) = match compilation {
        Ok(compiled) => compiled,
        Err(error) => {
            let file_name = options.source_path.to_string_lossy();
            eprint!("{}", rake::render_diagnostic(&file_name, &source, &error));
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
