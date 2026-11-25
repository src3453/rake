use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::ExitCode;

fn usage() {
    eprintln!("Usage: rake <source.src> -o <output.sb3>");
}

fn run() -> ExitCode {
    let mut args = env::args().skip(1);
    let source_path = match args.next() {
        Some(value) if value == "--help" || value == "-h" => {
            usage();
            return ExitCode::SUCCESS;
        }
        Some(value) => PathBuf::from(value),
        None => {
            usage();
            eprintln!("error: missing source file");
            return ExitCode::FAILURE;
        }
    };
    if source_path
        .extension()
        .and_then(|extension| extension.to_str())
        != Some("rk")
    {
        eprintln!("error: Rake source files must use the .rk extension");
        return ExitCode::FAILURE;
    }
    let flag = match args.next() {
        Some(flag) => flag,
        None => {
            eprintln!("error: missing -o <output.sb3>");
            return ExitCode::FAILURE;
        }
    };
    if flag != "-o" && flag != "--output" {
        eprintln!("error: expected -o or --output, got {flag:?}");
        return ExitCode::FAILURE;
    }
    let output_path = match args.next() {
        Some(path) => PathBuf::from(path),
        None => {
            eprintln!("error: missing output path");
            return ExitCode::FAILURE;
        }
    };
    if args.next().is_some() {
        eprintln!("error: unexpected extra arguments");
        return ExitCode::FAILURE;
    }

    let source = match fs::read_to_string(&source_path) {
        Ok(source) => source,
        Err(error) => {
            eprintln!("error: cannot read {}: {error}", source_path.display());
            return ExitCode::FAILURE;
        }
    };
    let (project, assets, block_count) = match rake::compile_source(&source) {
        Ok(compiled) => compiled,
        Err(error) => {
            let file_name = source_path.to_string_lossy();
            eprint!("{}", rake::render_diagnostic(&file_name, &source, &error));
            return ExitCode::FAILURE;
        }
    };
    if let Err(error) = rake::write_sb3(&output_path, &project, &assets) {
        eprintln!("error: {error}");
        return ExitCode::FAILURE;
    }
    println!(
        "wrote {} ({} Scratch blocks)",
        output_path.display(),
        block_count
    );
    ExitCode::SUCCESS
}

fn main() -> ExitCode {
    run()
}
