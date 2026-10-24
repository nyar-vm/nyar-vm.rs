//! 编译期宏注册表。

use crate::types::{Identifier, hir::HirBlock};
use std::collections::BTreeMap;

/// 已注册的宏定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroDefinition {
    /// 宏名称。
    pub name: Identifier,
    /// 宏展开体。
    pub body: HirBlock,
    /// 泛型形参个数。
    pub generic_arity: usize,
}

/// 宏注册与展开入口。
#[derive(Debug, Default)]
pub struct MacroRegistry {
    macros: BTreeMap<String, MacroDefinition>,
}

impl MacroRegistry {
    /// 注册宏定义。
    pub fn register(&mut self, definition: MacroDefinition) {
        self.macros.insert(definition.name.to_string(), definition);
    }

    /// 按名称查找宏定义。
    pub fn lookup(&self, name: &str) -> Option<&MacroDefinition> {
        self.macros.get(name)
    }

    /// 已注册宏的数量。
    pub fn len(&self) -> usize {
        self.macros.len()
    }
}
