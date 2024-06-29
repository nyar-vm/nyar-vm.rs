//! 编译器内部符号工具。
use crate::types::{Identifier, NamePath, QualifiedName, hir::HirFunction};

/// Stable callable / contract lookup key (`demo::write`, `std.io::main`).
///
/// Structured [`NamePath`] segments carry module + name; this is the only
/// production encoding for host contracts and callable identity tables.
/// Does not normalize `::` and `.` against each other.
pub(crate) fn stable_symbol_key(path: &NamePath) -> String {
    let parts = path.parts();
    match parts {
        [] => String::new(),
        [single] => single.as_str().to_string(),
        _ => {
            let name = parts[parts.len() - 1].as_str();
            let module = parts[..parts.len() - 1].iter().map(|part| part.as_str()).collect::<Vec<_>>().join(".");
            if module.is_empty() {
                name.to_string()
            } else {
                format!("{module}::{name}")
            }
        }
    }
}

/// 生成 `HIR` 顶层函数的稳定限定名。
pub(crate) fn stable_function_symbol(module_name: &NamePath, function_name: &Identifier) -> String {
    let module = module_name.to_string();
    if module.is_empty() { function_name.to_string() } else { format!("{module}::{function_name}") }
}

/// 生成 `HIR` 顶层函数的稳定限定名。
#[doc(hidden)]
pub fn stable_hir_function_symbol(module_name: &NamePath, function: &HirFunction) -> String {
    stable_symbol_key(&stable_hir_function_name_path(module_name, function))
}

/// Structured overload / MIR symbol path for a top-level `HIR` function.
///
/// Module and name are separate [`NamePath`] segments; use [`stable_symbol_key`]
/// or [`stable_hir_function_symbol`] for the `module::name` contract string.
#[doc(hidden)]
pub fn stable_hir_function_name_path(module_name: &NamePath, function: &HirFunction) -> NamePath {
    if !function.declaring_namespace.parts().is_empty() {
        let mut parts = function.declaring_namespace.parts().to_vec();
        parts.push(function.name.clone());
        return NamePath::new(parts);
    }
    if module_name.parts().is_empty() {
        return NamePath::new(vec![function.name.clone()]);
    }
    let mut parts = module_name.parts().to_vec();
    parts.push(function.name.clone());
    NamePath::new(parts)
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
