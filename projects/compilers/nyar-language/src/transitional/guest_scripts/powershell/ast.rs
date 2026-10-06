//! PowerShell AST (legend demo subset).

use serde::{Deserialize, Serialize};

/// Statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PsStmt {
    /// `{ ... }`
    Block(Vec<PsStmt>),
    /// `$name = expr`
    Assign {
        /// Variable name without `$`.
        name: String,
        /// Right-hand side.
        value: PsExpr,
    },
    /// `if (cond) { ... } [else { ... }]`
    If {
        /// Condition.
        condition: PsExpr,
        /// Then body.
        then_branch: Vec<PsStmt>,
        /// Else body (empty when omitted).
        else_branch: Vec<PsStmt>,
    },
    /// `while (cond) { ... }`
    While {
        /// Loop condition.
        condition: PsExpr,
        /// Loop body.
        body: Vec<PsStmt>,
    },
    /// `for (init; cond; step) { ... }`
    For {
        /// Optional initializer statement.
        init: Option<Box<PsStmt>>,
        /// Optional condition.
        condition: Option<PsExpr>,
        /// Optional step expression.
        step: Option<PsExpr>,
        /// Loop body.
        body: Vec<PsStmt>,
    },
    /// `function Name($a, $b) { ... }`
    Function {
        /// Function name.
        name: String,
        /// Parameter names without `$`.
        params: Vec<String>,
        /// Function body.
        body: Vec<PsStmt>,
    },
    /// `return [expr]`
    Return(Option<PsExpr>),
    /// Expression / command statement.
    Expr(PsExpr),
}

/// Expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PsExpr {
    /// Integer literal.
    Int(i64),
    /// Floating literal.
    Float(f64),
    /// String literal.
    String(String),
    /// Boolean literal (`$true` / `$false`).
    Bool(bool),
    /// `$null`
    Null,
    /// `$name`
    Var(String),
    /// Bare identifier (command name / pipeline sink).
    Ident(String),
    /// Binary operator (`+`, `-eq`, `-and`, `=`, …).
    Binary {
        /// Operator text.
        op: String,
        /// Left operand.
        left: Box<PsExpr>,
        /// Right operand.
        right: Box<PsExpr>,
    },
    /// Unary operator (`-not`, `-`).
    Unary {
        /// Operator text.
        op: String,
        /// Operand.
        operand: Box<PsExpr>,
    },
    /// Command / function call.
    Call {
        /// Callee name.
        name: String,
        /// Arguments.
        args: Vec<PsExpr>,
    },
    /// `left | right`
    Pipeline {
        /// Upstream expression.
        left: Box<PsExpr>,
        /// Downstream expression / call.
        right: Box<PsExpr>,
    },
}
