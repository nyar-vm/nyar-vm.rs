//! 编译器内部符号工具。
use crate::types::{Identifier, NamePath, QualifiedName, hir::HirFunction};

/// 生成 `HIR` 顶层函数的稳定限定名。
pub(crate) fn stable_function_symbol(module_name: &NamePath, function_name: &Identifier) -> String {
    let module = module_name.to_string();
    if module.is_empty() { function_name.to_string() } else { format!("{module}::{function_name}") }
}

/// 生成 `HIR` 顶层函数的稳定限定名。
pub(crate) fn stable_hir_function_symbol(module_name: &NamePath, function: &HirFunction) -> String {
    if !function.declaring_namespace.parts().is_empty() {
        let mut parts = function.declaring_namespace.parts().to_vec();
        parts.push(function.name.clone());
        return QualifiedName::new(parts).to_string();
    }
    stable_function_symbol(module_name, &function.name)
}

/// 将稳定符号名转换为后端可发射的扁平名字。
///
/// 这里保留 ASCII 字母、数字与下划线，其余字符编码为 `_xNN_` / `_uNNNN_`
/// 形式，避免不同逻辑符号在 emit 阶段被压扁成同名条目。
pub(crate) fn mangle_emitted_symbol(symbol: &str) -> String {
    let mut emitted = String::with_capacity(symbol.len());
    for ch in symbol.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            emitted.push(ch);
        }
        else if (ch as u32) <= 0xff {
            emitted.push_str(&format!("_x{:02x}_", ch as u32));
        }
        else {
            emitted.push_str(&format!("_u{:04x}_", ch as u32));
        }
    }

    if emitted.is_empty() {
        return "_".to_string();
    }

    if emitted.chars().next().is_some_and(|ch| ch.is_ascii_digit()) {
        emitted.insert(0, '_');
    }

    emitted
}
