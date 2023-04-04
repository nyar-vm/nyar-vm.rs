use std::{fs, path::PathBuf};

use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Result, WrapErr};
use nyar_vm::{NyarVm, json_bridge, workload_json};

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
        /// Process-level workload intent JSON (pause budget, soft heap limit, preferred GC mode).
        #[arg(long = "workload-json")]
        workload_json: Option<String>,
        /// Path to a workload intent JSON file (alternative to `--workload-json`).
        #[arg(long = "workload-file")]
        workload_file: Option<PathBuf>,
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
        Commands::Run {
            module,
            entry,
            json,
            args_json,
            workload_json: workload_json_arg,
            workload_file,
        } => {
            let bytes = fs::read(&module).into_diagnostic().wrap_err_with(|| format!("failed to read module file: {}", module.display()))?;
            let mut vm = NyarVm::new();
            if let Some(path) = workload_file {
                let text = fs::read_to_string(&path)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("failed to read workload file: {}", path.display()))?;
                let intent = workload_json::parse_workload_intent_json(&text)
                    .map_err(|error| miette::miette!("failed to parse --workload-file: {error}"))?;
                let decision = vm
                    .apply_workload_intent(intent)
                    .map_err(|error| miette::miette!("failed to apply workload intent: {error}"))?;
                eprintln!("workload: {}", decision.reason);
            } else if let Some(source) = workload_json_arg {
                let intent = workload_json::parse_workload_intent_json(&source)
                    .map_err(|error| miette::miette!("failed to parse --workload-json: {error}"))?;
                let decision = vm
                    .apply_workload_intent(intent)
                    .map_err(|error| miette::miette!("failed to apply workload intent: {error}"))?;
                eprintln!("workload: {}", decision.reason);
            }
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
        use nyar_vm::value::Value;
        let args = json_bridge::parse_call_args_json("[1, 2, 3]").expect("parse");
        assert_eq!(args.len(), 3);
        assert!(matches!(args[0], Value::I32(1)));
    }
}
