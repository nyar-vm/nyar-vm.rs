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
        /// Begin a nested phase intent JSON before run (source forced to phase_event).
        #[arg(long = "phase-json")]
        phase_json: Option<String>,
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
        Commands::Run { module, entry, json, args_json, workload_json: workload_json_arg, workload_file, phase_json } => {
            let bytes = fs::read(&module).into_diagnostic().wrap_err_with(|| format!("failed to read module file: {}", module.display()))?;
            let mut vm = NyarVm::new();
            if let Some(path) = workload_file {
                let text = fs::read_to_string(&path)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("failed to read workload file: {}", path.display()))?;
                let intent = workload_json::parse_workload_intent_json(&text)
                    .map_err(|error| miette::miette!("failed to parse --workload-file: {error}"))?;
                let decision = vm.apply_workload_intent(intent).map_err(|error| miette::miette!("failed to apply workload intent: {error}"))?;
                eprintln!("workload: {}", decision.reason);
            }
            else if let Some(source) = workload_json_arg {
                let intent = workload_json::parse_workload_intent_json(&source)
                    .map_err(|error| miette::miette!("failed to parse --workload-json: {error}"))?;
                let decision = vm.apply_workload_intent(intent).map_err(|error| miette::miette!("failed to apply workload intent: {error}"))?;
                eprintln!("workload: {}", decision.reason);
            }
            let mut active_phase: Option<String> = None;
            if let Some(source) = phase_json {
                let mut intent = workload_json::parse_workload_intent_json(&source)
                    .map_err(|error| miette::miette!("failed to parse --phase-json: {error}"))?;
                intent.source = nyar_vm::IntentSource::PhaseEvent;
                if intent.phase.is_none() {
                    return Err(miette::miette!("--phase-json requires a `phase` field"));
                }
                active_phase = intent.phase.clone();
                let decision = vm.begin_workload_phase(intent).map_err(|error| miette::miette!("failed to begin workload phase: {error}"))?;
                eprintln!("phase begin: {}", decision.reason);
            }
            let loaded = vm.load(&bytes).map_err(|error| miette::miette!("failed to load module `{}`: {error}", module.display()))?;
            let args = match args_json {
                Some(source) => json_bridge::parse_call_args_json_with_heap(&source, vm.heap_mut()).wrap_err("failed to parse --args-json")?,
                None => Vec::new(),
            };
            let result = vm.run(&loaded, &entry, args).map_err(|error| miette::miette!("failed to run entry `{entry}`: {error}"))?;
            if let Some(phase) = active_phase {
                let decision =
                    vm.end_workload_phase(Some(phase.as_str())).map_err(|error| miette::miette!("failed to end workload phase: {error}"))?;
                eprintln!("phase end: {}", decision.reason);
            }
            if json {
                println!("{}", serde_json::to_string(&json_bridge::value_to_json_with_heap(&result, Some(vm.heap()))?).into_diagnostic()?);
            }
            else {
                println!("{result}");
            }
        }
        Commands::List { module } => {
            let bytes = fs::read(&module).into_diagnostic().wrap_err_with(|| format!("failed to read module file: {}", module.display()))?;
            let loaded =
                NyarVm::new().load(&bytes).map_err(|error| miette::miette!("failed to load module `{}`: {error}", module.display()))?;
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
