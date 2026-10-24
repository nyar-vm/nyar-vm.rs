//! 封闭类静态化与见证消除优化。

mod closed_class;

pub use closed_class::{
    ClosedClassOptimizations, ClosedClassOptimizer, DeadCodeEliminationAnalyzer, MethodInlineAnalyzer, OptimizationResult,
    StackAllocationAnalyzer, StackAllocationRejectionReason, WitnessEliminationPass,
};
