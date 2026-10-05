//! 可扩展属性 / 运算符注册表。
//!
//! 解析后语义路径只持有 [`AttributeId`] / [`OperatorId`]；名称与词素仅存旁表。

use std::collections::BTreeMap;

use crate::semantic_ids::{AttributeId, AttributeRegistration, OperatorFixity, OperatorId, OperatorRegistration, builtin_attribute};

/// 属性注册表错误（失败关闭，禁止静默覆盖）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AttributeRegistryError {
    /// 同名属性已绑定不同 id。
    DuplicateName {
        /// 冲突的属性名。
        name: String,
        /// 已有 id。
        existing: AttributeId,
    },
}

impl std::fmt::Display for AttributeRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateName { name, existing } => {
                write!(f, "duplicate attribute name `{name}` already bound to {existing}")
            }
        }
    }
}

impl std::error::Error for AttributeRegistryError {}

/// 可扩展属性注册表：内建播种后可 `intern` 用户属性。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct AttributeRegistry {
    by_name: BTreeMap<String, AttributeId>,
    by_id: BTreeMap<AttributeId, AttributeRegistration>,
    next_index: u32,
}

impl AttributeRegistry {
    /// 播种内建 `export` / `main` / `test` / `benchmark` / `workload_phase`。
    pub fn with_builtins() -> Self {
        let mut registry = Self::default();
        for row in builtin_attribute::seed_registrations() {
            registry.by_name.insert(row.name.clone(), row.id);
            let index = row.id.index();
            registry.next_index = registry.next_index.max(index.saturating_add(1));
            registry.by_id.insert(row.id, row);
        }
        registry
    }

    /// 按名查找（含用户 intern 项）。
    pub fn lookup(&self, name: &str) -> Option<AttributeId> {
        self.by_name.get(name).copied()
    }

    /// 登记或复用属性名；同名已存在则返回已有 id（幂等）。
    ///
    /// 若要以「同名必须失败」策略接入，改用 [`AttributeRegistry::intern_unique`]。
    pub fn intern(&mut self, name: impl Into<String>) -> AttributeId {
        let name = name.into();
        if let Some(id) = self.by_name.get(&name).copied() {
            return id;
        }
        let id = AttributeId::from_index(self.next_index).expect("attribute id space");
        self.next_index = self.next_index.saturating_add(1);
        self.by_name.insert(name.clone(), id);
        self.by_id.insert(id, AttributeRegistration { id, name });
        id
    }

    /// 登记新属性名；若名已存在则失败关闭。
    pub fn intern_unique(&mut self, name: impl Into<String>) -> Result<AttributeId, AttributeRegistryError> {
        let name = name.into();
        if let Some(existing) = self.by_name.get(&name).copied() {
            return Err(AttributeRegistryError::DuplicateName { name, existing });
        }
        Ok(self.intern(name))
    }

    /// 旁表查询。
    pub fn registration(&self, id: AttributeId) -> Option<&AttributeRegistration> {
        self.by_id.get(&id)
    }
}

/// 运算符注册表错误。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperatorRegistryError {
    /// 同 `(fixity, lexeme)` 已绑定。
    DuplicateKey {
        /// 结合性。
        fixity: OperatorFixity,
        /// 词素。
        lexeme: String,
        /// 已有 id。
        existing: OperatorId,
    },
}

impl std::fmt::Display for OperatorRegistryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DuplicateKey { fixity, lexeme, existing } => {
                write!(f, "duplicate operator {fixity:?} `{lexeme}` already bound to {existing}")
            }
        }
    }
}

impl std::error::Error for OperatorRegistryError {}

/// 可扩展运算符注册表（Haskell/Scala 式用户运算符可继续 intern）。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OperatorRegistry {
    by_key: BTreeMap<(u8, String), OperatorId>,
    by_id: BTreeMap<OperatorId, OperatorRegistration>,
    next_index: u32,
}

fn fixity_key(fixity: OperatorFixity) -> u8 {
    match fixity {
        OperatorFixity::Prefix => 0,
        OperatorFixity::Infix => 1,
        OperatorFixity::Postfix => 2,
    }
}

