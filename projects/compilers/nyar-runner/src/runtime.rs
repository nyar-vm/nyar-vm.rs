use std::{
    fmt::{Display, Formatter},
    path::Path,
    process::Command,
    str::FromStr,
};

use miette::{IntoDiagnostic, Result};

/// 统一运行时家族门面。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum RuntimeFamily {
    /// `CLR / dotnet`
    Clr,
    /// `JVM / java`
    Jvm,
    /// `Node.js`
    Node,
    /// Windows 原生进程。
    Windows,
    /// `WASI / wasmtime`
    Wasi,
    /// `NyarVM`
    NyarVm,
}

impl RuntimeFamily {
    /// 解析运行时家族。
    pub fn parse(value: &str) -> Option<Self> {
        match value.to_ascii_lowercase().as_str() {
            "clr" => Some(Self::Clr),
            "jvm" => Some(Self::Jvm),
            "node" => Some(Self::Node),
            "windows" => Some(Self::Windows),
            "wasi" => Some(Self::Wasi),
            "nyar-vm" | "nyarvm" | "nyar" | "nvm" => Some(Self::NyarVm),
            _ => None,
        }
    }

    /// 返回标准字符串。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Clr => "clr",
            Self::Jvm => "jvm",
            Self::Node => "node",
            Self::Windows => "windows",
            Self::Wasi => "wasi",
            Self::NyarVm => "nyar-vm",
        }
    }

    /// 返回默认运行模板。
    pub fn default_template(self, contract: Option<RuntimeContract<'_>>) -> RuntimeTemplate {
        match self {
            Self::Clr => {
                RuntimeTemplate { family: Self::Clr, command: "dotnet".to_string(), args: vec!["exec".to_string(), "{artifact}".to_string()] }
            }
            Self::Jvm => {
                if contract.is_some() {
                    RuntimeTemplate { family: Self::Jvm, command: "java".to_string(), args: vec!["-jar".to_string(), "{artifact}".to_string()] }
                }
                else {
                    RuntimeTemplate {
                        family: Self::Jvm,
                        command: "java".to_string(),
                        args: vec!["-cp".to_string(), "{classpath}".to_string(), "{entry}".to_string()],
                    }
                }
            }
            Self::Node => RuntimeTemplate { family: Self::Node, command: "node".to_string(), args: vec!["{artifact}".to_string()] },
            Self::Windows => RuntimeTemplate { family: Self::Windows, command: "{artifact}".to_string(), args: Vec::new() },
            Self::Wasi => {
                // Component-model command: wasmtime run with GC + linear-memory hardening.
                let mut args = vec![
                    "run".to_string(),
                    "-W".to_string(),
                    "gc".to_string(),
                    "-W".to_string(),
                    "wmemcheck".to_string(),
                    "-W".to_string(),
                    "max-memory-size=16777216".to_string(),
                    "{artifact}".to_string(),
                ];
                if contract.is_some_and(|item| item.wasi_p3) {
                    let insert_at = args.len().saturating_sub(1);
                    args.insert(insert_at, "-S".to_string());
                    args.insert(insert_at + 1, "p3".to_string());
                }
                RuntimeTemplate { family: Self::Wasi, command: "wasmtime".to_string(), args }
            }
            Self::NyarVm => RuntimeTemplate {
                family: Self::NyarVm,
                command: "nyar-vm".to_string(),
                args: vec!["run".to_string(), "{artifact}".to_string(), "--entry".to_string(), "{entry}".to_string()],
            },
        }
    }
}

impl Display for RuntimeFamily {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for RuntimeFamily {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Self::parse(s).ok_or_else(|| format!("unsupported runtime family '{}'", s))
    }
}

/// 运行契约视图。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeContract<'a> {
    /// 逻辑入口名。
    pub logical_entry: Option<&'a str>,
    /// 物理入口名。
    pub physical_entry: Option<&'a str>,
    /// 启用 wasmtime `-S p3`（WASI 0.3 / wasip3 APIs）。
    pub wasi_p3: bool,
}

/// 运行模板。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeTemplate {
    /// 运行时家族。
    pub family: RuntimeFamily,
    /// 启动命令。
    pub command: String,
    /// 启动参数模板。
    pub args: Vec<String>,
}

