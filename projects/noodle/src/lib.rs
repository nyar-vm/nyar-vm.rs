//! Noodle — Node.js unified toolchain library.

#![warn(missing_docs)]

pub mod cmds;
pub mod layout;
pub mod manifest;
pub mod node_modules;
pub mod project;

#[cfg(test)]
mod architecture_guards;

pub use layout::project_layout;
pub use node_modules::{
    FOREIGN_LOCKFILES, LayoutKind, MaterializeReport, PNPM_STORE_DIR, materialize_for_compat, materialize_node_modules,
    materialize_node_modules_for, note_foreign_lockfiles, pnpm_store_id,
};
pub use project::{NoodleProject, PmCompat};