impl OperatorRegistry {
    /// 播种语言前端常见内建运算符（槽位稳定；不是封闭语义集合）。
    pub fn with_builtins() -> Self {
        let mut registry = Self::default();
        let seeds: &[(OperatorFixity, &str, u16)] = &[
            (OperatorFixity::Prefix, "!", 0),
            (OperatorFixity::Prefix, "-", 0),
            (OperatorFixity::Prefix, "+", 0),
            (OperatorFixity::Infix, "+", 6),
            (OperatorFixity::Infix, "-", 6),
            (OperatorFixity::Infix, "*", 7),
            (OperatorFixity::Infix, "/", 7),
            (OperatorFixity::Infix, "%", 7),
            (OperatorFixity::Infix, "==", 4),
            (OperatorFixity::Infix, "!=", 4),
            (OperatorFixity::Infix, "<", 4),
            (OperatorFixity::Infix, "<=", 4),
            (OperatorFixity::Infix, ">", 4),
            (OperatorFixity::Infix, ">=", 4),
            (OperatorFixity::Infix, "&&", 2),
            (OperatorFixity::Infix, "||", 1),
            (OperatorFixity::Infix, "<<", 5),
            (OperatorFixity::Infix, ">>", 5),
            (OperatorFixity::Infix, "&", 5),
            (OperatorFixity::Infix, "|", 5),
            (OperatorFixity::Infix, "^", 8),
            (OperatorFixity::Infix, "..", 3),
            (OperatorFixity::Infix, "..=", 3),
            (OperatorFixity::Infix, "..<", 3),
            (OperatorFixity::Postfix, "[]", 0),
            (OperatorFixity::Postfix, "[]=", 0),
            (OperatorFixity::Postfix, "⁅⁆", 0),
            (OperatorFixity::Postfix, "⁅⁆=", 0),
        ];
        for &(fixity, lexeme, precedence) in seeds {
            registry
                .intern_unique(fixity, lexeme, precedence, None)
                .unwrap_or_else(|error| panic!("builtin operator seed must be unique: {error}"));
        }
        registry
    }

    /// 按结合性 + 词素查找。
    pub fn lookup(&self, fixity: OperatorFixity, lexeme: &str) -> Option<OperatorId> {
        self.by_key.get(&(fixity_key(fixity), lexeme.to_string())).copied()
    }

    /// 由历史显示名（`infix ==` / `prefix !` / `suffix []`）查找。
    ///
    /// 仅用于迁移期：把已进入 HIR 的显示字符串映回 [`OperatorId`]；新代码应直接持有 id。
    pub fn lookup_display_name(&self, display: &str) -> Option<OperatorId> {
        let (fixity, lexeme) = parse_operator_display_name(display)?;
        self.lookup(fixity, lexeme)
    }

    /// 登记运算符；重复键失败关闭。
    pub fn intern_unique(
        &mut self,
        fixity: OperatorFixity,
        lexeme: impl Into<String>,
        precedence: u16,
        callee: Option<crate::ItemInstanceId>,
    ) -> Result<OperatorId, OperatorRegistryError> {
        let lexeme = lexeme.into();
        let key = (fixity_key(fixity), lexeme.clone());
        if let Some(existing) = self.by_key.get(&key).copied() {
            return Err(OperatorRegistryError::DuplicateKey { fixity, lexeme, existing });
        }
        let id = OperatorId::from_index(self.next_index).expect("operator id space");
        self.next_index = self.next_index.saturating_add(1);
        self.by_key.insert(key, id);
        self.by_id.insert(id, OperatorRegistration { id, lexeme, fixity, precedence, callee });
        Ok(id)
    }

    /// 旁表查询。
    pub fn registration(&self, id: OperatorId) -> Option<&OperatorRegistration> {
        self.by_id.get(&id)
    }
}

/// 解析迁移期显示名 `infix ==` / `prefix !` / `suffix []`。
pub fn parse_operator_display_name(display: &str) -> Option<(OperatorFixity, &str)> {
    if let Some(lexeme) = display.strip_prefix("infix ") {
        return Some((OperatorFixity::Infix, lexeme));
    }
    if let Some(lexeme) = display.strip_prefix("prefix ") {
        return Some((OperatorFixity::Prefix, lexeme));
    }
    if let Some(lexeme) = display.strip_prefix("suffix ") {
        return Some((OperatorFixity::Postfix, lexeme));
    }
    None
}

/// 内建运算符 id 便捷访问（与 [`OperatorRegistry::with_builtins`] 槽位一致）。
pub mod builtin_operator {
    use super::{OperatorFixity, OperatorId, OperatorRegistry};
    use std::sync::OnceLock;

