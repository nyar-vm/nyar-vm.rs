//! Legion 的产物缓存存储约定。
//!
//! 此模块不编译源码，不保存可替代 Compiler 入口的 token 或 staging 成功载荷。

mod artifact;
mod compilation;
mod resolve;

pub use artifact::{BuildBundle, CachedBuildBundle, collect_build_bundle, compute_artifact_hash, store_cached_build, toolchain_fingerprint};
pub use compilation::{CompilationCache, EntrySliceCacheEntry, IrCacheEntry};
pub use resolve::{cache_root_for, resolve_cache_root, resolve_workspace_root};
