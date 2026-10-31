//! 集合字面量 post-binding 转换：nominal 声明 + 数组字面量由 `LiteralSequence` 合同驱动构造。
//!
//! 验收面：`CollectionInit` 两槽冻结声明身份与调用合同、实例由单态化固定点铸造；
//! 嵌套字面量按元素类型递归构造；空字面量零项；`Array` 声明保持 `ArrayLiteral`；
//! `let` 与 `return`/尾表达式同一期望类型口径；无合同事实的 nominal 声明 + 字面量在构造点结构化拒绝。

use rusty_valkyrie::{
    HirError, ValkyrieCompiler,
    compiler::types::hir::{HirExpr, HirExprKind, HirFunction, HirModule, HirResolvedCall, HirStatementKind, ValkyrieType},
};

/// 合同 + 两个容器类 + 应用函数的完整单源夹具。
const CONTRACT_SOURCE: &str = r#"
namespace std.collection;

trait LiteralSequence<T> {
    micro with_length(n: usize): Self
    micro append(mut self, value: T): unit
}

class ArrayList<T> {
    _items: [T]
    _capacity: usize
}

class LinkedList<T> {
    _items: [T]
}

imply ArrayList<T>: LiteralSequence<T> {
    micro with_length(n: usize): Self {
        return ArrayList { _items: [], _capacity: n }
    }

    micro append(mut self, value: T): unit {
        self._capacity = 0
    }
}

imply LinkedList<T>: LiteralSequence<T> {
    micro with_length(n: usize): Self {
        return LinkedList { _items: [] }
    }

    micro append(mut self, value: T): unit {
        self._items = []
    }
}

micro build() -> ArrayList<u32> {
    let a: ArrayList<u32> = [0, 1, 2]
    return a
}

micro build_linked() -> LinkedList<u32> {
    let b: LinkedList<u32> = [7, 8]
    return b
}

micro build_empty() -> ArrayList<u32> {
    let e: ArrayList<u32> = []
    return e
}

micro build_nested() -> ArrayList<LinkedList<u32>> {
    let m: ArrayList<LinkedList<u32>> = [[0, 1], [2, 3]]
    return m
}

micro build_plain_array() -> [u32] {
    let d: [u32] = [7, 8]
    return d
}

micro return_literal() -> ArrayList<u32> {
    return [9, 10]
}

micro return_plain_array() -> [u32] {
    return [11]
}
"#;

fn compile(source: &str) -> Result<HirModule, HirError> {
    ValkyrieCompiler::default().compile_source(source)
}

fn function<'a>(module: &'a HirModule, name: &str) -> &'a HirFunction {
    module.functions.iter().find(|function| function.name.as_str() == name).unwrap_or_else(|| panic!("{name}"))
}

/// 取函数体第一条 `let` 的初始值表达式。
fn first_let_initializer<'a>(function: &'a HirFunction) -> &'a HirExpr {
    for statement in &function.body.statements {
        if let HirStatementKind::Let { initializer: Some(initializer), .. } = &statement.kind {
            return initializer;
        }
    }
    panic!("函数体必须含 let 绑定");
}

/// 取函数体第一条 `return` 的表达式；`return` 作为无分号尾表达式时位于 `block.expr`。
fn first_return_expr<'a>(function: &'a HirFunction) -> &'a HirExpr {
    fn return_value<'a>(expr: &'a HirExpr) -> Option<&'a HirExpr> {
        if let HirExprKind::Return(Some(inner)) = &expr.kind { Some(inner.as_ref()) } else { None }
    }
    for statement in &function.body.statements {
        if let HirStatementKind::Expr(expr) = &statement.kind {
            if let Some(value) = return_value(expr) {
                return value;
            }
        }
    }
    if let Some(tail) = &function.body.expr {
        if let Some(value) = return_value(tail) {
            return value;
        }
    }
    panic!("函数体必须含 return 表达式");
}

/// 断言初始值是 `CollectionInit`，返回两槽与项列表。
fn expect_collection_init(initializer: &HirExpr) -> (&ValkyrieType, &ValkyrieType, &HirResolvedCall, &HirResolvedCall, &Vec<HirExpr>) {
    match &initializer.kind {
        HirExprKind::CollectionInit { container_type, element_type, with_length, append, items } => {
            (container_type, element_type, with_length, append, items)
        }
        other => panic!("nominal 声明 + 字面量必须转换为 CollectionInit，实际 {other:?}"),
    }
}

/// `let a: ArrayList<u32> = [0, 1, 2]` 构造出两槽声明身份冻结、实例已铸造的 `CollectionInit`。
#[test]
fn collection_literal_builds_collection_init_with_minted_slots() {
    let module = compile(CONTRACT_SOURCE).expect("合同源码必须完成 HIR");
    let initializer = first_let_initializer(function(&module, "build"));
    let (container_type, element_type, with_length, append, items) = expect_collection_init(initializer);
    assert!(container_type.nominal_declaration().is_some(), "容器类型必须是名义类型");
    match element_type {
        ValkyrieType::Named(name) => assert_eq!(name.as_str(), "u32", "元素类型应为 u32"),
        other => panic!("元素类型应为 u32，实际 {other:?}"),
    }
    assert_eq!(items.len(), 3, "字面量项必须逐项保留");
    assert!(with_length.declaration.is_some(), "with_length 槽必须冻结声明身份");
    assert!(with_length.instance.is_some(), "with_length 槽实例必须由单态化固定点铸造");
    assert!(!with_length.has_receiver, "with_length 是静态构造槽");
    assert_eq!(with_length.return_type, *container_type, "with_length 返回容器类型");
    assert!(append.declaration.is_some(), "append 槽必须冻结声明身份");
    assert!(append.instance.is_some(), "append 槽实例必须由单态化固定点铸造");
    assert!(append.has_receiver, "append 是接收者元素槽");
    assert_eq!(append.parameter_types.len(), 2, "append 合同为 (self, value)");
    assert_eq!(append.parameter_types[0], *container_type, "append 接收者类型必须是容器类型");
    assert_eq!(append.parameter_types[1], *element_type, "append 值参数类型必须是元素类型");
    assert_eq!(append.return_type, ValkyrieType::Unit, "append 返回 unit");
}

