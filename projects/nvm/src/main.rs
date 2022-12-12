use std::{fs, path::PathBuf};

use clap::{Parser, Subcommand};
use miette::{IntoDiagnostic, Result, WrapErr};
use nvm::NyarVm;

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
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    match cli.command {
        Commands::Run { module, entry } => {
            let bytes = fs::read(&module).into_diagnostic().wrap_err_with(|| format!("failed to read module file: {}", module.display()))?;
            let mut vm = NyarVm::new();
            let loaded = vm.load(&bytes).wrap_err_with(|| format!("failed to load module: {}", module.display()))?;
            let result = vm.run(&loaded, &entry, Vec::new()).wrap_err_with(|| format!("failed to run entry `{entry}`"))?;
            println!("{result}");
        }
    }
    Ok(())
}
