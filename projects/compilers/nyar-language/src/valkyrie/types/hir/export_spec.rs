//! `[export(..)]` 注解在 HIR 项上的元数据。

use super::{HirArgument, HirAttribute, HirExpr, HirExprKind, HirLiteral, HirStringLiteral, HirStringSegment};
use crate::types::{Identifier, NamePath, SourceSpan};
use nyar_types::{AttributeId, AttributeRegistry, builtin_attribute};
use std::sync::{Mutex, OnceLock};

/// CLR / wasm / Nyar 模块表面上的导出分区与可选重命名。
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirExportSpec {
    /// 目标导出分区。空表示使用点上的 `default`。
    pub partitions: Vec<String>,
    /// 可选的导出名覆盖（`name: "twoSum"`）。
    pub export_name: Option<String>,
    /// 可选的大小写变换（`case: "camelCase"`）。
    pub export_case: Option<String>,
}

impl HirExportSpec {
    /// 产物路由用的主分区。
    pub fn primary_partition(&self) -> String {
        self.partitions.first().cloned().unwrap_or_else(|| "default".to_string())
    }

    /// 由本地 `micro` 名解析 wasm / 宿主导出符号。
    ///
    /// 非法 `case` 不得被当成导出名；未知 case 回退为本地名（诊断由调用方负责）。
    pub fn resolve_exported_name(&self, local_name: &Identifier) -> String {
        if let Some(name) = &self.export_name {
            return name.clone();
        }
        match self.export_case.as_deref() {
            Some("camelCase") => snake_case_to_camel_case(local_name.as_str()),
            Some("snake_case") | None => local_name.as_str().to_string(),
            Some(_) => local_name.as_str().to_string(),
        }
    }
}

/// 解析阶段共享的属性注册表（内建播种 + 用户属性 intern）。
///
/// 完整编译会话上下文接入前，用进程内单例保证同名 → 同 id；重复键失败关闭由
/// [`AttributeRegistry::intern_unique`] 提供，此处 `intern` 对同名幂等。
fn attribute_registry() -> &'static Mutex<AttributeRegistry> {
    static REGISTRY: OnceLock<Mutex<AttributeRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| Mutex::new(AttributeRegistry::with_builtins()))
}

/// 将属性简单名解析为 [`AttributeId`]（解析边界；之后只比较 id）。
///
/// 内建属性走播种槽；用户属性由 [`AttributeRegistry::intern`] 分配。
pub fn resolve_attribute_id(attribute: &HirAttribute) -> Option<AttributeId> {
    let name = attribute.name.parts().last()?.as_str();
    let Ok(mut registry) = attribute_registry().lock()
    else {
        return None;
    };
    Some(registry.intern(name))
}

/// 解析 `[workload_phase("request")]` / `[workload_phase(name: "request")]` 的阶段名。
///
/// 空名或缺失字符串字面量时返回 `None`（失败闭合，不发明默认阶段）。
pub fn parse_workload_phase_from_annotations(annotations: &[HirAttribute]) -> Option<String> {
    let attribute = annotations.iter().find(|attribute| resolve_attribute_id(attribute) == Some(builtin_attribute::workload_phase()))?;

    if attribute.arguments.is_empty() {
        return None;
    }

    for argument in &attribute.arguments {
        if let Some(key) = argument.key.as_ref() {
            if key.as_str() == "name" {
                let name = argument_string_literal(argument)?;
                return non_empty_phase_name(name);
            }
            continue;
        }
        if let Some(name) = argument_string_literal(argument) {
            return non_empty_phase_name(name);
        }
    }
    None
}

fn non_empty_phase_name(name: String) -> Option<String> {
    if name.is_empty() { None } else { Some(name) }
}

/// 解析 `[export]` / `[export(unity.runtime)]` / `[export(name: "twoSum")]` / `[export(case: "camelCase")]`。
pub fn parse_export_spec_from_annotations(annotations: &[HirAttribute]) -> Option<HirExportSpec> {
    let attribute = annotations.iter().find(|attribute| resolve_attribute_id(attribute) == Some(builtin_attribute::export()))?;

    if attribute.arguments.is_empty() {
        return Some(HirExportSpec { partitions: vec!["default".to_string()], export_name: None, export_case: None });
    }

    let mut partitions = Vec::new();
    let mut export_name = None;
    let mut export_case = None;

    for argument in &attribute.arguments {
        if let Some(key) = argument.key.as_ref() {
            let key = key.as_str();
            if key == "name" {
                export_name = argument_string_literal(argument);
            }
            else if key == "case" {
                let Some(value) = argument_string_literal(argument)
                else {
                    return None;
                };
                match value.as_str() {
                    "camelCase" | "snake_case" => export_case = Some(value),
                    // 非法 case：整条 export 合同失败闭合，禁止把拼写当导出名。
                    _ => return None,
                }
            }
            else {
                export_name.get_or_insert_with(|| key.to_string());
            }
            continue;
        }

        if let Some(partition) = export_arg_to_partition(&argument.value) {
            partitions.push(partition);
        }
    }

    if partitions.is_empty() {
        partitions.push("default".to_string());
    }

    Some(HirExportSpec { partitions, export_name, export_case })
}

