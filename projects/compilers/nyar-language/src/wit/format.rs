//! WIT 文本格式化。

use super::model::WitPackage;

pub fn format_wit_package(package: &WitPackage) -> String {
    let mut result = format!("package {};\n", package.package_name);
    for interface in &package.interfaces {
        result.push('\n');
        result.push_str(&format!("interface {} {{\n", interface.name));
        for function in &interface.functions {
            result.push_str("  ");
            result.push_str(function.trim());
            result.push_str(";\n");
        }
        result.push_str("}\n");
    }
    result
}
