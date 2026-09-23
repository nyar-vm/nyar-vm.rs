use serde::{Serialize, de::DeserializeOwned};
use std::{
    env,
    fmt::Debug,
    fs,
    path::{Path, PathBuf},
};

/// 收集指定扩展名的 fixture 源文件。
pub fn collect_fixture_cases_with_extensions(root: &Path, extensions: &[&str]) -> Vec<PathBuf> {
    let mut cases = Vec::new();
    collect_fixture_cases_recursive(root, extensions, &mut cases);
    cases.sort();
    cases
}

/// 返回与 fixture 源文件同级的 YAML sidecar 路径。
pub fn yaml_sidecar_path(fixture_path: &Path) -> PathBuf {
    let Some(file_name) = fixture_path.file_name().and_then(|value| value.to_str())
    else {
        panic!("invalid fixture path '{}'", fixture_path.display());
    };
    fixture_path.with_file_name(format!("{file_name}.yaml"))
}

/// 尝试读取 fixture 的 YAML sidecar；不存在时返回 `None`。
pub fn load_optional_yaml_sidecar<T>(fixture_path: &Path) -> Option<T>
where
    T: DeserializeOwned,
{
    let yaml_path = yaml_sidecar_path(fixture_path);
    if !yaml_path.exists() {
        return None;
    }

    let source =
        fs::read_to_string(&yaml_path).unwrap_or_else(|error| panic!("failed to read fixture yaml '{}': {}", yaml_path.display(), error));
    Some(serde_yaml::from_str(&source).unwrap_or_else(|error| panic!("failed to parse fixture yaml '{}': {}", yaml_path.display(), error)))
}

/// 在首次运行或显式重生成时写入 sidecar；否则与基线进行精确比对。
pub fn assert_or_regenerate_yaml_sidecar<T>(fixture_path: &Path, observed: &T, regenerate: bool)
where
    T: Serialize + DeserializeOwned + PartialEq + Debug,
{
    let yaml_path = yaml_sidecar_path(fixture_path);
    let expected = load_optional_yaml_sidecar::<T>(fixture_path);
    if regenerate || expected.is_none() {
        write_yaml_sidecar(&yaml_path, observed);
        return;
    }

    assert_eq!(observed, &expected.unwrap(), "fixture mismatch: {}", fixture_path.display());
}

/// 返回 fixture 的文本 sidecar 路径，例如 `class.v` + `lex` → `class.v.lex`。
pub fn text_sidecar_path(fixture_path: &Path, suffix: &str) -> PathBuf {
    let Some(file_name) = fixture_path.file_name().and_then(|value| value.to_str())
    else {
        panic!("invalid fixture path '{}'", fixture_path.display());
    };
    fixture_path.with_file_name(format!("{file_name}.{suffix}"))
}

/// 尝试读取文本 sidecar；不存在时返回 `None`。
pub fn load_optional_text_sidecar(sidecar_path: &Path) -> Option<String> {
    if !sidecar_path.exists() {
        return None;
    }

    let source =
        fs::read_to_string(sidecar_path).unwrap_or_else(|error| panic!("failed to read text sidecar '{}': {}", sidecar_path.display(), error));
    Some(source)
}

/// 在首次运行或显式重生成时写入文本 sidecar；否则与基线进行全文精确比对。
pub fn assert_or_regenerate_text_sidecar(fixture_path: &Path, suffix: &str, observed: &str, regenerate: bool) {
    let sidecar_path = text_sidecar_path(fixture_path, suffix);
    let expected = load_optional_text_sidecar(&sidecar_path);
    if regenerate || expected.is_none() {
        write_text_sidecar(&sidecar_path, observed);
        return;
    }

    let expected = expected.unwrap();
    // `write_text_sidecar` 规范化时会在末尾追加 `\n`，但各 dump 函数通常以 `join("\n")`
    // 产出不含尾换行的字符串。比较前对两侧 `trim_end` 以消除这一无关差异。
    if observed.trim_end() == expected.trim_end() {
        return;
    }

    panic!(
        "fixture text sidecar mismatch: {} (suffix={})\n--- expected ---\n{}\n--- observed ---\n{}",
        fixture_path.display(),
        suffix,
        expected,
        observed
    );
}

/// 是否启用 sidecar 重生成（对齐 legion / valkyrie 测试环境变量语义）。
pub fn regenerate_enabled() -> bool {
    ["VALKYRIE_TEST_REGENERATE", "LEGION_TEST_REGENERATE", "NYAR_TEST_REGENERATE"].into_iter().any(env_flag_enabled)
}

