use nyar_language::{ValkyrieCompiler, types::SourceID};

#[test]
fn generic_static_call_uses_declared_trait_contract() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4301 });
    compiler.compile_source(r#"
trait Factory {
    micro build(value: bool) -> Self
}
micro invoke<C>(value: bool) -> C where C: Factory {
    return C::build(value);
}
"#).expect("泛型静态调用必须沿声明的 trait 合同解析");
}

#[test]
fn generic_call_rejects_unsatisfied_declared_trait_bound() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4300 });
    let result = compiler.compile_source(r#"
trait Required {}
micro constrained<T>(value: T) -> T where T: Required { return value; }
micro invoke(value: bool) -> bool { return constrained(value); }
"#);
    assert!(result.is_err(), "缺少 Required evidence 的调用不得成为前端成功结果");
}

#[test]
fn generic_static_call_preserves_associated_equations() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4302 });
    let hir = compiler.compile_source(r#"
trait Iterator { type Item; }
trait FromIterator {
    type Item;
    micro from_iterator<I>(item: Self::Item, iter: I) -> Self
        where I: Iterator<Item = Self::Item>
}

micro collect<I, T, C>(item: T, self: I) -> C
    where I: Iterator<Item = T>, C: FromIterator<Item = T>
{
    return C::from_iterator(item, self);
}
"#).expect("关联类型等式必须贯穿泛型静态调用");
    let collect = hir.functions.iter().find(|function| function.name.as_str() == "collect").expect("collect HIR");
    let resolved = collect.body.statements.iter().find_map(|statement| match &statement.kind {
        nyar_language::types::hir::HirStatementKind::Expr(expression) => match &expression.kind {
            nyar_language::types::hir::HirExprKind::Return(Some(value)) => match &value.kind {
                nyar_language::types::hir::HirExprKind::Call { resolved: Some(resolved), .. } => Some(resolved),
                _ => None,
            },
            _ => None,
        },
        _ => None,
    }).expect("the static trait call must have a resolved HIR contract");
    assert_eq!(resolved.symbol.to_string(), "FromIterator.from_iterator");
    assert_eq!(resolved.parameter_types.len(), 2);
    assert_eq!(resolved.parameter_types[0], nyar_language::types::hir::ValkyrieType::Named(nyar_language::types::Identifier::new("T")));
}

#[test]
fn trait_associated_result_binds_array_intrinsic_argument() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4303 });
    let base = r#"
unite Option<T> { Some { value: T }, None }
imply Option<T> {
    micro unwrap(self) -> T { match self { case Some(value): value case None: panic("none") } }
}

trait Iterator { type Item; micro has_next(self) -> bool; micro next(self) -> Option<Item>; }
micro collect_array<I, T>(self: I) -> [T]
    where I: Iterator<Item = T>
{
    let mut result: [T] = []
    let mut iter: I = self
    while iter.has_next() {
        result = VALUE
    }
    return result
}

"#;

    for value in ["result", "iter.next()", "iter.next().unwrap()", "push(result, iter.next().unwrap())"] {
        let source = base.replace("VALUE", value);
        let hir = compiler
            .compile_source(&source)
            .unwrap_or_else(|error| panic!("调用 `{value}` 未形成完整合同: {error:?}"));
        let iterator = hir.traits.iter().find(|item| item.name.as_str() == "Iterator").expect("Iterator 声明");
        let next = iterator.methods.iter().find(|method| method.name.as_str() == "next").expect("next 声明");
        let nyar_language::types::hir::ValkyrieType::Apply(_, arguments) = &next.return_type else {
            panic!("Option<Item> 必须保留 nominal 应用")
        };
        let nyar_language::types::hir::ValkyrieType::Associated(associated) = &arguments[0] else {
            panic!("trait 关联结果不得退化为普通 Named 类型")
        };
        assert_eq!(associated.base, nyar_language::types::hir::ValkyrieType::SelfType);
        assert_eq!(associated.name.as_str(), "Item");
    }
}

#[test]
fn namespaced_function_preserves_where_contract() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4308 });
    let hir = compiler
        .compile_source(r#"
namespace std.iterator;
trait Iterator { type Item; micro next(self) -> Option<Item>; }
micro collect_array<I, T>(self: I) -> bool
    where I: Iterator<Item = T>
{
    return true;
}
"#)
        .expect("带 namespace 的函数必须保留 where 合同");
    let function = hir.functions.iter().find(|function| function.name.as_str() == "collect_array").expect("collect_array HIR");
    assert_eq!(function.where_constraints.len(), 1);
}

#[test]
fn concatenated_namespace_preserves_where_contract() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4309 });
    let hir = compiler
        .compile_source(r#"
namespace std.iterator;
trait Iterator { type Item; micro next(self) -> Option<Item>; }

namespace std.iterator;
micro collect_array<I, T>(self: I) -> bool
    where I: Iterator<Item = T>
{
    return true;
}
"#)
        .expect("拼接后的重复 namespace 必须保留 where 合同");
    let function = hir.functions.iter().find(|function| function.name.as_str() == "collect_array").expect("collect_array HIR");
    assert_eq!(function.where_constraints.len(), 1);
}

#[test]
fn imported_impl_method_contract_survives_dependency_boundary() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 4307 });
    let provider = compiler.compile_source(r#"
namespace core.types;
unite Option<T> { Some { value: T }, None }
imply Option<T> {
    micro unwrap(self) -> T { match self { case Some(value): value case None: panic("none") } }
}
"#).expect("provider");
    let export = nyar_language::types::hir::HirDependencySemanticExport {
        module: nyar_language::types::NamePath::new(vec![nyar_language::types::Identifier::new("core.types")]),
        functions: provider.functions.clone(),
        structs: provider.structs.clone(),
        enums: provider.enums.clone(),
        traits: provider.traits.clone(),
        type_aliases: provider.type_aliases.clone(),
        impls: provider.impls.clone(),
    };
    compiler.compile_source_with_semantic_exports(r#"
micro consume(value: Option<i32>) -> i32 { return value.unwrap() }
"#, &[export]).expect("imported impl method must resolve from its exported contract");
}