    fn builtins() -> &'static OperatorRegistry {
        static REGISTRY: OnceLock<OperatorRegistry> = OnceLock::new();
        REGISTRY.get_or_init(OperatorRegistry::with_builtins)
    }

    /// 查内建表。
    pub fn lookup(fixity: OperatorFixity, lexeme: &str) -> Option<OperatorId> {
        builtins().lookup(fixity, lexeme)
    }

    /// 由显示名查内建表。
    pub fn lookup_display_name(display: &str) -> Option<OperatorId> {
        builtins().lookup_display_name(display)
    }

    /// 旁表查询（内建表）。
    pub fn registration(id: OperatorId) -> Option<&'static super::OperatorRegistration> {
        builtins().registration(id)
    }

    /// `prefix !`
    pub fn prefix_not() -> OperatorId {
        lookup(OperatorFixity::Prefix, "!").expect("seeded")
    }

    /// `prefix -`
    pub fn prefix_neg() -> OperatorId {
        lookup(OperatorFixity::Prefix, "-").expect("seeded")
    }

    /// `prefix +`
    pub fn prefix_pos() -> OperatorId {
        lookup(OperatorFixity::Prefix, "+").expect("seeded")
    }

    /// `infix ==`
    pub fn infix_eq() -> OperatorId {
        lookup(OperatorFixity::Infix, "==").expect("seeded")
    }

    /// `infix !=`
    pub fn infix_ne() -> OperatorId {
        lookup(OperatorFixity::Infix, "!=").expect("seeded")
    }

    /// `infix <`
    pub fn infix_lt() -> OperatorId {
        lookup(OperatorFixity::Infix, "<").expect("seeded")
    }

    /// `infix <=`
    pub fn infix_le() -> OperatorId {
        lookup(OperatorFixity::Infix, "<=").expect("seeded")
    }

    /// `infix >`
    pub fn infix_gt() -> OperatorId {
        lookup(OperatorFixity::Infix, ">").expect("seeded")
    }

    /// `infix >=`
    pub fn infix_ge() -> OperatorId {
        lookup(OperatorFixity::Infix, ">=").expect("seeded")
    }

    /// `infix +`
    pub fn infix_add() -> OperatorId {
        lookup(OperatorFixity::Infix, "+").expect("seeded")
    }

    /// `infix -`
    pub fn infix_sub() -> OperatorId {
        lookup(OperatorFixity::Infix, "-").expect("seeded")
    }

    /// `infix *`
    pub fn infix_mul() -> OperatorId {
        lookup(OperatorFixity::Infix, "*").expect("seeded")
    }

    /// `infix /`
    pub fn infix_div() -> OperatorId {
        lookup(OperatorFixity::Infix, "/").expect("seeded")
    }

    /// `infix %`
    pub fn infix_rem() -> OperatorId {
        lookup(OperatorFixity::Infix, "%").expect("seeded")
    }

    /// `infix &`
    pub fn infix_bit_and() -> OperatorId {
        lookup(OperatorFixity::Infix, "&").expect("seeded")
    }

    /// `infix |`
    pub fn infix_bit_or() -> OperatorId {
        lookup(OperatorFixity::Infix, "|").expect("seeded")
    }

    /// `infix &&`
    pub fn infix_and() -> OperatorId {
        lookup(OperatorFixity::Infix, "&&").expect("seeded")
    }

    /// `infix ||`
    pub fn infix_or() -> OperatorId {
        lookup(OperatorFixity::Infix, "||").expect("seeded")
    }

    /// 是否为比较 / 相等 / 逻辑类运算符（返回 `bool`）。
    pub fn is_boolean_result(id: OperatorId) -> bool {
        matches!(
            builtins().registration(id).map(|row| (row.fixity, row.lexeme.as_str())),
            Some((OperatorFixity::Prefix, "!")) | Some((OperatorFixity::Infix, "==" | "!=" | "<" | "<=" | ">" | ">=" | "&&" | "||"))
        )
    }

    /// 是否为数值算术 / 位运算中缀（返回操作数类型）。
    pub fn is_numeric_result(id: OperatorId) -> bool {
        matches!(
            builtins().registration(id).map(|row| (row.fixity, row.lexeme.as_str())),
            Some((OperatorFixity::Infix, "+" | "-" | "*" | "/" | "%" | "&" | "|" | "^" | "<<" | ">>"))
                | Some((OperatorFixity::Prefix, "-" | "+"))
        )
    }

    /// 是否为 wasm / nyar i32 原语路径可直接编码的运算符。
    pub fn is_i32_primitive(id: OperatorId) -> bool {
        id == prefix_not()
            || id == infix_eq()
            || id == infix_ne()
            || id == infix_lt()
            || id == infix_le()
            || id == infix_gt()
            || id == infix_ge()
            || id == infix_add()
            || id == infix_sub()
            || id == infix_mul()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn attribute_registry_interns_custom_names() {
        let mut registry = AttributeRegistry::with_builtins();
        assert_eq!(registry.lookup("export"), Some(builtin_attribute::export()));
        assert_eq!(registry.lookup("workload_phase"), Some(builtin_attribute::workload_phase()));
        let custom = registry.intern("my_attr");
        assert_eq!(registry.lookup("my_attr"), Some(custom));
        assert_eq!(registry.intern("my_attr"), custom);
        assert!(matches!(registry.intern_unique("export"), Err(AttributeRegistryError::DuplicateName { .. })));
    }

    #[test]
    fn operator_registry_seeds_and_rejects_duplicates() {
        let mut registry = OperatorRegistry::with_builtins();
        assert_eq!(registry.lookup_display_name("infix =="), Some(builtin_operator::infix_eq()));
        assert_eq!(registry.lookup_display_name("prefix !"), Some(builtin_operator::prefix_not()));
        assert!(matches!(registry.intern_unique(OperatorFixity::Infix, "==", 4, None), Err(OperatorRegistryError::DuplicateKey { .. })));
        let custom = registry.intern_unique(OperatorFixity::Infix, "+*", 7, None).expect("user op");
        assert_eq!(registry.lookup(OperatorFixity::Infix, "+*"), Some(custom));
    }
}
