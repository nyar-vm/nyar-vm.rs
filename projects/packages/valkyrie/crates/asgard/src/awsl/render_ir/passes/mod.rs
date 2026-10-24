//! Canonical RenderIR optimization passes.

mod drop_empty_text;
mod fold_static_attrs;
mod merge_static_text;
mod theme_registry;

use super::canonical::RenderModule;

pub use theme_registry::{ThemeRegistryOptions, theme_registry_pass};

/// A canonical RenderIR transform pass.
pub trait RenderPass {
    /// pass 名称，用于调试与日志。
    fn name(&self) -> &'static str;
    /// 就地改写 canonical RenderIR。
    fn run(&self, module: &mut RenderModule);
}

/// Run default optimization passes on canonical RenderIR.
pub fn run_default_passes(module: &mut RenderModule, theme: ThemeRegistryOptions<'_>) {
    let passes: [&dyn RenderPass; 3] =
        [&merge_static_text::MergeAdjacentStaticText, &drop_empty_text::DropEmptyText, &fold_static_attrs::FoldStaticAttrs];
    for pass in passes {
        pass.run(module);
    }
    theme_registry_pass(module, theme);
}
#[cfg(test)]
#[path = "../../../../tests/path_wired/awsl_render_ir_passes_mod.rs"]
mod awsl_render_ir_passes_mod_tests;
