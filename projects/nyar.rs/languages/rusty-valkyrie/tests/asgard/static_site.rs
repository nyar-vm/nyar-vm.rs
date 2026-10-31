//! 静态站点构建编排测试：显式入口、`.w` / `.awsl` 同语义、失败闭合。

use std::{fs, path::PathBuf};

use rusty_valkyrie::asgard::static_site::{StaticSiteOptions, build_static_site};
use tempfile::TempDir;

/// 含 `<style>` 块与 script 字段绑定的根组件源码；widget 名与入口文件 stem（`index`）一致。
const FIXTURE: &str = r#"<widget index>
<div class="hero">
    <h1>{greeting}</h1>
</div>
</widget>
<script>
let greeting: utf8 = "你好，静态世界"
</script>
<style>.hero { color: #c67b34; }</style>
"#;

/// 在项目根内写入指定文件名的入口并构建，返回 `index.html` 内容。
fn build_page(temp: &TempDir, entry_name: &str) -> String {
    let project = temp.path();
    fs::create_dir_all(project.join("source")).expect("创建 source/ 目录失败");
    fs::write(project.join("source").join(entry_name), FIXTURE).expect("写入入口文件失败");
    let report = build_static_site(&StaticSiteOptions {
        project_dir: project.to_path_buf(),
        entry: PathBuf::from(format!("source/{entry_name}")),
        output_dir: None,
        base: "/".into(),
        title: Some("静态站点测试".into()),
    })
    .expect("静态站点构建失败");
    assert_eq!(report.component_name, "index");
    assert_eq!(report.page_path.file_name().and_then(|n| n.to_str()), Some("index.html"));
    assert!(report.output_dir.starts_with(project), "输出目录应位于项目根内");
    fs::read_to_string(&report.page_path).expect("读取 index.html 失败")
}

#[test]
fn awsl_entry_builds_styled_static_page() {
    let temp = TempDir::new().expect("创建临时目录失败");
    let html = build_page(&temp, "index.awsl");

    assert!(html.contains("<!DOCTYPE html>"), "应为完整 HTML 页面");
    assert!(html.contains("<style>"), "页面应包含样式块:\n{html}");
    assert!(html.contains(".hero { color: #c67b34; }"), "应透传组件样式文本:\n{html}");
    assert!(html.contains("asgard-static-index"), "应包含作用域 class:\n{html}");
    assert!(html.contains("你好，静态世界"), "应包含初始状态插值结果:\n{html}");
    assert!(!html.contains("<script"), "静态页面不应包含 runtime script 标签:\n{html}");
}

#[test]
fn w_entry_produces_identical_page_to_awsl() {
    // `.w` 与 `.awsl` 同格式同语义：同内容入口应产出相同的页面。
    let awsl_temp = TempDir::new().expect("创建临时目录失败");
    let w_temp = TempDir::new().expect("创建临时目录失败");
    let awsl_page = build_page(&awsl_temp, "index.awsl");
    let w_page = build_page(&w_temp, "index.w");
    assert_eq!(awsl_page, w_page, ".w 与 .awsl 入口产物应语义相同");
}

#[test]
fn default_title_falls_back_to_component_name() {
    // 标题缺省时沿用根组件名（入口文件 stem）这一唯一命名来源。
    let temp = TempDir::new().expect("创建临时目录失败");
    fs::create_dir_all(temp.path().join("source")).expect("创建 source/ 目录失败");
    fs::write(temp.path().join("source/index.w"), FIXTURE).expect("写入入口文件失败");
    let report = build_static_site(&StaticSiteOptions {
        project_dir: temp.path().to_path_buf(),
        entry: PathBuf::from("source/index.w"),
        output_dir: None,
        base: "/".into(),
        title: None,
    })
    .expect("静态站点构建失败");
    let html = fs::read_to_string(&report.page_path).expect("读取 index.html 失败");
    assert!(html.contains("<title>index</title>"), "缺省标题应为根组件名:\n{html}");
}

#[test]
fn base_prefix_is_consumed_into_base_href() {
    // `artifact.base` 由配置所有者规范化后，静态构建必须消费它：非根基址写入
    // `<base href>`，末尾目录分隔符由消费层补齐，供子目录部署使用。
    let temp = TempDir::new().expect("创建临时目录失败");
    fs::create_dir_all(temp.path().join("source")).expect("创建 source/ 目录失败");
    fs::write(temp.path().join("source/index.w"), FIXTURE).expect("写入入口文件失败");
    let report = build_static_site(&StaticSiteOptions {
        project_dir: temp.path().to_path_buf(),
        entry: PathBuf::from("source/index.w"),
        output_dir: None,
        title: Some("子目录部署".into()),
        base: "/app".into(),
    })
    .expect("带非根基址的静态构建应成功");
    let html = fs::read_to_string(&report.page_path).expect("读取 index.html 失败");
    assert!(html.contains("<base href=\"/app/\">"), "非根基址必须写入 `<base href>` 且补齐目录分隔符:\n{html}");
}

#[test]
fn invalid_base_prefix_is_rejected() {
    let temp = TempDir::new().expect("创建临时目录失败");
    fs::create_dir_all(temp.path().join("source")).expect("创建 source/ 目录失败");
    fs::write(temp.path().join("source/index.w"), FIXTURE).expect("写入入口文件失败");
    let error = build_static_site(&StaticSiteOptions {
        project_dir: temp.path().to_path_buf(),
        entry: PathBuf::from("source/index.w"),
        output_dir: None,
        title: None,
        base: "app".into(),
    })
    .expect_err("基址必须以 `/` 开头");
    assert!(error.to_string().contains("目录 URL"), "诊断应说明基址目录 URL 合同: {error}");
}

#[test]
fn rejects_unknown_entry_extension() {
    let temp = TempDir::new().expect("创建临时目录失败");
    fs::create_dir_all(temp.path().join("source")).expect("创建 source/ 目录失败");
    fs::write(temp.path().join("source/index.v"), FIXTURE).expect("写入入口文件失败");
    let error = build_static_site(&StaticSiteOptions {
        project_dir: temp.path().to_path_buf(),
        entry: PathBuf::from("source/index.v"),
        output_dir: None,
        base: "/".into(),
        title: Some("静态站点测试".into()),
    })
    .expect_err(".v 不是静态站点入口后缀，应失败");
    let message = error.to_string();
    assert!(message.contains(".w") && message.contains(".awsl"), "诊断应说明两种后缀同语义: {message}");
}

#[test]
fn rejects_missing_entry_file() {
    let temp = TempDir::new().expect("创建临时目录失败");
    fs::create_dir_all(temp.path().join("source")).expect("创建 source/ 目录失败");
    let error = build_static_site(&StaticSiteOptions {
        project_dir: temp.path().to_path_buf(),
        entry: PathBuf::from("source/missing.awsl"),
        output_dir: None,
        base: "/".into(),
        title: Some("静态站点测试".into()),
    })
    .expect_err("入口文件不存在应失败");
    assert!(error.to_string().contains("missing.awsl"), "诊断应包含入口路径: {error}");
}

#[test]
fn rejects_entry_escaping_project_dir() {
    // 入口文件真实存在于项目根之外，越界检查必须先于读取失败闭合。
    let temp = TempDir::new().expect("创建临时目录失败");
    let project = temp.path().join("proj");
    fs::create_dir_all(&project).expect("创建项目目录失败");
    fs::write(temp.path().join("outside.awsl"), FIXTURE).expect("写入越界入口文件失败");
    let error = build_static_site(&StaticSiteOptions {
        project_dir: project,
        entry: PathBuf::from("../outside.awsl"),
        output_dir: None,
        base: "/".into(),
        title: Some("静态站点测试".into()),
    })
    .expect_err("越出项目根的入口应失败");
    assert!(error.to_string().contains("越出项目根"), "诊断应说明入口越出项目根: {error}");
}