fn env_flag_enabled(name: &str) -> bool {
    env::var(name).ok().is_some_and(|value| matches!(value.trim().to_ascii_lowercase().as_str(), "1" | "true" | "yes" | "on" | "regenerate"))
}

fn write_text_sidecar(sidecar_path: &Path, content: &str) {
    let normalized = if content.ends_with('\n') { content.to_string() } else { format!("{content}\n") };
    fs::write(sidecar_path, normalized).unwrap_or_else(|error| panic!("failed to write text sidecar '{}': {}", sidecar_path.display(), error));
}

fn collect_fixture_cases_recursive(root: &Path, extensions: &[&str], cases: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root)
    else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_fixture_cases_recursive(&path, extensions, cases);
        }
        else if path
            .extension()
            .and_then(|value| value.to_str())
            .is_some_and(|value| extensions.iter().any(|ext| value.eq_ignore_ascii_case(ext)))
        {
            cases.push(path);
        }
    }
}

fn write_yaml_sidecar<T>(yaml_path: &Path, value: &T)
where
    T: Serialize,
{
    let content =
        serde_yaml::to_string(value).unwrap_or_else(|error| panic!("failed to serialize fixture yaml '{}': {}", yaml_path.display(), error));
    fs::write(yaml_path, content).unwrap_or_else(|error| panic!("failed to write fixture yaml '{}': {}", yaml_path.display(), error));
}

#[cfg(test)]
mod tests {
    use super::{
        assert_or_regenerate_text_sidecar, assert_or_regenerate_yaml_sidecar, collect_fixture_cases_with_extensions,
        load_optional_text_sidecar, load_optional_yaml_sidecar, text_sidecar_path,
    };
    use serde::{Deserialize, Serialize};
    use std::{fs, path::Path};

    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    struct DemoFixture {
        value: String,
    }

    #[test]
    fn sidecar_is_created_on_first_run() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fixture_path = temp_dir.path().join("sample.demo");
        fs::write(&fixture_path, "fixture").unwrap();

        let observed = DemoFixture { value: "first".to_string() };
        assert_or_regenerate_yaml_sidecar(&fixture_path, &observed, false);

        let saved = load_optional_yaml_sidecar::<DemoFixture>(&fixture_path).unwrap();
        assert_eq!(saved, observed);
        assert!(Path::new(&format!("{}.yaml", fixture_path.display())).exists());
    }

    #[test]
    fn text_sidecar_is_created_on_first_run() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fixture_path = temp_dir.path().join("sample.demo");
        fs::write(&fixture_path, "fixture").unwrap();

        assert_or_regenerate_text_sidecar(&fixture_path, "lex", "Token { kind: Eof }\n", false);

        let saved = load_optional_text_sidecar(&text_sidecar_path(&fixture_path, "lex")).unwrap();
        assert_eq!(saved, "Token { kind: Eof }\n");
    }

    #[test]
    fn text_sidecar_can_be_regenerated() {
        let temp_dir = tempfile::tempdir().unwrap();
        let fixture_path = temp_dir.path().join("sample.demo");
        fs::write(&fixture_path, "fixture").unwrap();

        assert_or_regenerate_text_sidecar(&fixture_path, "lex", "first\n", false);
        assert_or_regenerate_text_sidecar(&fixture_path, "lex", "second\n", true);

        let saved = load_optional_text_sidecar(&text_sidecar_path(&fixture_path, "lex")).unwrap();
        assert_eq!(saved, "second\n");
    }

    #[test]
    fn collection_filters_requested_extensions() {
        let temp_dir = tempfile::tempdir().unwrap();
        fs::write(temp_dir.path().join("one.demo"), "").unwrap();
        fs::write(temp_dir.path().join("two.txt"), "").unwrap();
        fs::create_dir_all(temp_dir.path().join("nested")).unwrap();
        fs::write(temp_dir.path().join("nested").join("three.DEMO"), "").unwrap();

        let cases = collect_fixture_cases_with_extensions(temp_dir.path(), &["demo"]);

        assert_eq!(cases.len(), 2);
        assert!(
            cases.iter().all(|path| path.extension().and_then(|value| value.to_str()).is_some_and(|value| value.eq_ignore_ascii_case("demo")))
        );
    }
}
