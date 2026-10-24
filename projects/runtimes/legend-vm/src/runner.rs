//! Unified legacy VM runner.

use std::{collections::HashMap, path::Path};

use miette::{Result, miette};

use crate::{
    compiler::{CompileArtifact, NativeResidualModule, NyarBytecodeCompiler, PeCompiler, StackCompiler, emit_native_pe},
    evaluator::{evaluate_bash_script, evaluate_c_script, evaluate_lua_script, evaluate_powershell_script, evaluate_tcl_script},
    guest::{FnGuestInterpret, GuestInterpretFn},
    value::LegacyValue,
};

/// Language metadata for `list` output.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LanguageInfo {
    /// Canonical language name.
    pub name: String,
}

/// Legacy VM unified runner.
#[derive(Debug, Default)]
pub struct LegacyVmRunner {
    evaluators: HashMap<String, GuestInterpretFn>,
}

impl LegacyVmRunner {
    /// Create a runner with built-in language evaluators.
    pub fn new() -> Self {
        let mut runner = Self::default();
        runner.register_defaults();
        runner
    }

    /// Register a raw [`GuestInterpretFn`] under one language id / alias.
    pub fn register(&mut self, language: impl Into<String>, interpret: GuestInterpretFn) {
        self.evaluators.insert(language.into(), interpret);
    }

    /// Register a static [`FnGuestInterpret`] under all of its ids.
    pub fn register_fn_guest(&mut self, guest: FnGuestInterpret) {
        for id in guest.ids {
            self.evaluators.insert((*id).to_string(), guest.interpret);
        }
    }

    fn register_defaults(&mut self) {
        let guests: &[FnGuestInterpret] = &[
            FnGuestInterpret { ids: &["bash", "sh"], interpret: evaluate_bash_script },
            FnGuestInterpret { ids: &["powershell", "ps1", "pwsh"], interpret: evaluate_powershell_script },
            FnGuestInterpret { ids: &["lua"], interpret: evaluate_lua_script },
            FnGuestInterpret { ids: &["tcl"], interpret: evaluate_tcl_script },
            FnGuestInterpret { ids: &["c"], interpret: evaluate_c_script },
            FnGuestInterpret { ids: &["javascript", "js"], interpret: evaluate_javascript_stub },
            FnGuestInterpret { ids: &["python", "py"], interpret: evaluate_python_stub },
        ];
        for guest in guests {
            self.register_fn_guest(*guest);
        }
    }

    /// Return supported language metadata.
    pub fn languages(&self) -> Vec<LanguageInfo> {
        let mut names: Vec<_> = self.evaluators.keys().cloned().collect();
        names.sort();
        names.dedup();
        names.into_iter().map(|name| LanguageInfo { name }).collect()
    }

    /// Check whether a language is supported.
    pub fn is_language_supported(&self, language: &str) -> bool {
        self.evaluators.contains_key(&language.to_ascii_lowercase())
    }

    /// Run source for a language.
    pub fn run(&self, language: &str, source: &str, env: &mut HashMap<String, LegacyValue>) -> Result<LegacyValue> {
        let language = language.to_ascii_lowercase();
        let evaluator = self.evaluators.get(&language).copied().ok_or_else(|| miette!("language '{language}' is not supported"))?;
        Ok(evaluator(source, env))
    }

    /// Compile source for the language's supported artifact lane.
    ///
    /// Host-script languages: Futamura specialize → [`CompileArtifact::Native`]
    /// (native residual → PE). JS/Python remain bytecode-pipe **probes** only.
    pub fn compile_module(&self, language: &str, source: &str, module_name: &str) -> Result<CompileArtifact> {
        let language = language.to_ascii_lowercase();
        match language.as_str() {
            "javascript" | "js" | "typescript" | "ts" => Ok(CompileArtifact::BytecodeProbe(compile_js_stub(source, module_name))),
            "python" | "py" => Ok(CompileArtifact::BytecodeProbe(compile_python_stub(source, module_name))),
            "lua" | "bash" | "sh" | "powershell" | "ps1" | "pwsh" | "tcl" | "c" => {
                crate::compiler::specialize_language(&language, source, module_name)
                    .map(CompileArtifact::Native)
                    .map_err(|error| miette!("{error}"))
            }
            other => Err(miette!("language '{other}' does not support compilation")),
        }
    }

    /// Specialize a host-script language into a native-bound residual module.
    pub fn specialize_native(&self, language: &str, source: &str, module_name: &str) -> Result<NativeResidualModule> {
        match self.compile_module(language, source, module_name)? {
            CompileArtifact::Native(module) => Ok(module),
            CompileArtifact::BytecodeProbe(_) => {
                Err(miette!("language '{language}' is a bytecode probe, not a native PE specialization subject"))
            }
        }
    }

    /// Emit a Windows PE from a native residual (specialize product path).
    pub fn emit_native_pe(&self, module: &NativeResidualModule, output_path: &Path) -> Result<Vec<u8>> {
        emit_native_pe(module, Some(output_path))
    }

