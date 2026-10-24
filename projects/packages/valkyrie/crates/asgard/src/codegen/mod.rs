//! 代码生成：WASM 渲染（V）、JS 胶水、CSS、移动端/桌面 UI 二进制。

pub mod android_compose_pack;
pub mod android_compose_runtime;
pub mod android_shell;
pub mod css;
pub mod desktop_linux_bridge;
pub mod desktop_native_runtime;
pub mod desktop_win_bridge;
pub mod expr_eval;
pub mod html_render;
pub mod ios_bridge;
pub mod ios_swiftui_runtime;
pub mod island_package;
pub mod js_boot;
pub mod js_glue;
pub mod mobile_prelude;
pub mod mobile_ui_binary;
pub mod mp_boot;
pub mod mp_glue;
pub mod mp_ir_runtime;
pub mod mp_prelude;
pub mod mp_render;
pub mod mp_style;
pub mod reactive_prelude;
pub mod section_framing;
pub mod tailwind;
pub mod terminal_prelude;
pub mod terminal_render;
pub mod terminal_v_render;
pub mod ui_host_abi;
pub mod v_bundle;
pub mod v_prelude;
pub mod v_render;
pub mod v_synth;

pub use android_compose_pack::{
    AndroidComposePackMode, COMPOSE_SHELL_DEX, load_compose_shell_dex, resolve_android_compose_dex, resolve_android_compose_dex_with_mode,
    validate_compose_shell_dex, write_android_compose_kotlin_sources, write_vendor_dex_bootstrap,
};
pub use android_compose_runtime::generate_android_compose_runtime;
pub use android_shell::emit_android_shell_dex;
pub use css::{CssOutput, extract_and_merge_styles};
pub use desktop_linux_bridge::generate_desktop_linux_bridge;
pub use desktop_native_runtime::{desktop_runtime_asset_name, generate_desktop_native_runtime};
pub use desktop_win_bridge::generate_desktop_win_bridge;
pub use html_render::{StaticRenderResult, render_static_ir};
pub use ios_bridge::generate_ios_bridge;
pub use ios_swiftui_runtime::{SwiftUiProfile, generate_ios_swiftui_runtime, generate_swiftui_runtime};
pub use island_package::{IslandPackageOptions, IslandPackageReport, package_browser_islands};
pub use js_boot::{asgard_boot_script_tag, asgard_boot_stylesheet_tag, generate_boot_script, manifest_url_for_mode};
pub use js_glue::{JsGlueOutput, generate_component_glue};
pub use mobile_prelude::MOBILE_HOST_PRELUDE;
pub use mobile_ui_binary::{
    HOST_NATIVE_MAGIC, UI_BIN_MAGIC, build_integrated_host_product, embed_asgard_ui_section, embed_host_native_section,
    encode_mobile_ui_package, find_asgard_native_section, find_asgard_ui_section,
};
#[cfg(test)]
pub use mobile_ui_binary::{dex_placeholder, dex_with_embedded_ui, host_executable_with_embedded_ui, host_logic_placeholder};
pub use mp_boot::{MpBootOutput, generate_mp_boot};
pub use mp_glue::{MpGlueOutput, generate_page_glue};
pub use mp_ir_runtime::generate_mp_runtime;
pub use mp_render::{MpWxmlOutput, generate_page_wxml};
pub use mp_style::{MpWxssOutput, generate_page_wxss};
pub use section_framing::magic_bytes_literal;
pub use tailwind::{TailwindBuildOutput, append_tailwind_css, maybe_build_tailwind_css};
pub use ui_host_abi::{resolve_call_export, resolve_sig_export};
pub use v_bundle::{build_awsl_host_source, build_awsl_mp_source, build_awsl_terminal_source, build_awsl_wasm_source};
pub use v_render::export_name_for_route;
