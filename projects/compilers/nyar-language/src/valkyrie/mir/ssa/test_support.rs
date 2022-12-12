use std::collections::BTreeMap;

use crate::types::{
    Identifier, NamePath, SourceID, SourceSpan,
    hir::{
        HirBlock, HirDocumentation, HirExpr, HirExprKind, HirFunction, HirLiteral, HirModule, HirPattern, HirStatement, HirStruct,
        HirVisibility, ValkyrieType,
    },
};

use super::{
    AggregateLayoutPlan, MirBlock, MirBuilder, MirConstant, MirFunction, MirInstruction, MirModule, MirOperand, MirSuspendPoint, MirValue,
    MirValueRef, lower_function, lower_literal,
};

/// `MIR` 集成测试使用的只读构建器包装。
pub struct TestMirBuilder {
    inner: MirBuilder,
}

impl Default for TestMirBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl TestMirBuilder {
    /// 创建不带额外类型上下文的测试构建器。
    pub fn new() -> Self {
        Self {
            inner: MirBuilder::new(
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
                AggregateLayoutPlan::default(),
                BTreeMap::new(),
                BTreeMap::new(),
                BTreeMap::new(),
                None,
            ),
        }
    }

    /// 降低模式匹配谓词并返回匹配结果操作数。
    pub fn lower_pattern_match_operand(&mut self, pattern: &HirPattern, value: MirOperand) -> MirOperand {
        self.inner.lower_pattern_match_operand(pattern, value)
    }

    /// 降低表达式，便于测试恢复点与 handler 形状。
    pub fn lower_expr_to_operand(&mut self, expr: &HirExpr) -> MirOperand {
        self.inner.lower_expr_to_operand(expr)
    }

    /// 降低单条语句，便于测试类型回灌与局部绑定。
    pub fn lower_statement(&mut self, statement: &HirStatement) {
        self.inner.lower_statement(statement);
    }

    /// 读取当前已累计的指令缓冲。
    pub fn instructions(&self) -> &[MirInstruction] {
        &self.inner.instructions
    }

    /// 读取当前已创建的基本块。
    pub fn blocks(&self) -> &[MirBlock] {
        &self.inner.blocks
    }

    /// 读取已登记的值槽。
    pub fn values(&self) -> &[MirValue] {
        &self.inner.values
    }

    /// 读取已登记的 suspend 点元数据。
    pub fn suspend_points(&self) -> &[MirSuspendPoint] {
        &self.inner.suspend_points
    }

    /// 读取测试过程中的静态值类型表。
    pub fn value_types(&self) -> &BTreeMap<MirValueRef, ValkyrieType> {
        &self.inner.value_types
    }
}

/// 创建统一的测试 `span`。
pub fn span() -> SourceSpan {
    SourceSpan::new(SourceID::default(), 0, 0)
}

/// 创建统一的测试表达式。
pub fn expr(kind: HirExprKind) -> HirExpr {
    HirExpr { kind, span: span() }
}

/// 创建统一的测试代码块。
pub fn block(statements: Vec<HirStatement>, expr: Option<HirExpr>) -> HirBlock {
    HirBlock { statements, expr: expr.map(Box::new), span: span() }
}

/// 降低单个字面量，便于验证常量承载形状。
pub fn lower_test_literal(literal: &HirLiteral, expected_type: Option<&ValkyrieType>) -> (MirConstant, Option<ValkyrieType>) {
    lower_literal(literal, expected_type)
}

/// 构造一个最小函数并降低为 `MIR`。
pub fn lower_test_function(expr: HirExpr) -> MirFunction {
    let function = HirFunction {
        name: Identifier::new("main"),
        declaring_namespace: NamePath::default(),
        doc: HirDocumentation::default(),
        annotations: Vec::new(),
        generics: Vec::new(),
        params: Vec::new(),
        return_type: ValkyrieType::Boolean,
        body: block(Vec::new(), Some(expr)),
        span: span(),
        visibility: HirVisibility::default(),
        is_abstract: false,
        is_final: false,
        is_virtual: false,
        is_override: false,
    };
    let module = HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions: vec![function.clone()],
        structs: Vec::new(),
        enums: Vec::new(),
        imported_enums: Vec::new(),
        imported_semantic_exports: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    };
    lower_function(
        &module,
        &function,
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        &mut AggregateLayoutPlan::default(),
        &BTreeMap::new(),
        &BTreeMap::new(),
        None,
    )
}

/// 构造一个最小模块并降低为 `MIR`。
pub fn lower_test_module(functions: Vec<HirFunction>, structs: Vec<HirStruct>) -> MirModule {
    super::MirLowerer::lower_module(&HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        doc: HirDocumentation::default(),
        imports: Vec::new(),
        warnings: Vec::new(),
        submodules: Vec::new(),
        functions,
        structs,
        enums: Vec::new(),
        imported_enums: Vec::new(),
        imported_semantic_exports: Vec::new(),
        flags: Vec::new(),
        traits: Vec::new(),
        impls: Vec::new(),
        type_functions: Vec::new(),
        type_families: Vec::new(),
        widgets: Vec::new(),
        singletons: Vec::new(),
        statements: Vec::new(),
        type_aliases: Vec::new(),
    })
}
