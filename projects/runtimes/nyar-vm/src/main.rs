use std::{fs, path::PathBuf};

use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Result, WrapErr};
use nyar_vm::{NyarVm, json_bridge, value::Value};

/// Nyar VM command-line runner.
#[derive(Debug, Parser)]
#[command(name = "nyar-vm", about = "Execute Nyar VM bytecode modules")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Debug, Subcommand)]
enum Commands {
    /// Run a `.nyar` module entry function.
    Run {
        /// Path to the `.nyar` module file.
        module: PathBuf,
        /// Exported entry function name.
        #[arg(long, default_value = "main")]
        entry: String,
        /// Print the return value as JSON on stdout.
        #[arg(long)]
        json: bool,
        /// JSON array of call arguments, e.g. `[1, 2]` or `[{"nums":[1,2],"target":3}]`.
        #[arg(long = "args-json")]
        args_json: Option<String>,
    },
    /// List exported symbols in a `.nyar` module.
    List {
        /// Path to the `.nyar` module file.
        module: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { module, entry, json, args_json } => {
            let bytes = fs::read(&module).into_diagnostic().wrap_err_with(|| format!("failed to read module file: {}", module.display()))?;
            let mut vm = NyarVm::new();
            let loaded = vm.load(&bytes).wrap_err_with(|| format!("failed to load module: {}", module.display()))?;
            let args = match args_json {
                Some(source) => json_bridge::parse_call_args_json(&source).wrap_err("failed to parse --args-json")?,
                None => Vec::new(),
            };
            let result = vm
                .run(&loaded, &entry, args)
                .map_err(|error| miette::miette!("failed to run entry `{entry}`: {error}"))?;
            if json {
                println!("{}", serde_json::to_string(&json_bridge::value_to_json(&result)).into_diagnostic()?);
            } else {
                println!("{result}");
            }
        }
        Commands::List { module } => {
            let bytes = fs::read(&module).into_diagnostic().wrap_err_with(|| format!("failed to read module file: {}", module.display()))?;
            let loaded = NyarVm::new().load(&bytes).wrap_err_with(|| format!("failed to load module: {}", module.display()))?;
            for export in &loaded.exports {
                println!("{}\t{:?}", export.symbol_name, export.kind);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_args_json_array() {
        let args = json_bridge::parse_call_args_json("[1, 2, 3]").expect("parse");
        assert_eq!(args.len(), 3);
        assert!(matches!(args[0], Value::I32(1)));
    }
}
