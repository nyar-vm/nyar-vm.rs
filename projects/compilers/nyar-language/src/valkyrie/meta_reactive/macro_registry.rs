//! 编译期宏注册表。

use crate::types::{Identifier, hir::HirBlock};
use std::collections::BTreeMap;

/// 已注册的宏定义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MacroDefinition {
    pub name: Identifier,
    pub body: HirBlock,
    pub generic_arity: usize,
}

/// 宏注册与展开入口。
#[derive(Debug, Default)]
pub struct MacroRegistry {
    macros: BTreeMap<String, MacroDefinition>,
}

impl MacroRegistry {
    pub fn register(&mut self, definition: MacroDefinition) {
        self.macros.insert(definition.name.to_string(), definition);
    }

    pub fn lookup(&self, name: &str) -> Option<&MacroDefinition> {
        self.macros.get(name)
    }

    pub fn len(&self) -> usize {
        self.macros.len()
    }
}
