//! Guest language adapters for the legacy VM runner.
//!
//! **Target shape:** thin bridges that delegate interpret to `nyar-language` guests
//! (see `bash.rs` / `lua.rs` / `powershell.rs`). Registration uses the language-neutral
//! [`crate::guest::GuestInterpretFn`] seam — no concrete language ASTs here.

mod bash;
mod c;
mod lua;
mod powershell;
mod shell;
mod tcl;

pub use bash::evaluate_bash_script;
pub use c::evaluate_c_script;
pub use lua::evaluate_lua_script;
pub use powershell::evaluate_powershell_script;
pub use tcl::evaluate_tcl_script;

pub use crate::guest::{FnGuestInterpret, GuestInterpret, GuestInterpretFn};

use std::collections::HashMap;

use crate::value::LegacyValue;

/// AST/tree evaluator interface (optional object-shaped hook over [`GuestInterpretFn`]).
pub trait TreeEvaluator {
    /// Evaluate source in the given environment.
    fn evaluate(&mut self, source: &str, env: &mut HashMap<String, LegacyValue>) -> LegacyValue;
}
