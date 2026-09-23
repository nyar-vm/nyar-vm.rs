use super::{assert_or_regenerate_yaml_sidecar, load_optional_yaml_sidecar};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

/// 运行时 fixture 的目标声明与期望结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuntimeFixtureSpec {
    /// 声明该 fixture 需要覆盖的运行目标。
    #[serde(default)]
    pub targets: Vec<String>,
    /// 每个目标对应的期望运行结果。
    #[serde(default)]
    pub expect: BTreeMap<String, RuntimeFixtureResult>,
}

/// 单个运行目标的真实执行结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct RuntimeFixtureResult {
    /// 子进程是否以成功状态退出。
    #[serde(default)]
    pub success: bool,
    /// 标准输出按行归一化后的结果。
    #[serde(default)]
    pub stdout: Vec<String>,
    /// 标准错误按行归一化后的结果。
    #[serde(default)]
    pub stderr: Vec<String>,
    /// 是否允许该 case 在 stderr 非空时仍视为成功。
    #[serde(default)]
    pub allow_stderr: bool,
    /// 构建或运行阶段收集到的逻辑错误。
    #[serde(default)]
    pub errors: Vec<String>,
    /// 进程退出码；无法启动时为 `None`。
    pub result: Option<i32>,
}

/// 读取运行时 fixture 的 YAML sidecar；不存在时返回 `None`。
pub fn load_runtime_fixture_spec(fixture_path: &Path) -> Option<RuntimeFixtureSpec> {
    load_optional_yaml_sidecar(fixture_path)
}

/// 返回 fixture 实际需要执行的目标列表；若 sidecar 未声明则退回默认目标。
pub fn resolve_runtime_fixture_targets(fixture_path: &Path, default_targets: &[&str]) -> Vec<String> {
    let spec = load_runtime_fixture_spec(fixture_path);
    resolve_runtime_fixture_targets_from_spec(fixture_path, spec.as_ref(), default_targets)
}

/// 基于观测闭包执行通用的 runtime fixture 校验。
pub fn verify_runtime_fixture_case<F>(fixture_path: &Path, default_targets: &[&str], regenerate: bool, mut observe: F)
where
    F: FnMut(&str) -> RuntimeFixtureResult,
{
    let targets = resolve_runtime_fixture_targets(fixture_path, default_targets);
    let mut expect = BTreeMap::new();
    for target in &targets {
        expect.insert(target.clone(), observe(target));
    }
    let observed = RuntimeFixtureSpec { targets, expect };
    verify_runtime_fixture_spec(fixture_path, &observed, regenerate);
}

/// 在首次运行或显式重生成时写入运行时基线；否则与已有基线进行比对。
pub fn verify_runtime_fixture_spec(fixture_path: &Path, observed: &RuntimeFixtureSpec, regenerate: bool) {
    assert_or_regenerate_yaml_sidecar(fixture_path, observed, regenerate);
}

fn resolve_runtime_fixture_targets_from_spec(fixture_path: &Path, spec: Option<&RuntimeFixtureSpec>, default_targets: &[&str]) -> Vec<String> {
    let targets = match spec {
        Some(spec) if !spec.targets.is_empty() => spec.targets.clone(),
        _ => default_targets.iter().map(|target| (*target).to_string()).collect(),
    };
    assert!(!targets.is_empty(), "runtime fixture '{}' has no targets", fixture_path.display());
    targets
}

#[cfg(test)]
mod tests {
    use super::{
        RuntimeFixtureResult, RuntimeFixtureSpec, load_runtime_fixture_spec, resolve_runtime_fixture_targets, verify_runtime_fixture_case,
    };
    use std::{cell::RefCell, collections::BTreeMap, fs};

    #[test]
    fn resolves_targets_from_sidecar_before_defaults() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fixture_path = temp_dir.path().join("sample.demo");
        fs::write(&fixture_path, "fixture").unwrap();
        let sidecar = RuntimeFixtureSpec { targets: vec!["node".to_string(), "wasi".to_string()], expect: BTreeMap::new() };
        let yaml = serde_yaml::to_string(&sidecar).unwrap();
        fs::write(temp_dir.path().join("sample.demo.yaml"), yaml).unwrap();

        let targets = resolve_runtime_fixture_targets(&fixture_path, &["clr", "jvm"]);

        assert_eq!(targets, vec!["node".to_string(), "wasi".to_string()]);
    }

    #[test]
    fn verifies_runtime_fixture_case_with_default_targets() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fixture_path = temp_dir.path().join("sample.demo");
        fs::write(&fixture_path, "fixture").unwrap();
        let observed = RefCell::new(Vec::new());

        verify_runtime_fixture_case(&fixture_path, &["clr", "node"], false, |target| {
            observed.borrow_mut().push(target.to_string());
            RuntimeFixtureResult {
                success: true,
                stdout: vec![format!("hello-{target}")],
                stderr: Vec::new(),
                allow_stderr: false,
                errors: Vec::new(),
                result: Some(0),
            }
        });

        assert_eq!(observed.into_inner(), vec!["clr".to_string(), "node".to_string()]);
        let saved = load_runtime_fixture_spec(&fixture_path).unwrap();
        assert_eq!(saved.targets, vec!["clr".to_string(), "node".to_string()]);
        assert_eq!(saved.expect["clr"].stdout, vec!["hello-clr".to_string()]);
        assert_eq!(saved.expect["node"].stdout, vec!["hello-node".to_string()]);
    }
}
