use std::collections::BTreeMap;

use crate::types::{
    Identifier, NamePath, SourceID, SourceSpan,
    hir::{
        HirBlock, HirDocumentation, HirExpr, HirExprKind, HirFunction, HirLiteral, HirModule, HirPattern, HirStatement, HirStruct,
        HirVisibility, ValkyrieType,
    },
};

use nyar_types::ItemInstanceId;

use super::{
    AggregateLayoutPlan, MirBlock, MirBuilder, MirConstant, MirDiagnostic, MirFunction, MirInstruction, MirModule, MirOperand, MirOperation,
    MirSuspendPoint, MirValue, MirValueOrigin, MirValueRef, generic_call_instantiation::MirDispatchEntry, lower_function_semantic, lower_literal,
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
                AggregateLayoutPlan::default(),
                BTreeMap::new(),
                BTreeMap::new(),
                super::CallableReturnTypes { by_symbol: BTreeMap::new(), by_instance: BTreeMap::new() },
                Vec::new(),
                None,
                BTreeMap::new(),
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

    /// 降低整数/常量相等比较谓词，便于测试未绑定 callable 的失败形状。
    pub fn lower_eq_constant_operand(&mut self, lhs: MirOperand, rhs: MirConstant, ty: &ValkyrieType) -> MirOperand {
        self.inner.lower_eq_constant_operand(lhs, rhs, ty)
    }

    /// 降低范围比较所需的 `<` 谓词。
    pub fn lower_lt_operands(&mut self, lhs: MirOperand, rhs: MirOperand, ty: &ValkyrieType) -> MirOperand {
        self.inner.lower_lt_operands(lhs, rhs, ty)
    }

    /// 登记成员分派表项，供模式比较测试使用。
    pub fn register_member_dispatch(&mut self, target: ValkyrieType, member: &str, instance: ItemInstanceId) {
        self.inner.member_dispatch.push(MirDispatchEntry {
            trait_declaration: None,
            target,
            member: Identifier::new(member),
            instance,
        });
    }

    /// 为 SSA 值操作数登记类型事实。
    pub fn register_operand_type(&mut self, operand: MirOperand, ty: ValkyrieType) {
        if let MirOperand::Value(value) = operand {
            self.inner.value_types.insert(value, ty);
        }
    }

    /// 分配带类型的 SSA 值操作数。
    pub fn typed_value_operand(&mut self, ty: ValkyrieType) -> MirOperand {
        let value = self.inner.next_value(MirValueOrigin::Temporary);
        self.inner.value_types.insert(value, ty);
        MirOperand::Value(value)
    }

    /// 收集已发射的 `Call` 指令的 callee（含已刷入基本块的指令）。
    pub fn call_callees(&self) -> Vec<MirOperand> {
        let mut callees = self
            .inner
            .instructions
            .iter()
            .filter_map(|instruction| match &instruction.kind {
                MirOperation::Call { callee, .. } => Some(callee.clone()),
                _ => None,
            })
            .collect::<Vec<_>>();
        for block in &self.inner.blocks {
            for instruction in &block.instructions {
                if let MirOperation::Call { callee, .. } = &instruction.kind {
                    callees.push(callee.clone());
                }
            }
        }
        callees
    }

    /// 读取 lowering 阶段累计的诊断。
    pub fn diagnostics(&self) -> &[MirDiagnostic] {
        &self.inner.diagnostics
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
        declaration: None,
        instance: None,
        name: Identifier::new("main"),
        where_constraints: Vec::new(),
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
        dependency_key: "demo".into(),
        source_path: "demo.v".into(),
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
    let func = lower_function_semantic(
        &module,
        &function,
        &mut AggregateLayoutPlan::default(),
        &BTreeMap::new(),
        &super::CallableReturnTypes { by_symbol: BTreeMap::new(), by_instance: BTreeMap::new() },
        &[],
        None,
        Vec::new(),
        &BTreeMap::new(),
        &mut Vec::new(),
    );
    func
}

/// 构造一个最小模块并降低为 `MIR`。
pub fn lower_test_module(functions: Vec<HirFunction>, structs: Vec<HirStruct>) -> MirModule {
    super::MirLowerer::lower_module_semantic(&HirModule {
        name: NamePath::new(vec![Identifier::new("demo")]),
        dependency_key: "demo".into(),
        source_path: "demo.v".into(),
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
