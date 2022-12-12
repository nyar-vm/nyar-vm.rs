use std::{
    path::Path,
    process::{Command, Stdio},
};

use crate::{PackageManagerError, Result};

/// Result of executing a lifecycle script.
#[derive(Debug, Clone)]
pub struct ScriptResult {
    pub name: String,
    pub success: bool,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
}

/// Run scripts from package context.
pub struct ScriptRunner<'a> {
    package_path: &'a Path,
}

impl<'a> ScriptRunner<'a> {
    pub fn new(package_path: &'a Path) -> Self {
        Self { package_path }
    }

    pub fn run(&self, name: &str, command: &str) -> Result<ScriptResult> {
        self.run_with_env(name, command, &[])
    }

    /// Run a lifecycle script with additional process environment variables.
    ///
    /// Product CLIs may inject import/search paths (e.g. `PYTHONPATH`, `NODE_PATH`)
    /// without teaching this crate language-specific layout rules.
    pub fn run_with_env(&self, name: &str, command: &str, env: &[(&str, &str)]) -> Result<ScriptResult> {
        let mut cmd = if cfg!(windows) {
            let mut c = Command::new("cmd");
            c.args(["/C", command]);
            c
        }
        else {
            let mut c = Command::new("sh");
            c.args(["-c", command]);
            c
        };
        cmd.current_dir(self.package_path).stdout(Stdio::piped()).stderr(Stdio::piped());
        for (key, value) in env {
            cmd.env(key, value);
        }
        let output = cmd.output().map_err(|error| PackageManagerError::message(format!("failed to run script `{name}`: {error}")))?;

        Ok(ScriptResult {
            name: name.to_string(),
            success: output.status.success(),
            exit_code: output.status.code(),
            stdout: String::from_utf8_lossy(&output.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
        })
    }
}
