//! Rename context for tracking variable renames during lowering.

use std::collections::HashMap;

/// Context for tracking variable renames during lowering.
///
/// This structure manages scope-based variable shadowing by tracking
/// shadow indices for each variable name within nested scopes.
#[derive(Debug, Clone)]
pub struct RenameContext {
    /// The source ID for the current compilation unit.
    #[allow(dead_code)]
    source_id: crate::SourceID,
    /// Stack of scopes, each mapping variable names to their shadow indices.
    scopes: Vec<HashMap<String, u32>>,
    /// Counter for generating fresh shadow indices per variable name.
    counter: HashMap<String, u32>,
}

impl RenameContext {
    /// Creates a new rename context for the given source ID.
    pub fn new(source_id: crate::SourceID) -> Self {
        Self { source_id, scopes: vec![HashMap::new()], counter: HashMap::new() }
    }

    /// Pushes a new scope onto the scope stack.
    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
    }

    /// Pops the current scope from the scope stack.
    pub fn pop_scope(&mut self) {
        self.scopes.pop();
    }

    /// Generates a fresh shadow index for the given variable name.
    pub fn fresh_index(&mut self, name: &str) -> u32 {
        let idx = self.counter.entry(name.to_string()).or_insert(0);
        let result = *idx;
        *idx += 1;
        result
    }

    /// Adds a variable to the current scope with its shadow index.
    pub fn add_to_current_scope(&mut self, name: &str, index: u32) {
        if let Some(scope) = self.scopes.last_mut() {
            scope.insert(name.to_string(), index);
        }
    }

    /// Gets the current shadow index for a variable name.
    ///
    /// Searches from the innermost scope outward.
    pub fn get_current_index(&self, name: &str, _span: std::range::Range<u32>) -> Option<u32> {
        for scope in self.scopes.iter().rev() {
            if let Some(&idx) = scope.get(name) {
                return Some(idx);
            }
        }
        None
    }
}
