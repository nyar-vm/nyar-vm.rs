//! 宿主绑定生成的共享调度层。
//!
//! 按 `HostProjectionBoundary` 把绑定生成请求分发到 `wasm_js_glue` 或 `wasi` 子树各自的
//! 绑定生成器。本模块不承载 Node launcher 或 WASI 运行时语义，仅做路由。

use std::path::Path;

use miette::Result;
use nyar::{BinaryTarget, HostProjectionBoundary, packaging::ArtifactDescriptor};

use crate::nyar_backend_wasi::WasiPreview;

/// 宿主绑定生成阶段共享的输入上下文。
pub(crate) struct BindingGenerationContext<'a> {
    /// 逻辑产物名。
    pub artifact_name: &'a str,
    /// 输出目录。
    pub output_dir: &'a Path,
    /// 面向的目标平台（family/arch/flavor）。
    pub target: &'a BinaryTarget,
    /// `WASM` 模块声明的导入。
    pub imports: &'a [(String, String)],
    /// WASI package-train selection (`wasip2` / `wasip3`); ignored for JS glue.
    pub wasi_preview: WasiPreview,
}

/// 宿主绑定生成器的最小接口。
pub(crate) trait HostBindingBuilder {
    /// 生成宿主绑定产物。
    fn build(&self, context: &BindingGenerationContext<'_>) -> Result<Vec<ArtifactDescriptor>>;
}

/// 根据宿主边界选择对应的绑定生成器。
pub(crate) fn generate_host_binding_artifacts(
    boundary: HostProjectionBoundary,
    context: BindingGenerationContext<'_>,
) -> Result<Vec<ArtifactDescriptor>> {
    match boundary {
        HostProjectionBoundary::WasmJsGlue => crate::nyar_backend_wasm_js_glue::JsGlueBindingBuilder.build(&context),
        HostProjectionBoundary::WasiComponent => crate::nyar_backend_wasi::WitBindingBuilder.build(&context),
        other => Err(miette::miette!("`WASM` 绑定生成不支持宿主边界 {:?}", other)),
    }
}
