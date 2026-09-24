//! `[export(..)]` metadata on HIR items.

use super::{HirArgument, HirAttribute, HirExpr, HirExprKind, HirLiteral, HirStringSegment};
use crate::types::{Identifier, NamePath};

/// Export partition and optional rename for CLR / wasm / Nyar module surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirExportSpec {
    /// Target export partitions. Empty means `default` at use sites.
    pub partitions: Vec<String>,
    /// Optional exported symbol name override (`name: "twoSum"`).
    pub export_name: Option<String>,
    /// Optional casing transform (`case: "camelCase"`).
    pub export_case: Option<String>,
}

impl HirExportSpec {
    /// Primary partition for artifact routing.
    pub fn primary_partition(&self) -> String {
        self.partitions.first().cloned().unwrap_or_else(|| "default".to_string())
    }

    /// Resolve the wasm / host export symbol for a local `micro` name.
    pub fn resolve_exported_name(&self, local_name: &Identifier) -> String {
        if let Some(name) = &self.export_name {
            return name.clone();
        }
        match self.export_case.as_deref() {
            Some("camelCase") => snake_case_to_camel_case(local_name.as_str()),
            Some("snake_case") => local_name.as_str().to_string(),
            Some(other) => other.to_string(),
            None => local_name.as_str().to_string(),
        }
    }
}

/// Parse `[export]` / `[export(unity.runtime)]` / `[export(name: "twoSum")]` / `[export(case: "camelCase")]`.
pub fn parse_export_spec_from_annotations(annotations: &[HirAttribute]) -> Option<HirExportSpec> {
    let attribute = annotations.iter().find(|attribute| attribute.name.parts().last().is_some_and(|name| name.as_str() == "export"))?;

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
            } else if key == "case" {
                export_case = argument_string_literal(argument);
            } else {
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
        let HirStringSegment::Text(text) = segment else {
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
                let HirStringSegment::Text(text) = segment else {
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

/// `two_sum` → `twoSum`（LeetCode `metadata.invoke` 常用 camelCase）。
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
        } else {
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
}
