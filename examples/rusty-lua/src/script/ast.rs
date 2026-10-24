//! Mid-subset Lua AST for legacy VM (control flow + tables; not full Lua 5.x).

use serde::{Deserialize, Serialize};

/// Lua AST node / statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LuaNode {
    /// Statement block.
    Block(Vec<LuaStmt>),
    /// `local a, b = ...`
    LocalDecl {
        /// Variable names.
        names: Vec<String>,
        /// Initializer expressions.
        values: Vec<LuaExpr>,
    },
    /// `a, t.x, t[1] = ...`
    Assign {
        /// Assignment targets.
        targets: Vec<LuaLValue>,
        /// Values.
        values: Vec<LuaExpr>,
    },
    /// `if cond then ... [elseif ...] [else ...] end`
    If {
        /// Condition.
        condition: LuaExpr,
        /// Then block.
        then_block: Vec<LuaStmt>,
        /// Else block (elseif is nested `If` here).
        else_block: Vec<LuaStmt>,
    },
    /// `while cond do ... end`
    While {
        /// Loop condition.
        condition: LuaExpr,
        /// Loop body.
        body: Vec<LuaStmt>,
    },
    /// `repeat ... until cond`
    Repeat {
        /// Loop body.
        body: Vec<LuaStmt>,
        /// Exit condition (loop while false).
        condition: LuaExpr,
    },
    /// `for name = start, limit [, step] do ... end`
    ForNumeric {
        /// Loop variable.
        name: String,
        /// Start expression.
        start: LuaExpr,
        /// Limit expression.
        limit: LuaExpr,
        /// Optional step (default 1).
        step: Option<LuaExpr>,
        /// Loop body.
        body: Vec<LuaStmt>,
    },
    /// `break`
    Break,
    /// `return [expr]`
    Return(Option<LuaExpr>),
    /// Expression statement (`print(...)`, calls, …).
    ExprStmt(LuaExpr),
    /// `function name(...) ... end` / `local function name(...) ... end`
    FunctionDef {
        /// Function name.
        name: String,
        /// Parameter names.
        params: Vec<String>,
        /// Function body.
        body: Vec<LuaStmt>,
    },
}

/// Statement alias.
pub type LuaStmt = LuaNode;

/// Assignable location.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LuaLValue {
    /// Bare name.
    Name(String),
    /// `table[key]`
    Index {
        /// Table expression.
        table: LuaExpr,
        /// Key expression.
        key: LuaExpr,
    },
    /// `table.field`
    Field {
        /// Table expression.
        table: LuaExpr,
        /// Field name.
        name: String,
    },
}

/// Table constructor field.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LuaTableField {
    /// Positional / array part entry.
    Array(LuaExpr),
    /// `name = expr`
    Record {
        /// Field name.
        key: String,
        /// Value.
        value: LuaExpr,
    },
    /// `[expr] = expr`
    Indexed {
        /// Key expression.
        key: LuaExpr,
        /// Value.
        value: LuaExpr,
    },
}

/// Lua expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LuaExpr {
    /// Numeric literal.
    Number(f64),
    /// String literal.
    String(String),
    /// Boolean literal.
    Bool(bool),
    /// `nil`
    Nil,
    /// Identifier.
    Ident(String),
    /// `{ ... }`
    Table {
        /// Constructor fields.
        fields: Vec<LuaTableField>,
    },
    /// `table[key]`
    Index {
        /// Table expression.
        table: Box<LuaExpr>,
        /// Key expression.
        key: Box<LuaExpr>,
    },
    /// `table.field`
    Field {
        /// Table expression.
        table: Box<LuaExpr>,
        /// Field name.
        name: String,
    },
    /// Binary operation.
    Binary {
        /// Operator text.
        op: String,
        /// Left operand.
        left: Box<LuaExpr>,
        /// Right operand.
        right: Box<LuaExpr>,
    },
    /// Unary operation.
    Unary {
        /// Operator text.
        op: String,
        /// Operand.
        operand: Box<LuaExpr>,
    },
    /// Named function call (`print`, user functions).
    Call {
        /// Callee name.
        name: String,
        /// Arguments.
        args: Vec<LuaExpr>,
    },
}
