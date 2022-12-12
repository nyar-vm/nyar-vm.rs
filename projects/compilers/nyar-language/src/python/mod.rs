//! Python analysis frontend (format + lint).
//!
//! Product CLI: `panda`.

#![doc = include_str!("readme.md")]

pub mod format;
pub mod lint;
pub mod module;

pub use format::{PythonFormatter, format_python_source, python_language_ids};
pub use lint::{PythonLintIssue, PythonLintOptions, collect_python_files, lint_python_path, lint_python_source};
pub use module::PythonModule;
