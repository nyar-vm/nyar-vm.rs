//! 微信小程序 `app.js` / `Page` 启动壳（WASM 逻辑 + WXML setData）。

use std::fmt::Write as _;

use crate::{awsl::LoweredComponent, codegen::v_render::export_name_for_route};

/// 小程序启动脚本输出。
#[derive(Debug, Clone)]
pub struct MpBootOutput {
    /// `app.js` 内容。
    pub app_js: String,
    /// 页面 `Page({})` JS 内容（按路由）。
    pub page_js: String,
    /// 页面相对路径。
    pub page_relative_path: String,
}

/// 生成 `app.js` 与单页 `Page` 壳。
pub fn generate_mp_boot(components: &[LoweredComponent], host_stem: &str, event_forwarders: &str) -> MpBootOutput {
    let mut app_js = String::new();
    writeln!(app_js, "App({{").unwrap();
    writeln!(app_js, "  onLaunch() {{").unwrap();
    writeln!(app_js, "    console.log('asgard miniprogram boot');").unwrap();
    writeln!(app_js, "  }}").unwrap();
    writeln!(app_js, "}});").unwrap();

    let Some(component) = components.first()
    else {
        return MpBootOutput { app_js, page_js: String::new(), page_relative_path: "pages/index/index.js".into() };
    };
    let route = &component.route_name;
    let export_name = export_name_for_route(route);
    let mut data_fields = String::new();
    for binding in component.script_bindings.iter().filter(|b| b.reactive) {
        writeln!(data_fields, "    {}: {},", binding.name, binding.init_expr).unwrap();
    }

    let mut page_js = String::new();
    writeln!(page_js, "const asgard = require('../../asgard-runtime.js');").unwrap();
    writeln!(page_js, "const HOST_MODULE = '{host_stem}';").unwrap();
    writeln!(page_js).unwrap();
    writeln!(page_js, "Page({{").unwrap();
    writeln!(page_js, "  data: {{").unwrap();
    page_js.push_str(&data_fields);
    writeln!(page_js, "  }},").unwrap();
    writeln!(page_js, "  async onLoad() {{").unwrap();
    writeln!(page_js, "    await asgard.loadProduct();").unwrap();
    writeln!(page_js, "    asgard.bindPage(this, '{export_name}');").unwrap();
    writeln!(page_js, "  }},").unwrap();
    page_js.push_str(event_forwarders);
    writeln!(page_js, "}});").unwrap();

    MpBootOutput { app_js, page_js, page_relative_path: format!("pages/{route}/{route}.js") }
}
