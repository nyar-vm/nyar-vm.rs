//! Legend multi-language runtime CLI.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use clap::{Parser, Subcommand};
use legacy_vm::LegacyVmRunner;
use miette::{IntoDiagnostic, Result, WrapErr, miette};

/// Legend multi-language unified runtime CLI.
#[derive(Debug, Parser)]
#[command(name = "legend", version, about = "Multi-language unified runtime CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run a script file.
    Run {
        /// Script file path.
        file: PathBuf,
        /// Language identifier (auto-detected from extension when omitted).
        #[arg(short, long)]
        language: Option<String>,
        /// Execution target (`native`, `nyar-vm`, `jvm`, `clr`, `wasm`, `wasi`).
        /// Host-script PE / specialize product path is `native` (not `nyar-vm`).
        #[arg(short, long, default_value = "native")]
        target: String,
    },
    /// Evaluate a code snippet.
    Eval {
        /// Code snippet.
        code: String,
        /// Language identifier.
        #[arg(short, long)]
        language: Option<String>,
    },
    /// Build a script file to a target artifact.
    Build {
        /// Script file path.
        file: PathBuf,
        /// Build target. Host-script PE destination is `native` (residual → PE).
        #[arg(short, long, default_value = "native")]
        target: String,
    },
    /// List supported languages and targets.
    List,
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { file, language, target: _ } => run_file(&file, language.as_deref()),
        Commands::Eval { code, language } => eval_code(&code, language.as_deref()),
        Commands::Build { file, target } => build_file(&file, &target),
        Commands::List => list_support(),
    }
}

fn run_file(file: &Path, language: Option<&str>) -> Result<()> {
    if !file.is_file() {
        return Err(miette!("file not found: {}", file.display()));
    }

    let source = fs::read_to_string(file).into_diagnostic().wrap_err_with(|| format!("failed to read file: {}", file.display()))?;
    let runner = LegacyVmRunner::new();
    let resolved = resolve_language(&runner, language, &source, Some(file))?;
    println!("语言: {resolved}");
    println!("文件: {}", file.display());
    println!("--- 输出 ---");

    let mut env = std::collections::HashMap::new();
    let result = runner.run(&resolved, &source, &mut env)?;
    println!("结果: {result}");
    Ok(())
}

fn eval_code(code: &str, language: Option<&str>) -> Result<()> {
    let runner = LegacyVmRunner::new();
    let resolved = resolve_language(&runner, language, code, None)?;
    println!("自动检测语言: {resolved}");

    let mut env = std::collections::HashMap::new();
    let result = runner.run(&resolved, code, &mut env)?;
    println!("{result}");
    Ok(())
}

fn build_file(file: &Path, target: &str) -> Result<()> {
    if !file.is_file() {
        return Err(miette!("file not found: {}", file.display()));
    }

    let source = fs::read_to_string(file).into_diagnostic().wrap_err_with(|| format!("failed to read file: {}", file.display()))?;
    let language = LegacyVmRunner::detect_language_from_file(file.to_string_lossy().as_ref(), &source).unwrap_or_else(|| "bash".to_string());
    let resolved_target = resolve_target_alias(target);

    println!("编译: {}", file.display());
    println!("语言: {language}");
    println!("目标: {resolved_target}");

    let runner = LegacyVmRunner::new();
    match resolved_target.as_str() {
        "native" => {
            let output_path = file.with_extension("exe");
            let artifact = runner.compile_module(&language, &source, file_file_stem(file))?;
            match artifact {
                legacy_vm::compiler::CompileArtifact::Native(residual) => {
                    let bytes = runner.emit_native_pe(&residual, &output_path)?;
                    println!("输出: {} ({} 字节, specialize → native PE)", output_path.display(), bytes.len());
                }
                legacy_vm::compiler::CompileArtifact::BytecodeProbe(_) => {
                    // Non-host-script stubs: empty PE shell until a native specialize hook exists.
                    runner.compile_to_pe("main", &[], &output_path)?;
                    println!("输出: {} (bytecode-probe stub PE)", output_path.display());
                }
            }
        }
        "nyar-vm" => {
            let artifact = runner.compile_module(&language, &source, file_file_stem(file))?;
            match artifact {
                legacy_vm::compiler::CompileArtifact::BytecodeProbe(module) => {
                    let bytecode = runner.compile_to_nyar(&module)?;
                    let output_path = file.with_extension("nyar");
                    fs::write(&output_path, bytecode)
                        .into_diagnostic()
                        .wrap_err_with(|| format!("failed to write nyar module: {}", output_path.display()))?;
                    println!("输出: {} ({} 字节, probe)", output_path.display(), output_path.metadata().map(|m| m.len()).unwrap_or(0));
                }
                legacy_vm::compiler::CompileArtifact::Native(_) => {
                    return Err(miette!("host-script PE specializes to native (residual → PE), not nyar-vm; use --target native"));
                }
            }
        }
        other => {
            println!("注意: 目标 '{other}' 的编译暂未实现");
            println!("预期输出: {}", file.with_extension(extension_for_target(other)).display());
        }
    }

    Ok(())
}