    /// Compile a generate-time probe module to `.nyar` bytes (not host-script PE).
    pub fn compile_to_nyar(&self, module: &crate::compiler::GenerateModule) -> Result<Vec<u8>> {
        NyarBytecodeCompiler.compile(module)
    }

    /// Compile raw x64 machine code to a PE executable.
    pub fn compile_to_pe(&self, entry_function_name: &str, x64_code: &[u8], output_path: &Path) -> Result<Vec<u8>> {
        PeCompiler.compile(entry_function_name, x64_code, output_path)
    }

    /// Detect language from a file path.
    pub fn detect_language_from_file(file_path: &str, source: &str) -> Option<String> {
        if let Some(language) = detect_language_from_shebang(source) {
            return Some(language);
        }
        Some(detect_language_from_path(file_path))
    }

    /// Detect language from source content.
    pub fn detect_language_from_content(source: &str) -> Option<String> {
        if let Some(language) = detect_language_from_shebang(source) {
            return Some(language);
        }

        let trimmed = source.trim_start();
        if trimmed.starts_with("console.log")
            || trimmed.starts_with("let ")
            || trimmed.starts_with("const ")
            || trimmed.starts_with("function ")
        {
            return Some("javascript".to_string());
        }
        if trimmed.starts_with("print(") {
            return Some("lua".to_string());
        }
        if trimmed.starts_with("def ") {
            return Some("python".to_string());
        }
        if trimmed.starts_with("puts ") || trimmed.starts_with("set ") {
            return Some("tcl".to_string());
        }
        if trimmed.starts_with("int ") || trimmed.starts_with("#include") || trimmed.contains("int main") {
            return Some("c".to_string());
        }
        if trimmed.starts_with("Write-Output ") || trimmed.starts_with('$') {
            return Some("powershell".to_string());
        }
        if trimmed.starts_with("echo ") || trimmed.starts_with("#!/bin/bash") {
            return Some("bash".to_string());
        }

        Some("bash".to_string())
    }
}

fn detect_language_from_path(file_path: &str) -> String {
    let extension = Path::new(file_path).extension().and_then(|value| value.to_str()).unwrap_or_default().to_ascii_lowercase();
    match extension.as_str() {
        "sh" | "bash" => "bash".to_string(),
        "ps1" => "powershell".to_string(),
        "lua" => "lua".to_string(),
        "tcl" => "tcl".to_string(),
        "c" | "h" => "c".to_string(),
        "js" => "javascript".to_string(),
        "ts" => "typescript".to_string(),
        "py" => "python".to_string(),
        _ => extension,
    }
}

fn detect_language_from_shebang(source: &str) -> Option<String> {
    let first_line = source.lines().next()?.trim();
    if !first_line.starts_with("#!") {
        return None;
    }
    let shebang = first_line[2..].trim();
    if shebang.contains("python") {
        return Some("python".to_string());
    }
    if shebang.contains("lua") {
        return Some("lua".to_string());
    }
    if shebang.contains("tclsh") || shebang.contains("tcl") {
        return Some("tcl".to_string());
    }
    if shebang.contains("pwsh") || shebang.contains("powershell") {
        return Some("powershell".to_string());
    }
    if shebang.contains("node") || shebang.contains("deno") || shebang.contains("bun") {
        return Some("javascript".to_string());
    }
    if shebang.contains("bash") || shebang.contains("sh") {
        return Some("bash".to_string());
    }
    None
}

fn evaluate_javascript_stub(source: &str, env: &mut HashMap<String, LegacyValue>) -> LegacyValue {
    let trimmed = source.trim();
    if let Some(rest) = trimmed.strip_prefix("console.log(") {
        let payload = rest.trim_end_matches(");").trim_end_matches(')');
        let value = LegacyValue::String(strip_js_quotes(payload));
        println!("{}", value.to_string_value());
        env.insert("_".to_string(), value.clone());
        return value;
    }
    LegacyValue::Null
}

fn evaluate_python_stub(source: &str, env: &mut HashMap<String, LegacyValue>) -> LegacyValue {
    for line in source.lines() {
        let line = line.trim();
        if let Some(rest) = line.strip_prefix("print(") {
            let payload = rest.trim_end_matches(')');
            let value = LegacyValue::String(strip_js_quotes(payload));
            println!("{}", value.to_string_value());
            env.insert("_".to_string(), value.clone());
            return value;
        }
    }
    LegacyValue::Null
}

fn compile_js_stub(_source: &str, module_name: &str) -> crate::compiler::GenerateModule {
    let mut compiler = StackCompiler::new(module_name);
    compiler.emit_main_return_i64(0);
    compiler.finish()
}

fn compile_python_stub(_source: &str, module_name: &str) -> crate::compiler::GenerateModule {
    let mut compiler = StackCompiler::new(module_name);
    compiler.emit_main_return_i64(0);
    compiler.finish()
}

fn strip_js_quotes(value: &str) -> String {
    let value = value.trim();
    if (value.starts_with('"') && value.ends_with('"')) || (value.starts_with('\'') && value.ends_with('\'')) {
        value[1..value.len().saturating_sub(1)].to_string()
    }
    else {
        value.to_string()
    }
}
