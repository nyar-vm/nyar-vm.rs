//! `build --static` 产物树合同：文件布局、自包含引用与配置字段消费。
//!
//! 当前静态交付线（账本「Asgard 静态页面交付线」）只编排显式入口的 AWSL 初始渲染；
//! 浏览器交互运行时（JS glue + `.wasm` chunk）尚未接入。本文件固化的当前真实形态：
//! - 产物树 = 仅 `index.html`；
//! - 严格模型校验后的 `artifact.base` 已作为构建输入被消费（写入 `<base href>`）；
//! - `artifact.chunks` 目前只完成配置形状校验，成对 JS/Wasm 产物仍待交互运行时。
//! 转红触发器：chunk 成对产物落地后，本文件的产物树用例必须升级为 manifest 断言。

use std::{fs, path::PathBuf};

use rusty_valkyrie::asgard::{
    config_model::{ConfigOrigin, ConfigValue, resolve_static_config},
    static_site::{StaticSiteOptions, build_static_site},
};
use tempfile::TempDir;

/// 带样式与初始状态绑定的根组件入口 fixture。
const FIXTURE: &str = r#"<widget index>
<div class="hero">
    <h1>{greeting}</h1>
</div>
</widget>
<script>
let greeting: utf8 = "静态产物树"
</script>
<style>.hero { color: #345; }</style>
"#;

/// 在项目根写入入口并构建，返回输出目录内的文件清单（相对输出根，`/` 分隔）。
fn build_and_list(project: &std::path::Path, output: &std::path::Path, base: &str) -> Vec<String> {
    fs::create_dir_all(project.join("source")).expect("创建 source/ 目录失败");
    fs::write(project.join("source/index.w"), FIXTURE).expect("写入入口文件失败");
    let report = build_static_site(&StaticSiteOptions {
        project_dir: project.to_path_buf(),
        entry: PathBuf::from("source/index.w"),
        output_dir: Some(output.to_path_buf()),
        title: Some("产物树测试".into()),
        base: base.to_string(),
    })
    .expect("静态站点构建应成功");
    assert_eq!(report.page_path, output.join("index.html"), "报告页面路径应落在输出目录");
    let mut files = Vec::new();
    collect_files(output, output, &mut files);
    files.sort();
    files
}

fn collect_files(root: &std::path::Path, dir: &std::path::Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).expect("读取输出目录失败") {
        let path = entry.expect("目录项").path();
        if path.is_dir() {
            collect_files(root, &path, out);
        }
        else {
            out.push(path.strip_prefix(root).unwrap().to_string_lossy().replace('\\', "/"));
        }
    }
}

#[test]
fn static_build_output_tree_is_index_html_only() {
    // 当前形态：静态交付只落一个自包含 `index.html`，没有 `.mjs`/`.wasm`/chunk/资源拷贝。
    // 转红触发器：浏览器交互运行时与 wasm chunk 拆分实现落地后，产物树必然多出
    // 成对的 loader/wasm 文件，本用例应升级为完整 manifest 断言而非直接删除。
    let temp = TempDir::new().expect("创建临时目录失败");
    let files = build_and_list(temp.path(), &temp.path().join("dist"), "/");
    assert_eq!(files, vec!["index.html".to_string()], "当前静态产物树应只有 index.html，实际：{files:?}");
}

#[test]
fn static_page_is_self_contained_no_missing_references() {
    // entry 引用一致性：页面内不得出现指向不存在文件的 `src`/`href` 引用；
    // 当前实现是纯内联样式与正文，因此也不应出现任何外链属性。
    let temp = TempDir::new().expect("创建临时目录失败");
    build_and_list(temp.path(), &temp.path().join("dist"), "/");
    let html = fs::read_to_string(temp.path().join("dist/index.html")).expect("读取 index.html 失败");
    assert!(html.contains("<title>产物树测试</title>"), "标题应写入页面:\n{html}");
    assert!(html.contains("静态产物树"), "根组件初始状态应写入页面:\n{html}");
    assert!(!html.contains("src="), "当前页面不应携带任何 src 引用:\n{html}");
    assert!(!html.contains("href="), "根部署页面不应携带任何 href 引用:\n{html}");
}

fn obj(fields: Vec<(&str, ConfigValue)>) -> ConfigValue {
    ConfigValue::Object(fields.into_iter().map(|(name, value)| (name.to_owned(), value)).collect())
}

fn text(value: &str) -> ConfigValue {
    ConfigValue::Text(value.to_owned())
}

#[test]
fn resolved_base_is_consumed_by_static_build() {
    // 严格模型把 `artifact.base` 规范化为目录基址后，静态构建必须把它写入页面
    // `<base href>`，支持子目录部署；`chunks` 仍只完成配置形状校验，成对 JS/Wasm
    // 产物由交互运行时落地后再升级断言。
    let temp = TempDir::new().expect("创建临时目录失败");
    // 严格模型校验入口存在性；下面 build_and_list 会再写同一入口（幂等）。
    fs::create_dir_all(temp.path().join("source")).expect("创建 source/ 目录失败");
    fs::write(temp.path().join("source/index.w"), FIXTURE).expect("写入入口文件失败");
    let value = obj(vec![(
        "artifact",
        obj(vec![
            ("entry", text("source/index.w")),
            ("base", text("/app/")),
            (
                "chunks",
                obj(vec![
                    ("boundaries", ConfigValue::List(vec![obj(vec![("component", text("Admin")), ("preload", ConfigValue::Bool(true))])])),
                    ("budgets", obj(vec![("entry_bytes", ConfigValue::Int(1024)), ("chunk_bytes", ConfigValue::Int(512))])),
                ]),
            ),
        ]),
    )]);
    let origin = ConfigOrigin { config_path: temp.path().join("asgard.config.v"), project_root: temp.path().to_path_buf() };
    let config = resolve_static_config(value, origin).expect("base/chunks 配置必须被严格模型接受");
    assert_eq!(config.base, "/app");
    assert_eq!(config.chunks.boundaries.len(), 1);

    let files = build_and_list(temp.path(), &temp.path().join("dist"), &config.base);
    assert_eq!(files, vec!["index.html".to_string()], "chunks 配置不应改变当前产物树，实际：{files:?}");
    let html = fs::read_to_string(temp.path().join("dist/index.html")).expect("读取 index.html 失败");
    assert!(html.contains("<base href=\"/app/\">"), "构建必须消费规范化基址并写入 `<base href>`:\n{html}");
}
