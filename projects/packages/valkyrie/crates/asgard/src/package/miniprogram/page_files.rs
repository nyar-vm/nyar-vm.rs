//! 每页 `.wxml` / `.wxss` / `.js` / `.json` 发射。

use crate::{
    awsl::LoweredComponent,
    codegen::{generate_mp_boot, generate_page_glue, generate_page_wxml, generate_page_wxss},
};

/// 小程序页面打包产物。
#[derive(Debug, Clone)]
pub struct MpPageFiles {
    /// 路由名。
    pub route_name: String,
    /// 文件列表（相对路径 + 内容）。
    pub files: Vec<(String, String)>,
}

/// 小程序 dist 汇总。
#[derive(Debug, Clone)]
pub struct MpPackageOutput {
    /// 页面文件。
    pub pages: Vec<MpPageFiles>,
    /// `app.js`。
    pub app_js: String,
    /// `asgard-runtime.js`。
    pub runtime_js: String,
}

/// 为所有组件生成小程序页面文件描述。
pub fn package_miniprogram_pages(components: &[LoweredComponent], host_stem: &str) -> MpPackageOutput {
    let first = components.first();
    let glue = first.map(generate_page_glue);
    let forwarders = glue.as_ref().map(|g| g.event_forwarders.as_str()).unwrap_or("");
    let boot = generate_mp_boot(components, host_stem, forwarders);
    let runtime_js = crate::codegen::generate_mp_runtime(host_stem);
    let mut pages = Vec::new();
    for component in components {
        let wxml = generate_page_wxml(component);
        let wxss = generate_page_wxss(component);
        let component_glue = generate_page_glue(component);
        let page_js = if Some(component.route_name.as_str()) == first.map(|c| c.route_name.as_str()) {
            boot.page_js.clone()
        }
        else {
            let fwd = component_glue.event_forwarders.as_str();
            generate_mp_boot(std::slice::from_ref(component), host_stem, fwd).page_js
        };
        let mut files = vec![
            (wxml.relative_path, wxml.content),
            (format!("pages/{}/{}.json", component.route_name, component.route_name), r#"{ "usingComponents": {} }"#.into()),
            (format!("pages/{}/{}.js", component.route_name, component.route_name), page_js),
        ];
        if let Some(wxss) = wxss {
            files.push((wxss.relative_path, wxss.content));
        }
        pages.push(MpPageFiles { route_name: component.route_name.clone(), files });
    }
    MpPackageOutput { pages, app_js: boot.app_js, runtime_js }
}
