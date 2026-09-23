//! Workspace-level disk cache mechanism.
//!
//! Provides content-addressed bucket storage under a cache root. Compilation-stage
//! semantics (token / staging / ir keys) live in consumers such as `legion`.

mod error;
mod hash;
mod path;
mod store;

pub use error::WorkspaceCacheError;
pub use hash::{combined_hash, file_hash, files_hash};
pub use path::{resolve_marker_root, sanitize_bucket_name};
pub use store::WorkspaceCache;

/// Result type for workspace cache operations.
pub type Result<T> = std::result::Result<T, WorkspaceCacheError>;
