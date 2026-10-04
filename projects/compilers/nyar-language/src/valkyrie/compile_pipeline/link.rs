//! Semantic MIR 的实例身份链接边界。
//!
//! 链接器只接受前端已经冻结的 ItemInstanceId 闭包，不从名称或布局补造身份。

use crate::valkyrie::mir::MirModule;

/// 合并已由 Compiler 统一注册的依赖实例闭包。
pub(crate) fn link_reachable_dependency_mir(
    _consumer: &mut MirModule,
    dependency_mirs: &[MirModule],
) -> Result<(), std_data::text::valkyrie::ParseError> {
    if dependency_mirs.is_empty() {
        return Ok(());
    }
    Err(std_data::text::valkyrie::ParseError::invalid(
        "依赖链接要求 Compiler 提供统一 ItemInstanceId 闭包；名称链接路径已删除",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ValkyrieCompiler, mir::MirLowerer};

    #[test]
    fn dependency_linking_without_shared_instances_fails_without_mutation() {
        let compiler = ValkyrieCompiler::default();
        let consumer_hir = compiler.compile_source("micro consumer() -> unit { return }").expect("源码解析");
        let dependency_hir = compiler.compile_source("micro helper() -> unit { return }").expect("依赖源码解析");
        let mut consumer = MirLowerer::lower_module_semantic(&consumer_hir);
        let dependency = MirLowerer::lower_module_semantic(&dependency_hir);
        let original = consumer.clone();
        let error = link_reachable_dependency_mir(&mut consumer, &[dependency]).expect_err("缺少统一实例表不能链接");
        assert!(error.to_string().contains("ItemInstanceId"));
        assert_eq!(consumer, original);
    }
}
