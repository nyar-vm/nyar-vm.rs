# nyar-workspace

Workspace-level content-addressed disk cache for Nyar tooling.

## Overview

`nyar-workspace` stores typed binary payloads under `{cache_root}/{bucket}/{key_hash}.nyar`. Entries are wrapped with `nyar-bytecode::encode_module` so each blob carries a format version and a **type tag** (`NyarModuleData::name`) separate from the bucket name. Compilers and package tooling define semantic cache keys; this crate only provides safe paths, hashing helpers, and get/put IO.

## Features

- **`WorkspaceCache::open`** — opens a cache root (typically `{workspace}/.cache`); directories are created on first write.
- **`get(bucket, key_hash, expected_type)`** — returns payload bytes when the entry exists and the type tag matches; decode errors and tag mismatches are treated as cache misses (`Ok(None)`).
- **`put(bucket, key_hash, type_tag, payload)`** — writes a versioned module wrapper around raw bytes.
- **`combined_hash` / `file_hash` / `files_hash`** — stable hash inputs for cache key construction.
- **`sanitize_bucket_name`** — filesystem-safe bucket directory names.
- **`resolve_marker_root`** — locates workspace roots from marker files (product-specific callers).

## Usage

```rust
use nyar_workspace::{WorkspaceCache, files_hash};

let cache = WorkspaceCache::open(".cache");
let key = files_hash(&["src/main.val", "legion.toml"]);
cache.put("compile", &key, "valkyrie.mir", &mir_bytes)?;
if let Some(hit) = cache.get("compile", &key, "valkyrie.mir")? {
    // use hit
}
```

## Boundaries

- Does not parse source code or execute bytecode.
- Does not assign meaning to bucket names beyond storage layout.
- Type tags are conventions agreed by the caller (e.g. `valkyrie.mir`, staging tokens).
