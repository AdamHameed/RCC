mod ast;
mod codegen;
mod lexer;
mod parser;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{self, Command};

fn main() {
    if let Err(error) = run() {
        eprintln!("error: {error}");
        process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let mut args = env::args().skip(1);
    let mut input_path: Option<String> = None;
    let mut output_path: Option<String> = None;

    while let Some(arg) = args.next() {
        if arg == "-o" {
            output_path = Some(
                args.next()
                    .ok_or_else(|| "-o option requires an output file path".to_string())?,
            );
        } else if arg.starts_with('-') {
            return Err(format!("unknown option: {arg}"));
        } else {
            if input_path.is_some() {
                return Err("multiple input files are not supported".to_string());
            }
            input_path = Some(arg);
        }
    }

    let input_path =
        input_path.ok_or_else(|| "usage: compiler <input.c> [-o <output>]".to_string())?;

    if !input_path.ends_with(".c") {
        return Err(format!("expected a .c input file, received: {input_path}"));
    }

    log_step(&format!("reading source from {input_path}"));
    let source = fs::read_to_string(&input_path)
        .map_err(|err| format!("failed to read {input_path}: {err}"))?;

    log_step("lexing source");
    let tokens = lexer::tokenize(&source)?;
    log_step("parsing tokens");
    let program = parser::parse(&tokens).map_err(|err| err.to_string())?;

    // Validate that the defined function is named "main"
    if program.function.name != "main" {
        return Err(format!(
            "expected function name to be 'main', found '{}' (rcc only supports compiling programs with a 'main' entrypoint)",
            program.function.name
        ));
    }

    log_step("generating LLVM IR");
    let ir = codegen::generate_ir(&program)?;

    // Determine output paths dynamically
    let executable_path = PathBuf::from(output_path.unwrap_or_else(|| "output".to_string()));
    let ir_path = if let Some(ext) = executable_path.extension() {
        executable_path.with_extension(format!("{}.ll", ext.to_string_lossy()))
    } else {
        executable_path.with_extension("ll")
    };

    log_step(&format!("writing LLVM IR to {}", ir_path.display()));
    fs::write(&ir_path, ir)
        .map_err(|err| format!("failed to write {}: {err}", ir_path.display()))?;

    let compiler = find_system_compiler()?;
    log_step(&format!(
        "compiling LLVM IR with {} -> {}",
        compiler,
        executable_path.display()
    ));
    compile_ir(compiler, &ir_path, &executable_path)?;

    log_step(&format!("build complete: {}", executable_path.display()));

    Ok(())
}

fn find_system_compiler() -> Result<&'static str, String> {
    for compiler in ["clang", "gcc"] {
        if let Ok(output) = Command::new(compiler).arg("--version").output() {
            if output.status.success() {
                return Ok(compiler);
            }
        }
    }

    Err("could not find `clang` or `gcc` in PATH".to_string())
}

fn compile_ir(compiler: &str, ir_path: &Path, executable_path: &Path) -> Result<(), String> {
    let mut command = Command::new(compiler);
    command
        .arg("-x")
        .arg("ir")
        .arg(ir_path)
        .arg("-o")
        .arg(executable_path);

    let output = command
        .output()
        .map_err(|err| format!("failed to invoke {compiler}: {err}"))?;

    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);

    Err(format!(
        "{compiler} failed with status {}.\nstdout:\n{}\nstderr:\n{}",
        output.status,
        stdout.trim(),
        stderr.trim()
    ))
}

fn log_step(message: &str) {
    eprintln!("[rcc] {message}");
}
