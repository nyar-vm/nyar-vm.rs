#![warn(missing_docs)]

//! Nyar bytecode VM — language-agnostic `.nyar` / Nyar IR runtime (JVM-style substrate).
//!
//! # Layering
//!
//! - This crate executes **decoded Nyar IR modules** only. It must not import concrete
//!   language ASTs or compilers.
//! - Concrete languages live in `nyar-language` (guests / frontends) and lower through
//!   the `emitter` crate into `.nyar` bytes before this VM sees them.
//! - Tree-walk / PE host-script substrate is **`legacy-vm`** (separate); other
//!   `nyar-*` crates are not “part of” this VM.
//!
//! # Naming
//!
//! This runtime is **nvm** (like JVM): *the* Nyar bytecode VM, not a generic
//! “nyar series VM architecture.” Crate/lib: `nvm`. CLI binary stays `nyar-vm`
//! (bare `nvm` collides with Node Version Manager on PATH).

pub mod error;
pub mod executor;
pub mod frame;
pub mod gc;
pub mod heap;
pub mod jit;
pub mod module;
pub mod ops;
pub mod stack;
pub mod value;
pub mod vm;

pub use error::NyarRuntimeError;
pub use heap::ObjectHeap;
pub use module::{LoadedModule, ModuleGlobals};
pub use value::{CoroutineState, Value};
pub use vm::NyarVm;