fn argument_string_literal(argument: &HirArgument) -> Option<String> {
    let HirExprKind::Literal(HirLiteral::String(literal)) = &argument.value.kind
    else {
        return None;
    };

    let mut rendered = String::new();
    for segment in &literal.segments {
        let HirStringSegment::Text(text) = segment
        else {
            return None;
        };
        rendered.push_str(text);
    }
    Some(rendered)
}

fn export_arg_to_partition(expr: &HirExpr) -> Option<String> {
    match &expr.kind {
        HirExprKind::Literal(HirLiteral::String(literal)) => {
            let mut rendered = String::new();
            for segment in &literal.segments {
                let HirStringSegment::Text(text) = segment
                else {
                    return None;
                };
                rendered.push_str(text);
            }
            Some(rendered)
        }
        HirExprKind::Path(path) => Some(path_to_partition(path)),
        _ => None,
    }
}

fn path_to_partition(path: &NamePath) -> String {
    path.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>().join(".")
}

/// `two_sum` → `twoSum`（`export(case: "camelCase")` 命名变换）。
pub fn snake_case_to_camel_case(name: &str) -> String {
    let mut out = String::new();
    let mut upper_next = false;
    for ch in name.chars() {
        if ch == '_' {
            upper_next = true;
            continue;
        }
        if upper_next {
            out.extend(ch.to_uppercase());
            upper_next = false;
        }
        else {
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snake_case_to_camel_case_examples() {
        assert_eq!(snake_case_to_camel_case("two_sum"), "twoSum");
        assert_eq!(snake_case_to_camel_case("max_sub_array"), "maxSubArray");
        assert_eq!(snake_case_to_camel_case("api_ping"), "apiPing");
    }

    #[test]
    fn attribute_id_maps_export_and_main() {
        let export = HirAttribute::new(NamePath::new(vec![Identifier::new("export")]));
        let main = HirAttribute::new(NamePath::new(vec![Identifier::new("main")]));
        assert_eq!(resolve_attribute_id(&export), Some(builtin_attribute::export()));
        assert_eq!(resolve_attribute_id(&main), Some(builtin_attribute::main()));
        let custom = HirAttribute::new(NamePath::new(vec![Identifier::new("my_attr")]));
        let custom_id = resolve_attribute_id(&custom).expect("user attribute interned");
        assert_ne!(custom_id, builtin_attribute::export());
        assert_eq!(resolve_attribute_id(&custom), Some(custom_id));
    }

    #[test]
    fn workload_phase_parses_positional_and_named() {
        let positional = HirArgument {
            key: None,
            value: Box::new(HirExpr {
                kind: HirExprKind::Literal(HirLiteral::String(HirStringLiteral {
                    prefix: None,
                    quote_count: 1,
                    segments: vec![HirStringSegment::Text("request".into())],
                })),
                span: SourceSpan { source: crate::types::SourceID { version_id: 0 }, span: (0..0).into() },
            }),
        };
        let annotations = vec![HirAttribute::with_arguments(NamePath::new(vec![Identifier::new("workload_phase")]), vec![positional])];
        assert_eq!(parse_workload_phase_from_annotations(&annotations).as_deref(), Some("request"));

        let named = HirArgument {
            key: Some(Identifier::new("name")),
            value: Box::new(HirExpr {
                kind: HirExprKind::Literal(HirLiteral::String(HirStringLiteral {
                    prefix: None,
                    quote_count: 1,
                    segments: vec![HirStringSegment::Text("batch".into())],
                })),
                span: SourceSpan { source: crate::types::SourceID { version_id: 0 }, span: (0..0).into() },
            }),
        };
        let annotations = vec![HirAttribute::with_arguments(NamePath::new(vec![Identifier::new("workload_phase")]), vec![named])];
        assert_eq!(parse_workload_phase_from_annotations(&annotations).as_deref(), Some("batch"));
    }

    #[test]
    fn workload_phase_rejects_empty_name() {
        let empty = HirArgument {
            key: None,
            value: Box::new(HirExpr {
                kind: HirExprKind::Literal(HirLiteral::String(HirStringLiteral {
                    prefix: None,
                    quote_count: 1,
                    segments: vec![HirStringSegment::Text(String::new())],
                })),
                span: SourceSpan { source: crate::types::SourceID { version_id: 0 }, span: (0..0).into() },
            }),
        };
        let annotations = vec![HirAttribute::with_arguments(NamePath::new(vec![Identifier::new("workload_phase")]), vec![empty])];
        assert!(parse_workload_phase_from_annotations(&annotations).is_none());
    }

    #[test]
    fn illegal_export_case_fails_closed() {
        let case_arg = HirArgument {
            key: Some(Identifier::new("case")),
            value: Box::new(HirExpr {
                kind: HirExprKind::Literal(HirLiteral::String(HirStringLiteral {
                    prefix: None,
                    quote_count: 1,
                    segments: vec![HirStringSegment::Text("PascalCase".into())],
                })),
                span: SourceSpan { source: crate::types::SourceID { version_id: 0 }, span: (0..0).into() },
            }),
        };
        let annotations = vec![HirAttribute::with_arguments(NamePath::new(vec![Identifier::new("export")]), vec![case_arg])];
        assert!(parse_export_spec_from_annotations(&annotations).is_none());
    }
}
