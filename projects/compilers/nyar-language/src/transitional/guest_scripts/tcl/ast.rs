//! Tcl command AST (legend demo subset).

use serde::{Deserialize, Serialize};

/// Word with brace metadata for substitution control.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TclWord {
    /// Text with outer braces stripped.
    pub text: String,
    /// True when the source word was `{...}`.
    pub braced: bool,
}

/// Tcl script command.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TclCommand {
    /// `set name value`
    Set {
        /// Variable name.
        name: String,
        /// Value word.
        value: TclWord,
    },
    /// `puts word...`
    Puts {
        /// Output words.
        words: Vec<TclWord>,
    },
    /// `expr expression`
    Expr {
        /// Expression word.
        expression: TclWord,
    },
    /// `incr name [delta]`
    Incr {
        /// Variable name.
        name: String,
        /// Optional delta.
        delta: Option<TclWord>,
    },
    /// `if test body`
    If {
        /// Test expression.
        test: String,
        /// Body script.
        body: String,
    },
    /// `while test body`
    While {
        /// Test expression.
        test: String,
        /// Body script.
        body: String,
    },
    /// `for {init} {test} {next} {body}`
    For {
        /// Init script.
        init: String,
        /// Test expression.
        test: String,
        /// Next script.
        next: String,
        /// Loop body.
        body: String,
    },
    /// `foreach varList list body`
    Foreach {
        /// Loop variables.
        vars: Vec<String>,
        /// List word.
        list: TclWord,
        /// Loop body.
        body: String,
    },
    /// `proc name args body`
    Proc {
        /// Procedure name.
        name: String,
        /// Formal parameters.
        params: Vec<String>,
        /// Procedure body.
        body: String,
    },
    /// `return [value]`
    Return {
        /// Optional return value.
        value: Option<TclWord>,
    },
    /// `list word...`
    List {
        /// Elements.
        words: Vec<TclWord>,
    },
    /// `llength list`
    Llength {
        /// List word.
        list: TclWord,
    },
    /// `lindex list index`
    Lindex {
        /// List word.
        list: TclWord,
        /// Index word.
        index: TclWord,
    },
    /// User / unknown command call.
    Call {
        /// Command name.
        name: String,
        /// Arguments.
        args: Vec<TclWord>,
    },
}
