//! Language-neutral guest interpret seam for the PE / Futamura substrate.
//!
//! Concrete language product logic lives in `nyar-language` guests. This module
//! only defines how the runner invokes them — no language ASTs, no product
//! semantics. Adapters under [`crate::evaluator`] convert `LegacyValue` env maps
//! to/from guest-local value types.

use std::collections::HashMap;

use crate::value::LegacyValue;

/// Function-pointer shape registered on [`crate::LegacyVmRunner`].
///
/// Guests typically expose `evaluate_<lang>_source` in `nyar-language`; VM
/// adapters wrap those into this signature.
pub type GuestInterpretFn = fn(&str, &mut HashMap<String, LegacyValue>) -> LegacyValue;

/// Thin, language-agnostic entry for “run this guest program”.
///
/// Prefer inherent APIs on concrete guests in `nyar-language`. This trait is the
/// substrate-side registration / dispatch surface only.
pub trait GuestInterpret {
    /// Canonical ids and aliases this guest answers to (e.g. `powershell`, `ps1`).
    fn language_ids(&self) -> &[&str];

    /// Interpret `source`, reading/writing the shared runtime environment.
    fn interpret(&self, source: &str, env: &mut HashMap<String, LegacyValue>) -> LegacyValue;
}

/// [`GuestInterpret`] over a static id list and [`GuestInterpretFn`].
#[derive(Debug, Clone, Copy)]
pub struct FnGuestInterpret {
    /// Language ids / aliases.
    pub ids: &'static [&'static str],
    /// Interpret entry point.
    pub interpret: GuestInterpretFn,
}

impl GuestInterpret for FnGuestInterpret {
    fn language_ids(&self) -> &[&str] {
        self.ids
    }

    fn interpret(&self, source: &str, env: &mut HashMap<String, LegacyValue>) -> LegacyValue {
        (self.interpret)(source, env)
    }
}
