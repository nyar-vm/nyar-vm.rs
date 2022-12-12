//! JavaScript / TypeScript analysis frontend (format + lint).
//!
//! Lives in `nyar-language` so product CLIs (`noodle`) call the analysis framework
//! instead of shelling out to external formatters/linters.

#![doc = include_str!("readme.md")]

pub mod format;
pub mod lint;
pub mod module;

pub use format::{JavascriptFormatter, format_javascript_source, javascript_language_ids};
pub use lint::{JavascriptLintIssue, JavascriptLintOptions, collect_javascript_files, lint_javascript_source};
pub use module::JavascriptModule;
