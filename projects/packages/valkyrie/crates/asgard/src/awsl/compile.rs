//! AWSL 单文件组件统一编译入口。

use oak_awsl::{AwslParseError, AwslParser, validate_component_contract};

use super::{LoweredComponent, LoweringOptions, lower_component};

/// 解析、校验并降级单个 `.awsl` 文件为 `LoweredComponent`。
///
/// 契约：一文件对应一个 widget；`file_stem` 为路由名与 widget 名的唯一来源。
pub fn compile_awsl_source(
    source: &str,
    file_stem: &str,
    source_path: &str,
    options: &LoweringOptions,
) -> Result<LoweredComponent, AwslParseError> {
    let root = AwslParser::parse_root_with_options(source, options.strict_mode)?;
    validate_component_contract(&root, file_stem)?;
    Ok(lower_component(&root, file_stem, source_path, options))
}
