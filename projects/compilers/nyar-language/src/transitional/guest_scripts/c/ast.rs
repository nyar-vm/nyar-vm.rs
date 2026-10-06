//! Minimal C AST for the legend / legacy-vm subset.

use serde::{Deserialize, Serialize};

/// Top-level translation unit item.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CItem {
    /// Function definition.
    Function(CFunction),
    /// File-scope variable declaration.
    GlobalVar(CVarDecl),
}

/// Function definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CFunction {
    /// Return type text (`int`, `void`, …).
    pub return_type: String,
    /// Function name.
    pub name: String,
    /// Parameter names (types are ignored beyond presence).
    pub params: Vec<String>,
    /// Function body.
    pub body: Vec<CStmt>,
}

/// Variable declaration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CVarDecl {
    /// Declared type text.
    pub ty: String,
    /// Variable name.
    pub name: String,
    /// Optional initializer.
    pub init: Option<CExpr>,
}

/// Statement.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CStmt {
    /// `{ ... }`
    Block(Vec<CStmt>),
    /// Local declaration.
    Decl(CVarDecl),
    /// Expression statement.
    Expr(CExpr),
    /// `return [expr];`
    Return(Option<CExpr>),
    /// `if (cond) then [else]`
    If {
        /// Condition.
        condition: CExpr,
        /// Then branch.
        then_branch: Box<CStmt>,
        /// Optional else branch.
        else_branch: Option<Box<CStmt>>,
    },
    /// `while (cond) body`
    While {
        /// Loop condition.
        condition: CExpr,
        /// Loop body.
        body: Box<CStmt>,
    },
    /// `for (init; cond; step) body`
    For {
        /// Optional initializer statement.
        init: Option<Box<CStmt>>,
        /// Optional condition.
        condition: Option<CExpr>,
        /// Optional step expression.
        step: Option<CExpr>,
        /// Loop body.
        body: Box<CStmt>,
    },
    /// `break;`
    Break,
    /// `continue;`
    Continue,
}

/// Expression.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CExpr {
    /// Integer literal.
    Int(i64),
    /// Floating literal.
    Float(f64),
    /// String literal.
    String(String),
    /// Character literal (stored as its code point).
    Char(i64),
    /// Identifier.
    Ident(String),
    /// Binary operator.
    Binary {
        /// Operator text.
        op: String,
        /// Left operand.
        left: Box<CExpr>,
        /// Right operand.
        right: Box<CExpr>,
    },
    /// Unary operator.
    Unary {
        /// Operator text.
        op: String,
        /// Operand.
        operand: Box<CExpr>,
    },
    /// Assignment `lhs = rhs` (lhs is an identifier for this subset).
    Assign {
        /// Target name.
        name: String,
        /// Value.
        value: Box<CExpr>,
    },
    /// Function / builtin call.
    Call {
        /// Callee name.
        name: String,
        /// Arguments.
        args: Vec<CExpr>,
    },
}
