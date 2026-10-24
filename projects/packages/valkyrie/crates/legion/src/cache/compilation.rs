//! Legion 产物缓存存储；不拥有词法分析或源码 staging 路线。

use nyar_workspace::{WorkspaceCache, combined_hash};

const TYPE_IR: &str = "ir";
const TYPE_ENTRY_SLICE: &str = "entry-slice";
/// IR / artifact-set cache entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IrCacheEntry {
    /// Kind label (`"HIR"` / `"MIR"` / `"LIR"` / `"artifact-set"`).
    pub ir_kind: String,
    /// Serialized IR or artifact bundle.
    pub ir_data: Vec<u8>,
    /// IR hash.
    pub ir_hash: String,
    /// Canonical triple.
    pub canonical_triple: String,
}

/// Entry-slice (reachable functions) cache entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntrySliceCacheEntry {
    /// Entry function name.
    pub entry_name: String,
    /// Reachable function names.
    pub reachable_functions: Vec<String>,
    /// Call graph hash.
    pub call_graph_hash: String,
    /// Canonical triple.
    pub canonical_triple: String,
}

/// Legion-facing compilation cache.
#[derive(Debug, Clone)]
pub struct CompilationCache {
    store: WorkspaceCache,
}

impl CompilationCache {
    /// Open cache under a cache root directory (typically `{root}/.cache`).
    pub fn open(cache_root: impl AsRef<std::path::Path>) -> Self {
        Self { store: WorkspaceCache::open(cache_root) }
    }

    /// Underlying disk store root.
    pub fn cache_root(&self) -> &std::path::Path {
        &self.store.root
    }

    /// Try get IR / artifact-set.
    pub fn try_get_ir(&self, module_name: &str, canonical_triple: &str, ir_hash: &str) -> Option<IrCacheEntry> {
        let key = combined_hash(&[module_name, canonical_triple, ir_hash]);
        let data = self.store.get(canonical_triple, &key, TYPE_IR).ok().flatten()?;
        let mut cursor = ByteReader::new(&data);
        let ir_kind = cursor.read_string().ok()?;
        let ir_hash = cursor.read_string().ok()?;
        let canonical_triple = cursor.read_string().ok()?;
        let ir_data = cursor.read_bytes().ok()?;
        Some(IrCacheEntry { ir_kind, ir_data, ir_hash, canonical_triple })
    }

    /// Put IR / artifact-set.
    pub fn put_ir(&self, module_name: &str, canonical_triple: &str, ir_hash: &str, entry: &IrCacheEntry) -> Result<(), String> {
        let key = combined_hash(&[module_name, canonical_triple, ir_hash]);
        let mut buf = ByteWriter::new();
        buf.write_string(&entry.ir_kind);
        buf.write_string(&entry.ir_hash);
        buf.write_string(&entry.canonical_triple);
        buf.write_bytes(&entry.ir_data);
        self.store.put(canonical_triple, &key, TYPE_IR, &buf.into_inner()).map_err(|e| e.to_string())
    }

    /// Try get entry slice.
    pub fn try_get_entry_slice(&self, entry_name: &str, canonical_triple: &str, call_graph_hash: &str) -> Option<EntrySliceCacheEntry> {
        let key = combined_hash(&[entry_name, canonical_triple, call_graph_hash]);
        let data = self.store.get(canonical_triple, &key, TYPE_ENTRY_SLICE).ok().flatten()?;
        let mut cursor = ByteReader::new(&data);
        let entry_name = cursor.read_string().ok()?;
        let canonical_triple = cursor.read_string().ok()?;
        let call_graph_hash = cursor.read_string().ok()?;
        let count = cursor.read_i32_le().ok()? as usize;
        let mut reachable_functions = Vec::with_capacity(count);
        for _ in 0..count {
            reachable_functions.push(cursor.read_string().ok()?);
        }
        Some(EntrySliceCacheEntry { entry_name, reachable_functions, call_graph_hash, canonical_triple })
    }

    /// Put entry slice.
    pub fn put_entry_slice(
        &self,
        entry_name: &str,
        canonical_triple: &str,
        call_graph_hash: &str,
        entry: &EntrySliceCacheEntry,
    ) -> Result<(), String> {
        let key = combined_hash(&[entry_name, canonical_triple, call_graph_hash]);
        let mut buf = ByteWriter::new();
        buf.write_string(&entry.entry_name);
        buf.write_string(&entry.canonical_triple);
        buf.write_string(&entry.call_graph_hash);
        buf.write_i32_le(entry.reachable_functions.len() as i32);
        for name in &entry.reachable_functions {
            buf.write_string(name);
        }
        self.store.put(canonical_triple, &key, TYPE_ENTRY_SLICE, &buf.into_inner()).map_err(|e| e.to_string())
    }

    /// Invalidate one triple bucket.
    pub fn invalidate(&self, canonical_triple: &str) -> Result<(), String> {
        self.store.invalidate_bucket(canonical_triple).map_err(|e| e.to_string())
    }

    /// Invalidate all cache entries.
    pub fn invalidate_all(&self) -> Result<(), String> {
        self.store.invalidate_all().map_err(|e| e.to_string())
    }
}

struct ByteWriter {
    buf: Vec<u8>,
}

impl ByteWriter {
    fn new() -> Self {
        Self { buf: Vec::new() }
    }

    fn write_i32_le(&mut self, value: i32) {
        self.buf.extend_from_slice(&value.to_le_bytes());
    }

    fn write_string(&mut self, value: &str) {
        let bytes = value.as_bytes();
        self.write_i32_le(bytes.len() as i32);
        self.buf.extend_from_slice(bytes);
    }

    fn write_bytes(&mut self, data: &[u8]) {
        self.write_i32_le(data.len() as i32);
        self.buf.extend_from_slice(data);
    }

    fn into_inner(self) -> Vec<u8> {
        self.buf
    }
}

struct ByteReader<'a> {
    data: &'a [u8],
    pos: usize,
}

impl<'a> ByteReader<'a> {
    fn new(data: &'a [u8]) -> Self {
        Self { data, pos: 0 }
    }

    fn read_i32_le(&mut self) -> Result<i32, ()> {
        if self.pos + 4 > self.data.len() {
            return Err(());
        }
        let value = i32::from_le_bytes(self.data[self.pos..self.pos + 4].try_into().map_err(|_| ())?);
        self.pos += 4;
        Ok(value)
    }

    fn read_string(&mut self) -> Result<String, ()> {
        let len = self.read_i32_le()? as usize;
        if self.pos + len > self.data.len() {
            return Err(());
        }
        let start = self.pos;
        self.pos += len;
        String::from_utf8(self.data[start..self.pos].to_vec()).map_err(|_| ())
    }

    fn read_bytes(&mut self) -> Result<Vec<u8>, ()> {
        let len = self.read_i32_le()? as usize;
        if self.pos + len > self.data.len() {
            return Err(());
        }
        let start = self.pos;
        self.pos += len;
        Ok(self.data[start..self.pos].to_vec())
    }
}
#[cfg(test)]
#[path = "../../tests/path_wired/cache_compilation.rs"]
mod cache_compilation_tests;
