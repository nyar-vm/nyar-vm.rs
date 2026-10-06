//! Minimal Bash AST for the legend / legacy-vm subset.

use serde::{Deserialize, Serialize};

/// Top-level statement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BashStmt {
    /// `NAME=value`
    Assign {
        /// Variable name.
        name: String,
        /// Raw value text (may contain `$vars`).
        value: String,
    },
    /// `export NAME[=value]`
    Export {
        /// Variable name.
        name: String,
        /// Optional value text.
        value: Option<String>,
    },
    /// Simple command: `echo hi`, `[ $x -eq 1 ]`, `cd /tmp`, …
    Command {
        /// Command words (argv), first is the command name.
        words: Vec<String>,
        /// Optional redirections.
        redirects: Vec<BashRedirect>,
    },
    /// `left | right | …`
    Pipeline {
        /// Pipeline stages (usually `Command`).
        stages: Vec<BashStmt>,
    },
    /// `left && right` / `left || right`
    AndOr {
        /// Left side.
        left: Box<BashStmt>,
        /// `&&` or `||`.
        op: String,
        /// Right side.
        right: Box<BashStmt>,
    },
    /// `if cond; then … [else …] fi`
    If {
        /// Condition command (exit status).
        condition: Box<BashStmt>,
        /// Then body.
        then_body: Vec<BashStmt>,
        /// Else body.
        else_body: Vec<BashStmt>,
    },
    /// `while cond; do … done`
    While {
        /// Loop condition.
        condition: Box<BashStmt>,
        /// Loop body.
        body: Vec<BashStmt>,
    },
    /// `for name in words; do … done`
    For {
        /// Loop variable.
        var: String,
        /// Iteration words.
        items: Vec<String>,
        /// Loop body.
        body: Vec<BashStmt>,
    },
    /// `name() { … }` / `function name { … }`
    FunctionDef {
        /// Function name.
        name: String,
        /// Function body.
        body: Vec<BashStmt>,
    },
    /// `{ … }`
    Group(Vec<BashStmt>),
    /// `break`
    Break,
    /// `continue`
    Continue,
    /// `return [n]`
    Return(Option<i64>),
}

/// Output / input redirection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BashRedirect {
    /// `> path` or `>> path`
    Write {
        /// Target path (virtual in the demo interpreter).
        path: String,
        /// Append when true.
        append: bool,
    },
    /// `< path`
    Read {
        /// Source path.
        path: String,
    },
}
