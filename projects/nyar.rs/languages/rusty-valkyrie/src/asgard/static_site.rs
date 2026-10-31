//! 静态站点构建编排。
//!
//! 职责：把既有所有权层串成一次可被 CLI / 测试调用的静态站点构建——
//! 显式入口校验（`.w` 与 `.awsl` 同格式同语义，两种后缀同等，不做后缀猜测，
//! 入口必须显式给出且位于项目根内）、AWSL 解析与降级、DOM 静态渲染
//! （以组件初始状态渲染，外部数据为 `Null`）、完整页面组装、产物写出 `index.html`。
//! 本模块不做表达式求值，也不复制解析、降级或渲染逻辑；
//! 各环节的唯一事实所有者分别是 `awsl::compile`、`codegen::html_render` 与 `ssg`。

use std::path::PathBuf;

use crate::asgard::{
    awsl::{LoweringOptions, compile_awsl_source},
    codegen::html_render::render_static_ir,
    error::{AsgardError, AsgardResult},
    ssg::generate_static_page,
};

/// 静态站点构建选项。
#[derive(Debug, Clone)]
pub struct StaticSiteOptions {
    /// 项目根目录。
    pub project_dir: PathBuf,
    /// 入口根组件文件路径（相对 `project_dir`，如 `source/index.w`）。
    /// `.w` 与 `.awsl` 同格式同语义，须显式给出，不做后缀猜测。
    pub entry: PathBuf,
    /// 输出目录；缺省为 `project_dir/dist`。
    pub output_dir: Option<PathBuf>,
    /// 页面 `<title>` 文本；`None` 时使用根组件名（入口文件 stem）。
    pub title: Option<String>,
}

/// 静态站点构建成功报告。
#[derive(Debug, Clone)]
pub struct StaticSiteReport {
    /// 实际写出产物的目录。
    pub output_dir: PathBuf,
    /// 产物 `index.html` 的完整路径。
    pub page_path: PathBuf,
    /// 根组件名（来自入口文件 stem，widget 名与路由名的唯一来源）。
    pub component_name: String,
}

/// 构建静态站点：入口校验 → AWSL 降级 → DOM 静态渲染 → 页面组装 → 写出 `index.html`。
pub fn build_static_site(options: &StaticSiteOptions) -> AsgardResult<StaticSiteReport> {
    let extension = options.entry.extension().and_then(|ext| ext.to_str()).unwrap_or_default();
    if !matches!(extension, "w" | "awsl") {
        return Err(AsgardError::msg(report!(
            "静态站点入口必须是 `.w` 或 `.awsl` 文件（两种后缀同格式同语义，不做后缀猜测）: {}",
            options.entry.display()
        )));
    }

    // 入口合同正式前置校验：项目根包含性以文件系统规范化（`canonicalize` 解析 `..` 段与
    // 符号链接）判定，先于读取入口内容失败闭合。词法 `starts_with` 在 Unix 上不折叠 `..`，
    // 不能充当包含性证据。
    let project_root = std::path::absolute(&options.project_dir)
        .map_err(|error| AsgardError::msg(report!("规范化项目根失败 {}: {error}", options.project_dir.display())))?;
    let entry_path = std::path::absolute(project_root.join(&options.entry))
        .map_err(|error| AsgardError::msg(report!("规范化入口路径失败 {}: {error}", options.entry.display())))?;
    let root_canonical =
        std::fs::canonicalize(&project_root).map_err(|error| AsgardError::msg(report!("项目根不可用 {}: {error}", project_root.display())))?;
    // 入口可能尚不存在（缺失文件应报读取失败并带路径证据）：以父目录规范化结果重建
    // 判定基准；父目录也不存在时无从判定包含性，同样由后续读取失败闭合。
    let entry_canonical = std::fs::canonicalize(&entry_path).ok().or_else(|| {
        let parent = entry_path.parent()?;
        let name = entry_path.file_name()?;
        Some(std::fs::canonicalize(parent).ok()?.join(name))
    });
    if let Some(resolved) = &entry_canonical {
        if !resolved.starts_with(&root_canonical) {
            return Err(AsgardError::msg(report!("入口越出项目根: {} 不在 {} 内", resolved.display(), project_root.display())));
        }
    }

    let file_stem = entry_path
        .file_stem()
        .and_then(|stem| stem.to_str())
        .ok_or_else(|| AsgardError::msg(report!("入口文件名无法提取组件名: {}", entry_path.display())))?;
    let relative_path = entry_path.strip_prefix(&project_root).unwrap_or(&entry_path).to_string_lossy().replace('\\', "/");

    let source = std::fs::read_to_string(&entry_path)
        .map_err(|error| AsgardError::msg(report!("读取入口文件失败 {}: {error}", entry_path.display())))?;
    let component = compile_awsl_source(&source, file_stem, &relative_path, &LoweringOptions::default())
        .map_err(|error| AsgardError::msg(report!("{}:{}: {}", entry_path.display(), error.span.start, error.message)))?;

    // 静态页面以组件初始状态渲染，外部数据为 Null；标题缺省沿用根组件名这一唯一来源。
    let rendered = render_static_ir(&component, &serde_json::Value::Null);
    let title = options.title.as_deref().unwrap_or(file_stem);
    let page = generate_static_page(title, std::slice::from_ref(&rendered), "");

    let output_dir = options.output_dir.clone().unwrap_or_else(|| project_root.join("dist"));
    std::fs::create_dir_all(&output_dir).map_err(|error| AsgardError::msg(report!("创建输出目录失败 {}: {error}", output_dir.display())))?;
    let page_path = output_dir.join("index.html");
    std::fs::write(&page_path, page).map_err(|error| AsgardError::msg(report!("写入页面失败 {}: {error}", page_path.display())))?;

    Ok(StaticSiteReport { output_dir, page_path, component_name: file_stem.to_string() })
}
