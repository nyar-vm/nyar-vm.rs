/// Opaque native code artifact produced by a JIT backend.
///
/// Future backends attach machine code, stack maps, and deoptimization metadata here.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JitCompiledArtifact {
    /// Function index this artifact was compiled from.
    pub function_index: usize,
}
