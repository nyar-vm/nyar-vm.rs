//! 封闭类优化：单态化前提下消除动态 witness 派发。

use std::collections::{HashMap, HashSet};

use crate::types::{Identifier, hir::HirStruct};

/// 针对单个封闭类的优化结论。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedClassOptimizations {
    /// 是否可将全部方法内联。
    pub can_inline_all_methods: bool,
    /// 封闭类无需保留 trait witness 动态派发。
    pub no_witness_dispatch_needed: bool,
    /// 是否可栈分配实例。
    pub can_stack_allocate: bool,
    /// 是否可做死代码消除。
    pub dead_code_elimination: bool,
}

impl ClosedClassOptimizations {
    /// 分析单个 class 的封闭类优化空间。
    pub fn analyze(class: &HirStruct) -> Self {
        let optimizable = class.is_closed();
        Self {
            can_inline_all_methods: optimizable,
            no_witness_dispatch_needed: optimizable,
            can_stack_allocate: optimizable,
            dead_code_elimination: optimizable,
        }
    }

    /// 可用优化项数量。
    pub fn optimization_count(&self) -> usize {
        [self.can_inline_all_methods, self.no_witness_dispatch_needed, self.can_stack_allocate, self.dead_code_elimination]
            .into_iter()
            .filter(|enabled| *enabled)
            .count()
    }

    /// 是否存在任何可用优化。
    pub fn has_optimizations(&self) -> bool {
        self.optimization_count() > 0
    }
}

/// 单类优化结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptimizationResult {
    /// 类名。
    pub class_name: Identifier,
    /// 优化结论。
    pub optimizations: ClosedClassOptimizations,
    /// 是否可栈分配。
    pub can_stack_allocate: bool,
}

/// 方法内联分析器。
#[derive(Debug, Default)]
pub struct MethodInlineAnalyzer {
    closed_classes: HashSet<String>,
}

impl MethodInlineAnalyzer {
    /// 创建分析器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册封闭类。
    pub fn register_closed_class(&mut self, class: &HirStruct) {
        if class.is_closed() {
            self.closed_classes.insert(class.name.to_string());
        }
    }

    /// 是否可内联该类的方法。
    pub fn can_inline_method(&self, class_name: &Identifier) -> bool {
        self.closed_classes.contains(class_name.as_str())
    }

    /// 已注册封闭类数量。
    pub fn closed_class_count(&self) -> usize {
        self.closed_classes.len()
    }

    /// 清空状态。
    pub fn clear(&mut self) {
        self.closed_classes.clear();
    }
}

/// 见证消除 pass：封闭类在单态化后可去掉 witness 动态派发。
#[derive(Debug, Default)]
pub struct WitnessEliminationPass {
    eliminated: HashSet<String>,
}

impl WitnessEliminationPass {
    /// 创建 pass。
    pub fn new() -> Self {
        Self::default()
    }

    /// 对 class 列表运行见证消除分析。
    pub fn run(&mut self, classes: &[HirStruct]) -> usize {
        let mut count = 0;
        for class in classes {
            if class.is_closed() {
                if self.eliminated.insert(class.name.to_string()) {
                    count += 1;
                }
            }
        }
        count
    }

    /// 类是否已标记为可消除 witness 派发。
    pub fn is_eliminated(&self, class_name: &Identifier) -> bool {
        self.eliminated.contains(class_name.as_str())
    }

    /// 已消除 witness 派发的类名列表。
    pub fn eliminated_witness_tables(&self) -> Vec<Identifier> {
        self.eliminated.iter().map(|name| Identifier::new(name)).collect()
    }

    /// 清空状态。
    pub fn clear(&mut self) {
        self.eliminated.clear();
    }
}

/// 栈分配拒绝原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StackAllocationRejectionReason {
    /// 非封闭类。
    NotClosed,
    /// 实例过大。
    TooLarge,
}

/// 栈分配分析器。
#[derive(Debug, Default)]
pub struct StackAllocationAnalyzer {
    candidates: Vec<Identifier>,
    rejected: Vec<(Identifier, StackAllocationRejectionReason)>,
}

impl StackAllocationAnalyzer {
    /// 创建分析器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 分析是否可栈分配。
    pub fn analyze(&mut self, class: &HirStruct) -> bool {
        if class.is_closed() {
            self.candidates.push(class.name.clone());
            true
        }
        else {
            self.rejected.push((class.name.clone(), StackAllocationRejectionReason::NotClosed));
            false
        }
    }

