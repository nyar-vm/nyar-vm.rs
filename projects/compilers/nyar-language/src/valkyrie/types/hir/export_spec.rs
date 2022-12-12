//! `[export(..)]` metadata on HIR items.

use super::{HirAttribute, HirExpr, HirExprKind, HirLiteral, HirStringSegment};
use crate::types::NamePath;

/// Export partition and optional rename for CLR / Nyar module surfaces.
#[derive(Debug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct HirExportSpec {
    /// Target export partitions. Empty means `default` at use sites.
    pub partitions: Vec<String>,
    /// Optional exported symbol name override.
    pub export_name: Option<String>,
}

impl HirExportSpec {
    /// Primary partition for artifact routing.
    pub fn primary_partition(&self) -> String {
        self.partitions.first().cloned().unwrap_or_else(|| "default".to_string())
    }
}

/// Parse `[export]` / `[export(unity.runtime)]` / `[export("unity.editor")]` from HIR attributes.
pub fn parse_export_spec_from_annotations(annotations: &[HirAttribute]) -> Option<HirExportSpec> {
    let attribute = annotations.iter().find(|attribute| attribute.name.parts().last().is_some_and(|name| name.as_str() == "export"))?;

    let partition = if attribute.arguments.is_empty() {
        "default".to_string()
    }
    else {
        export_arg_to_partition(&attribute.arguments[0].value).unwrap_or_else(|| "default".to_string())
    };

    Some(HirExportSpec { partitions: vec![partition], export_name: None })
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