/// `LinkedList<u32>` 走同一合同事实：按目标类型匹配，两槽携带各自 impl 的声明身份。
#[test]
fn collection_literal_matches_linked_list_target() {
    let module = compile(CONTRACT_SOURCE).expect("合同源码必须完成 HIR");
    let (container_type, _, with_length, append, items) = expect_collection_init(first_let_initializer(function(&module, "build_linked")));
    let (array_container, _, array_with_length, _, _) = expect_collection_init(first_let_initializer(function(&module, "build")));
    assert!(container_type.nominal_declaration().is_some());
    assert_ne!(container_type, array_container, "两个目标的容器类型必须不同");
    assert_eq!(with_length.return_type, *container_type);
    assert_eq!(items.len(), 2);
    assert!(with_length.instance.is_some() && append.instance.is_some(), "两槽实例必须由固定点铸造");
    assert_ne!(with_length.declaration, array_with_length.declaration, "不同目标的合同成员必须携带各自 impl 的声明身份");
}

/// 空字面量 `[]` 构造零项 `CollectionInit`，`with_length(0)` 槽照常冻结。
#[test]
fn empty_collection_literal_builds_zero_items() {
    let module = compile(CONTRACT_SOURCE).expect("合同源码必须完成 HIR");
    let (_, _, with_length, _, items) = expect_collection_init(first_let_initializer(function(&module, "build_empty")));
    assert!(items.is_empty(), "空字面量零项");
    assert!(with_length.declaration.is_some() && with_length.instance.is_some());
}

/// 嵌套字面量按元素类型递归构造：内层是 `LinkedList<u32>` 的 `CollectionInit`。
#[test]
fn nested_collection_literal_builds_inner_containers() {
    let module = compile(CONTRACT_SOURCE).expect("合同源码必须完成 HIR");
    let (_, element_type, _, _, items) = expect_collection_init(first_let_initializer(function(&module, "build_nested")));
    assert_eq!(
        element_type,
        &expect_collection_init(first_let_initializer(function(&module, "build_linked"))).0.clone(),
        "外层元素类型必须等于 LinkedList 容器类型"
    );
    assert_eq!(items.len(), 2, "外层两项");
    for item in items {
        let (inner_container, _, _, append, inner_items) = expect_collection_init(item);
        assert_eq!(inner_items.len(), 2, "内层各两项");
        assert_eq!(inner_container, element_type, "内层容器类型必须与外层元素类型一致");
        assert!(append.instance.is_some(), "内层槽实例同样由固定点铸造");
    }
}

/// `Array`/`[T]` 声明不实现合同：字面量保持 `ArrayLiteral` 原路径。
#[test]
fn array_declared_literal_stays_array_literal() {
    let module = compile(CONTRACT_SOURCE).expect("合同源码必须完成 HIR");
    let initializer = first_let_initializer(function(&module, "build_plain_array"));
    assert!(matches!(&initializer.kind, HirExprKind::ArrayLiteral { items } if items.len() == 2), "非名义声明保持 ArrayLiteral");
}

/// `return [...]` 与 `let` 初始化同一口径：按声明返回类型驱动 `CollectionInit` 构造。
#[test]
fn return_literal_uses_declared_return_type() {
    let module = compile(CONTRACT_SOURCE).expect("合同源码必须完成 HIR");
    let (container_type, _, with_length, append, items) = expect_collection_init(first_return_expr(function(&module, "return_literal")));
    assert!(container_type.nominal_declaration().is_some(), "返回类型驱动的容器必须是名义类型");
    assert_eq!(items.len(), 2, "返回位置字面量项必须逐项保留");
    assert!(with_length.instance.is_some() && append.instance.is_some(), "返回位置两槽实例同样由固定点铸造");
}

/// 返回 `[T]` 时不套用名义合同：字面量保持 `ArrayLiteral`。
#[test]
fn return_plain_array_literal_stays_array_literal() {
    let module = compile(CONTRACT_SOURCE).expect("合同源码必须完成 HIR");
    let expr = first_return_expr(function(&module, "return_plain_array"));
    assert!(matches!(&expr.kind, HirExprKind::ArrayLiteral { items } if items.len() == 1), "数组返回类型保持 ArrayLiteral");
}

/// 无合同事实的 nominal 声明 + 字面量：在构造点结构化拒绝，禁止退化为 Array 隐式转换。
#[test]
fn nominal_literal_without_contract_is_rejected() {
    let source = r#"
class Bag {
    _x: i32
}

micro carry() -> Bag {
    let x: Bag = [1]
    return x
}
"#;
    let error = compile(source).expect_err("无合同的 nominal 声明 + 字面量必须被拒绝");
    match error {
        HirError::CollectionLiteralContractMissing { type_label, span, .. } => {
            // `display_type` 对名义类型输出 `Nominal(ItemId(n))`，不含类型短名；
            // 本用例只验证结构化拒绝的变体、非空标签与非空 span。
            assert!(!type_label.is_empty(), "诊断必须携带声明类型标签");
            assert!(span.end > span.start, "诊断必须携带非空 span");
        }
        other => panic!("必须返回 CollectionLiteralContractMissing，实际 {other:?}"),
    }
}