    /// 带大小上限的栈分配分析。
    pub fn analyze_with_size(&mut self, class: &HirStruct, max_bytes: usize) -> bool {
        if !class.is_closed() {
            self.rejected.push((class.name.clone(), StackAllocationRejectionReason::NotClosed));
            return false;
        }
        let estimated = class.fields.len().saturating_mul(8);
        if estimated > max_bytes {
            self.rejected.push((class.name.clone(), StackAllocationRejectionReason::TooLarge));
            return false;
        }
        self.candidates.push(class.name.clone());
        true
    }

    /// 栈分配候选。
    pub fn stack_allocation_candidates(&self) -> &[Identifier] {
        &self.candidates
    }

    /// 被拒绝的类及原因。
    pub fn rejected_classes(&self) -> &[(Identifier, StackAllocationRejectionReason)] {
        &self.rejected
    }

    /// 是否为候选类。
    pub fn is_candidate(&self, class_name: &Identifier) -> bool {
        self.candidates.iter().any(|name| name == class_name)
    }

    /// 清空状态。
    pub fn clear(&mut self) {
        self.candidates.clear();
        self.rejected.clear();
    }
}

/// 死代码消除分析器。
#[derive(Debug, Default)]
pub struct DeadCodeEliminationAnalyzer {
    eliminable: HashSet<String>,
    unused_methods: HashMap<String, Vec<Identifier>>,
}

impl DeadCodeEliminationAnalyzer {
    /// 创建分析器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 注册类。
    pub fn register(&mut self, class: &HirStruct) {
        if class.is_closed() {
            self.eliminable.insert(class.name.to_string());
        }
    }

    /// 是否可消除死代码。
    pub fn can_eliminate(&self, class_name: &Identifier) -> bool {
        self.eliminable.contains(class_name.as_str())
    }

    /// 记录未使用方法。
    pub fn record_unused_methods(&mut self, class_name: Identifier, methods: Vec<Identifier>) {
        self.unused_methods.insert(class_name.to_string(), methods);
    }

    /// 未使用方法总数。
    pub fn total_unused_count(&self) -> usize {
        self.unused_methods.values().map(Vec::len).sum()
    }

    /// 清空状态。
    pub fn clear(&mut self) {
        self.eliminable.clear();
        self.unused_methods.clear();
    }
}

/// 封闭类优化器入口。
#[derive(Debug, Default)]
pub struct ClosedClassOptimizer {
    method_inline: MethodInlineAnalyzer,
    witness_elimination: WitnessEliminationPass,
    stack_allocator: StackAllocationAnalyzer,
    dead_code: DeadCodeEliminationAnalyzer,
}

impl ClosedClassOptimizer {
    /// 创建优化器。
    pub fn new() -> Self {
        Self::default()
    }

    /// 分析单个类。
    pub fn analyze_class(&mut self, class: &HirStruct) -> OptimizationResult {
        self.method_inline.register_closed_class(class);
        self.dead_code.register(class);
        let optimizations = ClosedClassOptimizations::analyze(class);
        let can_stack_allocate = self.stack_allocator.analyze(class);
        OptimizationResult { class_name: class.name.clone(), optimizations, can_stack_allocate }
    }

    /// 分析多个类。
    pub fn analyze_classes(&mut self, classes: &[HirStruct]) -> Vec<OptimizationResult> {
        classes.iter().map(|class| self.analyze_class(class)).collect()
    }

    /// 运行见证消除 pass。
    pub fn run_witness_elimination(&mut self, classes: &[HirStruct]) -> usize {
        self.witness_elimination.run(classes)
    }

    /// 方法内联分析器。
    pub fn method_inline_analyzer(&self) -> &MethodInlineAnalyzer {
        &self.method_inline
    }

    /// 见证消除 pass。
    pub fn witness_elimination(&self) -> &WitnessEliminationPass {
        &self.witness_elimination
    }

    /// 栈分配分析器。
    pub fn stack_allocator(&self) -> &StackAllocationAnalyzer {
        &self.stack_allocator
    }

    /// 清空全部状态。
    pub fn clear(&mut self) {
        self.method_inline.clear();
        self.witness_elimination.clear();
        self.stack_allocator.clear();
        self.dead_code.clear();
    }
}
