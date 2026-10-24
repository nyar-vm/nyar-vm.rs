//! 运算符成员名的唯一所有者。
//!
//! Valkyrie 2020 不把运算符当作语义类别：`a + b` 就是 `a.infix +(b)` 的普通成员调用，
//! `!a` 就是 `a.prefix !()`，`a[i]` 就是 `a.suffix [](i)`。HIR/MIR/LIR 只保留普通成员名与
//! 普通 `Invoke`，不存在 operator/unary/binary 类别、`OperatorId` 或运算符身份表。
//!
//! 声明侧（`[infix] micro \`+\``）与调用侧（AST 运算符语法糖）必须得到同一个成员名，否则
//! 调用点会解析不到声明。因此「`{fixity} {lexeme}` 拼写」与「运算符词素 → 成员名」的映射
//! 只能有这一份所有者：HIR lowering 消费它，语义闭包规划消费它。
//!
//! 注意：本模块只给出**名字**，不给出语义。运算符没有独立身份，成员解析、重载匹配与
//! 歧义判定都在普通名字解析路径上完成。

use oak_valkyrie::lexer::token_type::ValkyrieTokenType;

use crate::frontend::ast::Attribute;

/// 序数下标 `receiver[i]` 的成员名。
pub(crate) const SUBSCRIPT_MEMBER: &str = "suffix []";
/// 序数下标赋值 `receiver[i] = value` 的成员名。
pub(crate) const SUBSCRIPT_ASSIGN_MEMBER: &str = "suffix []=";
/// 基数下标 `receiver::[i]` 的成员名。
pub(crate) const CARDINAL_SUBSCRIPT_MEMBER: &str = "suffix ⁅⁆";
/// 基数下标赋值 `receiver::[i] = value` 的成员名。
pub(crate) const CARDINAL_SUBSCRIPT_ASSIGN_MEMBER: &str = "suffix ⁅⁆=";
/// 解引用读取 `*receiver` 的成员名。
pub(crate) const DEREF_READ_MEMBER: &str = "deref_read";

/// `{fixity} {lexeme}`：运算符成员名的唯一拼写。
///
/// fixity 只允许 `prefix` / `infix` / `suffix` 三种词素；`prefix \`-\`` 与 `infix \`-\``
/// 共享词素，只有组合名能让两者在普通成员解析与调用点匹配中区分开。
pub(crate) fn fixed_member_name(fixity: &str, lexeme: &str) -> String {
    format!("{fixity} {lexeme}")
}

/// 中缀运算符调用的成员名（`a + b` → `a.infix +(b)`）。
pub(crate) fn infix_member_name(operator: &ValkyrieTokenType) -> String {
    let lexeme = match operator {
        ValkyrieTokenType::Plus => "+",
        ValkyrieTokenType::Minus => "-",
        ValkyrieTokenType::Star => "*",
        ValkyrieTokenType::Slash => "/",
        ValkyrieTokenType::Percent => "%",
        ValkyrieTokenType::EqEq => "==",
        ValkyrieTokenType::NotEq => "!=",
        ValkyrieTokenType::LessThan => "<",
        ValkyrieTokenType::LessEq => "<=",
        ValkyrieTokenType::GreaterThan => ">",
        ValkyrieTokenType::GreaterEq => ">=",
        ValkyrieTokenType::LeftShift => "<<",
        ValkyrieTokenType::RightShift => ">>",
        ValkyrieTokenType::Ampersand => "&",
        ValkyrieTokenType::Pipe => "|",
        ValkyrieTokenType::Caret => "^",
        ValkyrieTokenType::DotDot => "..",
        // `AndAnd` / `OrOr` / `Eq` 降低为控制流或赋值，不产生成员调用，调用方不会走到这里。
        _ => "+",
    };
    fixed_member_name("infix", lexeme)
}

/// 前缀运算符调用的成员名（`!a` → `a.prefix !()`）。
pub(crate) fn prefix_member_name(operator: &ValkyrieTokenType) -> String {
    let lexeme = match operator {
        ValkyrieTokenType::Bang => "!",
        _ => "-",
    };
    fixed_member_name("prefix", lexeme)
}

/// 运算符声明的成员名：`[infix] micro \`+\`` → `infix +`；非运算符声明保持原名。
///
/// fixity 判定与拼写都只在这里：声明侧（本函数）与调用侧（[`infix_member_name`] /
/// [`prefix_member_name`]）必须给出同一个名字，闭包、HIR 与后端都消费这个名字。
pub(crate) fn declared_member_name(declared: &str, annotations: &[Attribute]) -> String {
    let fixity = if has_modifier(annotations, "prefix") {
        "prefix"
    }
    else if has_modifier(annotations, "infix") {
        "infix"
    }
    else if has_modifier(annotations, "postfix") || has_modifier(annotations, "suffix") {
        "suffix"
    }
    else {
        return declared.to_string();
    };
    fixed_member_name(fixity, declared)
}

/// 调用点/声明侧的普通成员名是否为运算符成员（`{fixity} {lexeme}` 或 `deref_read`）。
pub(crate) fn is_operator_member_name(name: &str) -> bool {
    name.starts_with("prefix ") || name.starts_with("infix ") || name.starts_with("suffix ") || name == DEREF_READ_MEMBER
}

/// 声明是否带有指定修饰注解。
fn has_modifier(annotations: &[Attribute], name: &str) -> bool {
    annotations.iter().any(|attribute| attribute.name.name == name)
}
