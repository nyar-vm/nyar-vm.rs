//! Language-agnostic PE / Futamura interpreter substrate (GraalVM / Truffle-like).
//!
//! Core (`algebra` / `compiler` / `value` / [`guest`]) must stay free of concrete language
//! product logic. Guest languages live in `nyar-language` and plug in via thin evaluator
//! adapters that implement the [`guest`] seam. See crate `readme.md` and
//! `projects/host-script-languages.md`.

pub mod algebra;
pub mod compiler;
pub mod evaluator;
pub mod guest;
pub mod runner;
pub mod value;

pub use guest::{FnGuestInterpret, GuestInterpret, GuestInterpretFn};
pub use runner::LegacyVmRunner;
pub use value::LegacyValue;