fn list_support() -> Result<()> {
    let runner = LegacyVmRunner::new();
    let languages = runner.languages();

    println!("支持的语言:");
    for language in languages {
        let flag = language_short_flag(&language.name).map(|flag| format!(" (--{flag})")).unwrap_or_default();
        println!("  - {}{flag}", language.name);
    }

    println!();
    println!("支持的目标:");
    for (canonical, aliases) in target_groups() {
        println!("  - {canonical} (别名: {aliases})");
    }

    println!();
    println!("共 {} 种语言, {} 个目标", runner.languages().len(), target_groups().len());
    Ok(())
}

fn resolve_language(runner: &LegacyVmRunner, explicit: Option<&str>, source: &str, file: Option<&Path>) -> Result<String> {
    if let Some(language) = explicit {
        if !runner.is_language_supported(language) {
            return Err(miette!("language '{language}' is not supported"));
        }
        return Ok(language.to_ascii_lowercase());
    }

    if let Some(path) = file {
        return LegacyVmRunner::detect_language_from_file(path.to_string_lossy().as_ref(), source)
            .ok_or_else(|| miette!("unable to detect language; use --language"));
    }

    LegacyVmRunner::detect_language_from_content(source).ok_or_else(|| miette!("unable to detect language; use --language"))
}

fn resolve_target_alias(target: &str) -> String {
    static TARGET_ALIASES: &[(&str, &str)] = &[
        ("pe", "native"),
        ("exe", "native"),
        ("native", "native"),
        ("nyar", "nyar-vm"),
        ("nyar-vm", "nyar-vm"),
        ("nyar_vm", "nyar-vm"),
        ("nyarvm", "nyar-vm"),
        ("nvm", "nyar-vm"),
        ("jvm", "jvm"),
        ("java", "jvm"),
        ("clr", "clr"),
        ("dotnet", "clr"),
        ("net", "clr"),
        ("wasm", "wasm"),
        ("wasi", "wasi"),
    ];

    TARGET_ALIASES
        .iter()
        .find_map(|(alias, canonical)| alias.eq_ignore_ascii_case(target).then_some((*canonical).to_string()))
        .unwrap_or_else(|| target.to_ascii_lowercase())
}

fn target_groups() -> Vec<(&'static str, String)> {
    let aliases: BTreeMap<&str, Vec<&str>> = [
        ("legacy-vm", vec!["legacy-vm", "legacy_vm", "legacyvm", "interpret"]),
        ("native", vec!["pe", "exe", "native"]),
        ("nyar-vm", vec!["nyar", "nyar-vm", "nyar_vm", "nyarvm", "nvm"]),
        ("jvm", vec!["jvm", "java"]),
        ("clr", vec!["clr", "dotnet", "net"]),
        ("wasm", vec!["wasm"]),
        ("wasi", vec!["wasi"]),
    ]
    .into_iter()
    .collect();

    aliases.into_iter().map(|(canonical, values)| (canonical, values.into_iter().collect::<Vec<_>>().join(", "))).collect()
}

fn language_short_flag(language: &str) -> Option<&'static str> {
    match language {
        "python" => Some("py"),
        "javascript" => Some("js"),
        "typescript" => Some("ts"),
        "lua" => Some("lua"),
        "tcl" => Some("tcl"),
        "bash" => Some("sh"),
        "powershell" => Some("ps1"),
        "c" => Some("c"),
        "rust" => Some("rs"),
        "julia" => Some("jl"),
        _ => None,
    }
}

fn extension_for_target(target: &str) -> &'static str {
    match target {
        "native" => "exe",
        "nyar-vm" => "nyar",
        "jvm" => "class",
        "clr" => "dll",
        "wasm" | "wasi" => "wasm",
        _ => "out",
    }
}

fn file_file_stem(file: &Path) -> &str {
    file.file_stem().and_then(|value| value.to_str()).unwrap_or("main")
}