impl RuntimeTemplate {
    /// 按占位符展开为可执行命令。
    pub fn prepare_command(&self, artifact: &Path, classpath: &Path, entry: &str) -> PreparedRuntimeCommand {
        let artifact = strip_verbatim_prefix(artifact.to_string_lossy().as_ref()).to_owned();
        let classpath = strip_verbatim_prefix(classpath.to_string_lossy().as_ref()).to_owned();
        let placeholders = [("artifact", artifact.as_str()), ("classpath", classpath.as_str()), ("entry", entry)];

        PreparedRuntimeCommand {
            family: self.family,
            command: expand_placeholders(&self.command, &placeholders),
            args: self.args.iter().map(|item| expand_placeholders(item, &placeholders)).collect(),
        }
    }
}

/// 已展开占位符、可直接执行的运行命令。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedRuntimeCommand {
    /// 运行时家族。
    pub family: RuntimeFamily,
    /// 启动命令。
    pub command: String,
    /// 启动参数。
    pub args: Vec<String>,
}

impl PreparedRuntimeCommand {
    /// 在指定工作目录执行命令并返回退出码。
    pub fn run_in(&self, cwd: &Path) -> Result<i32> {
        let status = Command::new(&self.command).args(&self.args).current_dir(cwd).status().into_diagnostic()?;

        Ok(status.code().unwrap_or(1))
    }
}

fn expand_placeholders(template: &str, placeholders: &[(&str, &str)]) -> String {
    placeholders.iter().fold(template.to_string(), |current, (key, value)| current.replace(&format!("{{{}}}", key), value))
}

fn strip_verbatim_prefix(path: &str) -> &str {
    path.strip_prefix(r"\\?\").unwrap_or(path)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::{RuntimeContract, RuntimeFamily};

    #[test]
    fn jvm_defaults_to_classpath_without_contract() {
        let template = RuntimeFamily::Jvm.default_template(None);
        assert_eq!(template.command, "java");
        assert_eq!(template.args, vec!["-cp".to_string(), "{classpath}".to_string(), "{entry}".to_string()]);
    }

    #[test]
    fn jvm_uses_jar_mode_with_contract() {
        let template = RuntimeFamily::Jvm.default_template(Some(RuntimeContract {
            logical_entry: Some("main"),
            physical_entry: Some("app"),
            wasi_p3: false,
        }));
        assert_eq!(template.command, "java");
        assert_eq!(template.args, vec!["-jar".to_string(), "{artifact}".to_string()]);
    }

    #[test]
    fn wasi_defaults_to_component_command_mode() {
        let invoke =
            RuntimeFamily::Wasi.default_template(Some(RuntimeContract { logical_entry: Some("main"), physical_entry: None, wasi_p3: false }));
        assert_eq!(
            invoke.args,
            vec![
                "run".to_string(),
                "-W".to_string(),
                "gc".to_string(),
                "-W".to_string(),
                "wmemcheck".to_string(),
                "-W".to_string(),
                "max-memory-size=16777216".to_string(),
                "{artifact}".to_string(),
            ]
        );

        let start =
            RuntimeFamily::Wasi.default_template(Some(RuntimeContract { logical_entry: Some("_start"), physical_entry: None, wasi_p3: false }));
        assert_eq!(
            start.args,
            vec![
                "run".to_string(),
                "-W".to_string(),
                "gc".to_string(),
                "-W".to_string(),
                "wmemcheck".to_string(),
                "-W".to_string(),
                "max-memory-size=16777216".to_string(),
                "{artifact}".to_string(),
            ]
        );

        let p3 =
            RuntimeFamily::Wasi.default_template(Some(RuntimeContract { logical_entry: Some("_start"), physical_entry: None, wasi_p3: true }));
        assert!(p3.args.windows(2).any(|pair| pair == ["-S", "p3"]));
    }

    #[test]
    fn expands_runtime_template_placeholders() {
        let template = RuntimeFamily::Clr.default_template(None);
        let command = template.prepare_command(Path::new(r"\\?\C:\tmp\app.exe"), Path::new("ignored"), "ignored");
        assert_eq!(command.command, "dotnet");
        assert_eq!(command.args, vec!["exec".to_string(), r"C:\tmp\app.exe".to_string()]);
    }
}
