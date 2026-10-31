//! 按调用点的泛型 callable 单态化（instantiation 固定点）。
//!
//! 阶段位置：全部源码组的 `resolve_hir_calls` 完成之后，`validate_hir_semantic_contract`
//! 与 MIR lowering 之前。声明期开放签名不再分配 `ItemInstanceId`（见
//! `register_function_declarations`）；调用点的具体代入在这里被冻结成真正的 callable
//! 实例：克隆声明体、代入签名/局部/聚合类型，并递归处理体内调用，直到不再产生新实例。
//!
//! 唯一事实所有者是 frontend：禁止在 MIR/emitter 事后补做后置物化，禁止按短名特判，
//! 也禁止把开放实例擦成 `anyref`。调用点无法闭合时保持 `instance` 为空，
//! 由既有 `UnresolvedCallableIdentity` 结构化诊断失败。
//!
//! 泛型 trait 分派先以 witness（`HirTraitDispatch`）保留事实；当 `self` 类型在代入后
//! 完全具体化、且声明确实归属该 trait 时，本模块把它消去为真实实现实例；否则继续
//! 保留 witness，让后端按冻结的 (声明, self 类型) 精确失败，禁止在那里猜实现。

use std::collections::{BTreeMap, BTreeSet};

use crate::compiler::{
    hir::overload::{has_unresolved_generic_arguments, owner_type_generic_binders, substitute_type_self},
    types::hir::{
        AssociatedType, FunctionType, GenericBinderId, HirBlock, HirExpr, HirExprKind, HirFunction, HirFunctionReference, HirInstantiatedCallable,
        HirModule, HirParam, HirResolvedCall, HirStatementKind, HirTraitDispatch, HirWhereConstraint, ValkyrieType,
        export_spec::parse_library_glue_operation, parse_export_spec_from_annotations,
    },
};

/// 未实例化声明的快照：声明体、owner 类型与 trait 归属都不再依赖模块借用。
#[derive(Debug, Clone)]
struct CallableDeclaration {
    /// 声明所在模块在本次闭包里的下标。
    module: usize,
    /// 声明自身的已冻结下标。
    index: u32,
    /// 声明原样（`instance` 为空，签名与体内仍含 binder）。
    function: HirFunction,
    /// impl owner 类型（含 owner 泛型 binder）；自由函数为空。
    owner_type: Option<ValkyrieType>,
    /// impl 块的 trait 声明身份；inherent impl 为空。
    trait_declaration: Option<nyar_types::ItemId>,
}

/// `GenericBinderId` 到具体类型的代入。`BTreeMap` 保证 `(声明, 代入)` 的稳定序。
type Substitution = BTreeMap<GenericBinderId, ValkyrieType>;

/// 固定点需要闭合的可调用请求点：已解析调用合同，或具名函数作值的引用。
enum CallableRequest<'a> {
    /// 已解析调用合同。
    Call(&'a mut HirResolvedCall),
    /// 具名函数作值引用；只有非泛型声明会被改写生成。
    Reference(&'a mut HirFunctionReference),
}

/// 把仍指向 trait 方法槽的 witness 换成具体 impl 成员的 (声明下标, 代入)。
///
/// witness 冻结的是「trait 方法槽 + 接收者静态类型」。`self` 类型具体化后，哪个
/// impl 提供该成员只能由 (trait 声明, 成员名, impl owner 与接收者实例同一, 成员签名
/// 统一) 共同决定，且候选必须唯一。owner 同一不能省：静态 trait 方法的
/// `parameter_types` 不含接收者，签名统一本身约束不到 owner，只按成员名 + 签名
/// 匹配会让同名成员跨 owner 命中。任何一项不成立都保持 witness，让后端按冻结身份
/// 失败；不得回退首个候选或按名字猜一个「看起来合适」的实现（`Hash::hash` 被猜成
/// `i64.infix -` 就是这么来的）。
fn discharge_trait_witness(
    declarations: &BTreeMap<u32, CallableDeclaration>,
    trait_members: &BTreeMap<u32, String>,
    call: &HirResolvedCall,
) -> Option<(u32, Substitution)> {
    let dispatch = call.trait_dispatch.as_ref()?;
    if has_unresolved_generic_arguments(&dispatch.self_type) {
        return None;
    }
    let member_name = trait_members.get(&dispatch.method_declaration.index())?;
    let mut matched: Option<(u32, Substitution)> = None;
    for declaration in declarations.values() {
        if declaration.trait_declaration != Some(dispatch.trait_declaration) {
            continue;
        }
        if declaration.function.name.as_str() != member_name.as_str() {
            continue;
        }
        let Some(substitution) = derive_witness_substitution(declaration, call)
        else {
            continue;
        };
        if matched.is_some() {
            return None;
        }
        matched = Some((declaration.index, substitution));
    }
    matched
}

/// witness 候选的精确代入：impl owner 必须与 witness 冻结的接收者实例结构化同一，
/// 成员签名必须与调用合同逐项等长统一，二者共用一张代入表以避免各自自洽。
///
/// 不做等长检查时，形参个数不同的候选会退化成「只按返回类型匹配」，这正是跨 owner
/// 错选 witness 的来源；因此这里对不等长候选直接判不合格。
fn derive_witness_substitution(declaration: &CallableDeclaration, call: &HirResolvedCall) -> Option<Substitution> {
    let dispatch = call.trait_dispatch.as_ref()?;
    let owner = declaration.owner_type.as_ref()?;
    let mut substitution = Substitution::new();
    if !unify_instantiation(owner, &dispatch.self_type, &mut substitution) {
        return None;
    }
    // owner 的每个 binder 都必须被接收者实例固定；否则该 impl 在实例级并不成立。
    if owner_type_generic_binders(owner).iter().any(|binder| !substitution.contains_key(binder)) {
        return None;
    }
    let declared_params =
        declaration.function.params.iter().enumerate().map(|(index, param)| declared_param_type(param, index, Some(owner))).collect::<Vec<_>>();
    if declared_params.len() != call.parameter_types.len() {
        return None;
    }
    for (declared, actual) in declared_params.iter().zip(&call.parameter_types) {
        if !unify_instantiation(declared, actual, &mut substitution) {
            return None;
        }
    }
    let declared_return = declared_type(&declaration.function.return_type, Some(owner));
    if !unify_instantiation(&declared_return, &call.return_type, &mut substitution) {
        return None;
    }
    if substitution.values().any(has_unresolved_generic_arguments) {
        return None;
    }
    Some(substitution)
}

/// 从完整源码闭包出发，把开放泛型 impl 成员的调用点闭合成真实 callable 实例。
///
/// 实例下标从闭包内已冻结的最大 `ItemInstanceId` 续号，顺序只取决于
/// `(声明下标, 代入)` 的字典序，因此同一输入恒产生同一实例空间。
pub(crate) fn instantiate_generic_callables(modules: &mut [HirModule]) {
    let (declarations, trait_members) = collect_impl_declarations(modules);
    if declarations.is_empty() {
        return;
    }
    let mut next_instance = modules.iter().filter_map(super::max_hir_callable_instance_index).max().map_or(0, |index| index.saturating_add(1));
    let mut instances: BTreeMap<(u32, Substitution), nyar_types::ItemInstanceId> = BTreeMap::new();
    loop {
        // 1. 收集仍指向开放声明的调用点。
        let mut requests: BTreeSet<(u32, Substitution)> = BTreeSet::new();
        for module in modules.iter_mut() {
            visit_module_calls(module, &mut |request| {
                let call = match request {
                    CallableRequest::Call(call) => call,
                    // 具名函数作值：非泛型声明按空代入照常铸造实例；
                    // 声明不在闭包表里（如抽象无泛型 intrinsic）保持空，由后端 fail-closed。
                    CallableRequest::Reference(reference) => {
                        let Some(index) = reference.declaration.map(|declaration| declaration.index())
                        else {
                            return;
                        };
                        if declarations.get(&index).is_some_and(|declaration| declaration.function.generics.is_empty()) {
                            requests.insert((index, Substitution::new()));
                        }
                        return;
                    }
                };
                let Some(index) = call.declaration.map(|declaration| declaration.index())
                else {
                    return;
                };
                // 非泛型 callable 的身份在声明期已冻结成唯一实例，固定点不得再铸造重复
                // 实例；泛型声明的声明期种子只是占位，调用点的具体代入仍必须铸造真实例，
                // 并在回填时覆盖它。witness 调用的 `instance` 恒为空，不受影响。
                if call.instance.is_some()
                    && declarations.get(&index).is_some_and(|declaration| declaration.function.generics.is_empty())
                {
                    return;
                }
                // witness 的 declaration 是 trait 方法槽，不在 impl 成员表里；
                // 必须先换成具体 impl 成员，再按 (声明下标, 代入) 铸造实例。
                if call.trait_dispatch.is_some() {
                    let Some((index, substitution)) = discharge_trait_witness(&declarations, &trait_members, call)
                    else {
                        return;
                    };
                    let Some(declaration) = declarations.get(&index)
                    else {
                        return;
                    };
                    if !substitution.is_empty() || declaration.function.generics.is_empty() {
                        requests.insert((index, substitution));
                    }
                    return;
                }
                let Some(declaration) = declarations.get(&index)
                else {
                    return;
                };
                if let Some(substitution) = derive_substitution(declaration, call) {
                    // 有泛型参数的函数：空 substitution 说明调用侧 binder 和声明侧是同一个
                    // （比如 imply 方法内部自调用），没有替换就不需要铸造实例。
                    // 无泛型函数：空 substitution 是合法的（所有类型本就具体），照常铸造。
                    if !substitution.is_empty() || declaration.function.generics.is_empty() {
                        requests.insert((index, substitution));
                    }
                }
            });
        }
        // 2. 导出面种子：导出函数签名里的具体聚合实例，必须带上该聚合的全部
        //    `[library_glue]` 成员；这些成员可能没有任何调用点。
        seed_export_surface_requests(modules, &declarations, &mut requests);
        if requests.is_empty() {
            break;
        }
        // 3. 按稳定序铸造新实例。
        let before = instances.len();
        for (index, substitution) in &requests {
            if instances.contains_key(&(*index, substitution.clone())) {
                continue;
            }
            let Some(declaration) = declarations.get(index)
            else {
                continue;
            };
            let Some(instance) = nyar_types::ItemInstanceId::from_index(next_instance)
            else {
                continue;
            };
            next_instance = next_instance.saturating_add(1);
            let callable = instantiate_callable(declaration, substitution, instance);
            instances.insert((*index, substitution.clone()), instance);
            modules[declaration.module].instantiated_callables.push(callable);
        }
        // 4. 回填调用点。即使本轮没有新实例也必须回填：克隆体里的重复代入
        //    只能指向已存在的实例，不能永远悬空。
        for module in modules.iter_mut() {
            visit_module_calls(module, &mut |request| {
                let call = match request {
                    CallableRequest::Call(call) => call,
                    CallableRequest::Reference(reference) => {
                        let Some(index) = reference.declaration.map(|declaration| declaration.index())
                        else {
                            return;
                        };
                        if let Some(instance) = instances.get(&(index, Substitution::new())) {
                            reference.instance = Some(*instance);
                        }
                        return;
                    }
                };
                let Some(index) = call.declaration.map(|declaration| declaration.index())
                else {
                    return;
                };
                // 回填同样不得覆盖非泛型声明的冻结实例：与收集步保持同一守卫；
                // 泛型声明的种子实例必须让位给具体代入铸造出的实例。
                if call.instance.is_some()
                    && declarations.get(&index).is_some_and(|declaration| declaration.function.generics.is_empty())
                {
                    return;
                }
                if call.trait_dispatch.is_some() {
                    let Some((index, substitution)) = discharge_trait_witness(&declarations, &trait_members, call)
                    else {
                        return;
                    };
                    if let Some(instance) = instances.get(&(index, substitution)) {
                        // witness 义务已由具体 impl 实例偿清：继续把 trait evidence 传到后端
                        // 只会诱发「按签名挑一个实现」的后置猜测。
                        call.instance = Some(*instance);
                        call.trait_dispatch = None;
                    }
                    return;
                }
                let Some(declaration) = declarations.get(&index)
                else {
                    return;
                };
                let Some(substitution) = derive_substitution(declaration, call)
                else {
                    return;
                };
                if let Some(instance) = instances.get(&(index, substitution)) {
                    call.instance = Some(*instance);
                }
            });
        }
        if instances.len() == before {
            break;
        }
    }
}

/// 导出面种子：导出函数签名里出现了哪个具体聚合实例，就必须实例化该聚合声明的
/// 全部 `[library_glue]` 成员。
///
/// 这些成员由宿主 glue 按导出名直通调用，函数体里可能没有任何调用点，只靠调用点
/// 固定点永远到不了。归属仍只由「声明标注 + owner 类型合一」决定，不按成员名猜。
/// 幂等：同一 (声明, 代入) 重复插入由 `BTreeSet` 与实例表去重。
fn seed_export_surface_requests(
    modules: &[HirModule],
    declarations: &BTreeMap<u32, CallableDeclaration>,
    requests: &mut BTreeSet<(u32, Substitution)>,
) {
    let mut concretes = BTreeSet::new();
    for module in modules {
        collect_exported_concrete_nominals(module, &mut concretes);
    }
    if concretes.is_empty() {
        return;
    }
    for declaration in declarations.values() {
        // 只有声明标注给出角色的成员才是宿主直通操作。
        if parse_library_glue_operation(&declaration.function.annotations).is_none() {
            continue;
        }
        let Some(owner) = declaration.owner_type.as_ref()
        else {
            continue;
        };
        // 无泛型的聚合成员由声明本身闭合，不需要按导出面铸造实例。
        if !has_unresolved_generic_arguments(owner) {
            continue;
        }
        for concrete in &concretes {
            let mut substitution = Substitution::new();
            if !unify_instantiation(owner, concrete, &mut substitution) {
                continue;
            }
            // 代入必须闭合：owner 的每个 binder 都要落到具体类型。
            if owner_type_generic_binders(owner).iter().any(|binder| !substitution.contains_key(binder)) {
                continue;
            }
            if substitution.values().any(has_unresolved_generic_arguments) {
                continue;
            }
            requests.insert((declaration.index, substitution));
        }
    }
}

/// 收集导出函数签名里出现的完全具体名义实例；含嵌套实参（`ArrayList<ArrayList<i64>>` 收两层）。
fn collect_exported_concrete_nominals(module: &HirModule, out: &mut BTreeSet<ValkyrieType>) {
    for function in &module.functions {
        // 只有已实例化的导出函数才代表对外签名。
        if function.instance.is_none() || parse_export_spec_from_annotations(&function.annotations).is_none() {
            continue;
        }
        for param in &function.params {
            collect_concrete_nominals(&param.ty, out);
        }
        collect_concrete_nominals(&function.return_type, out);
    }
    for submodule in &module.submodules {
        collect_exported_concrete_nominals(submodule, out);
    }
}

/// 递归收集类型里所有完全具体的名义实例；仍含开放 binder 的形状一律跳过。
fn collect_concrete_nominals(ty: &ValkyrieType, out: &mut BTreeSet<ValkyrieType>) {
    match ty {
        ValkyrieType::Nominal { arguments, .. } => {
            if !arguments.is_empty() && !has_unresolved_generic_arguments(ty) {
                out.insert(ty.clone());
            }
            for argument in arguments {
                collect_concrete_nominals(argument, out);
            }
        }
        ValkyrieType::Apply(base, arguments) => {
            collect_concrete_nominals(base, out);
            for argument in arguments {
                collect_concrete_nominals(argument, out);
            }
        }
        ValkyrieType::Array(element) | ValkyrieType::Nullable(element) => collect_concrete_nominals(element, out),
        ValkyrieType::FixedArray { element, .. } => collect_concrete_nominals(element, out),
        ValkyrieType::Tuple(items) | ValkyrieType::Union(items) | ValkyrieType::Intersection(items) => {
            for item in items {
                collect_concrete_nominals(item, out);
            }
        }
        ValkyrieType::Function(function) => {
            for param in &function.params {
                collect_concrete_nominals(param, out);
            }
            collect_concrete_nominals(&function.return_type, out);
        }
        _ => {}
    }
}

/// 收集所有有 declaration 的 callable 声明——包含 impl 成员和 module-level free function。
/// 不再过滤 `instance`：lowering 阶段就给每个声明分配了一个 ItemInstanceId（等于 ItemId），
/// 但那个 instance 只是"声明身份"，不是特化实例。泛型函数的特化版本需要 generic_instantiation
/// 在这里铸造新的 ItemInstanceId。因此只要有 declaration 就收集。
fn collect_impl_declarations(modules: &[HirModule]) -> (BTreeMap<u32, CallableDeclaration>, BTreeMap<u32, String>) {
    let mut declarations = BTreeMap::new();
    // trait 方法槽声明 -> 成员名：witness 只冻结槽身份，成员名在这里恢复。
    let mut trait_members = BTreeMap::new();
    for (module_index, module) in modules.iter().enumerate() {
        for trait_definition in &module.traits {
            for method in &trait_definition.methods {
                if let Some(declaration) = method.declaration {
                    trait_members.entry(declaration.index()).or_insert_with(|| method.name.to_string());
                }
            }
        }
        // impl 成员方法（owner_type 为 impl 目标类型）
        for implementation in &module.impls {
            for method in &implementation.methods {
                // is_abstract 的无泛型函数（如 __i32_eq host_contract）不需要实例化。
                // 但有泛型的 intrinsic（如 __array_get<T>）需要实例化，因为泛型实参由调用点决定。
                if method.is_abstract && method.generics.is_empty() {
                    continue;
                }
                let Some(declaration) = method.declaration
                else {
                    continue;
                };
                declarations.entry(declaration.index()).or_insert_with(|| CallableDeclaration {
                    module: module_index,
                    index: declaration.index(),
                    function: method.clone(),
                    owner_type: Some(implementation.target.clone()),
                    trait_declaration: implementation.trait_declaration,
                });
            }
        }
        // module-level free function（无 owner_type，无 trait）
        for function in &module.functions {
            // is_abstract 的无泛型函数（如 __i32_eq host_contract）不需要实例化。
            // 但有泛型的 intrinsic（如 __array_get<T>）需要实例化，因为泛型实参由调用点决定。
            if function.is_abstract && function.generics.is_empty() {
                continue;
            }
            let Some(declaration) = function.declaration
            else {
                continue;
            };
            declarations.entry(declaration.index()).or_insert_with(|| CallableDeclaration {
                module: module_index,
                index: declaration.index(),
                function: function.clone(),
                owner_type: None,
                trait_declaration: None,
            });
        }
    }
    (declarations, trait_members)
}

/// 声明侧形参类型：`self` 槽的 `AutoType`/`Self` 先落到 owner，再交给调用点统一。
fn declared_param_type(param: &HirParam, index: usize, owner: Option<&ValkyrieType>) -> ValkyrieType {
    let declared = match (&param.ty, index, owner) {
        (ValkyrieType::AutoType | ValkyrieType::r#SelfType, 0, Some(owner)) => owner.clone(),
        (ty, _, _) => ty.clone(),
    };
    declared_type(&declared, owner)
}

/// 声明侧 `Self` 代入 owner；owner 为空时原样返回。
fn declared_type(ty: &ValkyrieType, owner: Option<&ValkyrieType>) -> ValkyrieType {
    match owner {
        Some(owner) => substitute_type_self(ty, owner),
        None => ty.clone(),
    }
}

/// 用开放签名与调用点已特化合同反推 binder→具体类型。
///
/// 只有代入后签名完全闭合（不含未代入 binder）才返回；否则调用点保持开放，
/// 由后续固定点轮次（或 `UnresolvedCallableIdentity`）处理。
fn derive_substitution(declaration: &CallableDeclaration, call: &HirResolvedCall) -> Option<Substitution> {
    let owner = declaration.owner_type.as_ref();
    let declared_params =
        declaration.function.params.iter().enumerate().map(|(index, param)| declared_param_type(param, index, owner)).collect::<Vec<_>>();
    let declared_return = declared_type(&declaration.function.return_type, owner);
    let mut substitution = Substitution::new();
    if declared_params.len() == call.parameter_types.len() {
        for (declared, actual) in declared_params.iter().zip(&call.parameter_types) {
            if !unify_instantiation(declared, actual, &mut substitution) {
                return None;
            }
        }
    }
    if !unify_instantiation(&declared_return, &call.return_type, &mut substitution) {
        return None;
    }
    // 不再检查 substitution 是否为空。空 substitution 有两种合法情况：
    // (a) 声明侧和调用侧共享同一个 binder（同结构泛型调用，不需要跨 binder 替换）；
    // (b) 声明本身就不是泛型，所有类型完全具体。
    // 也不再检查 substitution.values() 是否闭合——跨结构委托调用第一次只会把声明侧 binder
    // 映射到调用方的 binder，后续轮次会继续闭合。只要 unify 全通过，就接受。
    Some(substitution)
}

/// 结构化统一：只认已冻结的 binder 与名义声明身份，不做短名匹配。
pub(crate) fn unify_instantiation(template: &ValkyrieType, concrete: &ValkyrieType, substitution: &mut Substitution) -> bool {
    match template {
        ValkyrieType::Generic(generic) => {
            let Some(binder) = generic.binder
            else {
                return template == concrete;
            };
            match substitution.get(&binder) {
                Some(bound) => bound == concrete,
                None => {
                    // 自引用跳过：template binder == concrete binder 时不插入，避免 sub[X]=Generic(X) 循环。
                    // 同结构泛型调用（如 Array<T> → Array<T>）两边 binder 相同，空 substitution 是合法中间态。
                    if let ValkyrieType::Generic(other) = concrete {
                        if other.binder == Some(binder) {
                            return true;
                        }
                    }
                    substitution.insert(binder, concrete.clone());
                    true
                }
            }
        }
        ValkyrieType::Nominal { declaration, arguments } => match concrete {
            ValkyrieType::Nominal { declaration: actual, arguments: actual_arguments }
                if declaration == actual && arguments.len() == actual_arguments.len() =>
            {
                arguments.iter().zip(actual_arguments).all(|(template, concrete)| unify_instantiation(template, concrete, substitution))
            }
            _ => false,
        },
        ValkyrieType::Apply(base, arguments) => match concrete {
            ValkyrieType::Apply(actual, actual_arguments) if arguments.len() == actual_arguments.len() => {
                unify_instantiation(base, actual, substitution)
                    && arguments.iter().zip(actual_arguments).all(|(template, concrete)| unify_instantiation(template, concrete, substitution))
            }
            _ => false,
        },
        ValkyrieType::Array(element) => match concrete {
            ValkyrieType::Array(actual) => unify_instantiation(element, actual, substitution),
            _ => false,
        },
        ValkyrieType::Nullable(element) => match concrete {
            ValkyrieType::Nullable(actual) => unify_instantiation(element, actual, substitution),
            _ => false,
        },
        ValkyrieType::FixedArray { element, length } => match concrete {
            ValkyrieType::FixedArray { element: actual, length: actual_length } if length == actual_length => {
                unify_instantiation(element, actual, substitution)
            }
            _ => false,
        },
        ValkyrieType::Tuple(items) => match concrete {
            ValkyrieType::Tuple(actual) if items.len() == actual.len() => {
                items.iter().zip(actual).all(|(template, concrete)| unify_instantiation(template, concrete, substitution))
            }
            _ => false,
        },
        ValkyrieType::Union(items) => match concrete {
            ValkyrieType::Union(actual) if items.len() == actual.len() => {
                items.iter().zip(actual).all(|(template, concrete)| unify_instantiation(template, concrete, substitution))
            }
            _ => false,
        },
        ValkyrieType::Intersection(items) => match concrete {
            ValkyrieType::Intersection(actual) if items.len() == actual.len() => {
                items.iter().zip(actual).all(|(template, concrete)| unify_instantiation(template, concrete, substitution))
            }
            _ => false,
        },
        ValkyrieType::Function(function) => match concrete {
            ValkyrieType::Function(actual) if function.params.len() == actual.params.len() => {
                function.params.iter().zip(&actual.params).all(|(template, concrete)| unify_instantiation(template, concrete, substitution))
                    && unify_instantiation(&function.return_type, &actual.return_type, substitution)
            }
            _ => false,
        },
        _ => template == concrete,
    }
}

/// 铸造一个实例：克隆声明体并应用代入，实例身份与签名同时冻结。
fn instantiate_callable(
    declaration: &CallableDeclaration,
    substitution: &Substitution,
    instance: nyar_types::ItemInstanceId,
) -> HirInstantiatedCallable {
    let owner = declaration.owner_type.as_ref();
    let mut function = declaration.function.clone();
    function.instance = Some(instance);
    // 全部泛型已在签名与体内代入，实例不再携带自己的泛型参数。
    function.generics = Vec::new();
    for (index, param) in function.params.iter_mut().enumerate() {
        let declared = declared_param_type(&declaration.function.params[index], index, owner);
        param.ty = substitute_type(&declared, substitution);
    }
    function.return_type = substitute_type(&declared_type(&declaration.function.return_type, owner), substitution);
    for constraint in &mut function.where_constraints {
        substitute_where_constraint(constraint, substitution, owner);
    }
    substitute_block(&mut function.body, substitution, owner);
    let owner_type = owner.map(|owner| substitute_type(owner, substitution));
    HirInstantiatedCallable { function, owner_type, trait_declaration: declaration.trait_declaration }
}

/// 把声明侧类型里的 binder 换成代入结果。
/// Generic binder 可能指向另一个 Generic binder（如跨结构委托：67 → 109），
/// 这里用循环 resolve 到底，直到到达具体类型或无进一步代入。
pub(crate) fn substitute_type(ty: &ValkyrieType, substitution: &Substitution) -> ValkyrieType {
    match ty {
        ValkyrieType::Generic(generic) => {
            let Some(mut current) = generic.binder
            else {
                return ty.clone();
            };
            let mut visited = std::collections::HashSet::new();
            // 循环传递 resolve：sub[67]=Generic(109), sub[109]=i64 → resolve(67)=i64
            while let Some(replacement) = substitution.get(&current) {
                if !visited.insert(current) {
                    // 循环引用，停在这里
                    break;
                }
                match replacement {
                    ValkyrieType::Generic(g) => {
                        if let Some(next) = g.binder {
                            current = next;
                        }
                        else {
                            return replacement.clone();
                        }
                    }
                    _ => return replacement.clone(),
                }
            }
            ty.clone()
        }
        ValkyrieType::Nominal { declaration, arguments } => ValkyrieType::Nominal {
            declaration: *declaration,
            arguments: arguments.iter().map(|argument| substitute_type(argument, substitution)).collect(),
        },
        ValkyrieType::Apply(base, arguments) => ValkyrieType::Apply(
            Box::new(substitute_type(base, substitution)),
            arguments.iter().map(|argument| substitute_type(argument, substitution)).collect(),
        ),
        ValkyrieType::Array(element) => ValkyrieType::Array(Box::new(substitute_type(element, substitution))),
        ValkyrieType::Nullable(element) => ValkyrieType::Nullable(Box::new(substitute_type(element, substitution))),
        ValkyrieType::FixedArray { element, length } => {
            ValkyrieType::FixedArray { element: Box::new(substitute_type(element, substitution)), length: *length }
        }
        ValkyrieType::Tuple(items) => ValkyrieType::Tuple(items.iter().map(|item| substitute_type(item, substitution)).collect()),
        ValkyrieType::Union(items) => ValkyrieType::Union(items.iter().map(|item| substitute_type(item, substitution)).collect()),
        ValkyrieType::Intersection(items) => ValkyrieType::Intersection(items.iter().map(|item| substitute_type(item, substitution)).collect()),
        ValkyrieType::Function(function) => ValkyrieType::Function(Box::new(FunctionType {
            params: function.params.iter().map(|param| substitute_type(param, substitution)).collect(),
            return_type: substitute_type(&function.return_type, substitution),
        })),
        ValkyrieType::Associated(associated) => ValkyrieType::Associated(Box::new(AssociatedType {
            base: substitute_type(&associated.base, substitution),
            name: associated.name.clone(),
            type_arguments: associated.type_arguments.iter().map(|argument| substitute_type(argument, substitution)).collect(),
        })),
        _ => ty.clone(),
    }
}

fn substitute_where_constraint(constraint: &mut HirWhereConstraint, substitution: &Substitution, owner: Option<&ValkyrieType>) {
    constraint.target = substitute_type(&declared_type(&constraint.target, owner), substitution);
    for bound in &mut constraint.bounds {
        for argument in &mut bound.type_arguments {
            *argument = substitute_type(&declared_type(argument, owner), substitution);
        }
        for equation in &mut bound.associated_types {
            equation.ty = substitute_type(&declared_type(&equation.ty, owner), substitution);
        }
    }
}

fn substitute_block(block: &mut HirBlock, substitution: &Substitution, owner: Option<&ValkyrieType>) {
    for statement in &mut block.statements {
        match &mut statement.kind {
            HirStatementKind::Let { ty, initializer, .. } => {
                if let Some(ty) = ty {
                    *ty = substitute_type(&declared_type(ty, owner), substitution);
                }
                if let Some(initializer) = initializer {
                    substitute_expr(initializer, substitution, owner);
                }
            }
            HirStatementKind::Expr(expr) => substitute_expr(expr, substitution, owner),
        }
    }
    if let Some(expr) = &mut block.expr {
        substitute_expr(expr, substitution, owner);
    }
}

fn substitute_resolved_call(call: &mut HirResolvedCall, substitution: &Substitution, owner: Option<&ValkyrieType>) {
    for parameter in &mut call.parameter_types {
        *parameter = substitute_type(&declared_type(parameter, owner), substitution);
    }
    call.return_type = substitute_type(&declared_type(&call.return_type, owner), substitution);
    if let Some(payload) = &mut call.extractor_payload_type {
        *payload = substitute_type(&declared_type(payload, owner), substitution);
    }
    if let Some(dispatch) = &mut call.trait_dispatch {
        let self_type = substitute_type(&declared_type(&dispatch.self_type, owner), substitution);
        *dispatch =
            HirTraitDispatch { trait_declaration: dispatch.trait_declaration, method_declaration: dispatch.method_declaration, self_type };
    }
}

fn substitute_expr(expr: &mut HirExpr, substitution: &Substitution, owner: Option<&ValkyrieType>) {
    match &mut expr.kind {
        HirExprKind::Cast { ty, expr: inner } => {
            *ty = substitute_type(&declared_type(ty, owner), substitution);
            substitute_expr(inner, substitution, owner);
        }
        HirExprKind::Lambda { params, return_type, body, .. } => {
            for param in params {
                param.ty = substitute_type(&declared_type(&param.ty, owner), substitution);
            }
            *return_type = substitute_type(&declared_type(return_type, owner), substitution);
            substitute_block(body, substitution, owner);
        }
        HirExprKind::TryScope { result_type, body, .. } => {
            if let Some(ty) = result_type {
                *ty = substitute_type(&declared_type(ty, owner), substitution);
            }
            substitute_block(body, substitution, owner);
        }
        HirExprKind::Block(block) => substitute_block(block, substitution, owner),
        HirExprKind::If { condition, then_branch, else_branch, .. } => {
            substitute_expr(condition, substitution, owner);
            substitute_block(then_branch, substitution, owner);
            if let Some(branch) = else_branch {
                substitute_block(branch, substitution, owner);
            }
        }
        HirExprKind::IfLet { scrutinee, then_branch, else_branch, .. } => {
            substitute_expr(scrutinee, substitution, owner);
            substitute_block(then_branch, substitution, owner);
            if let Some(branch) = else_branch {
                substitute_block(branch, substitution, owner);
            }
        }
        HirExprKind::Return(Some(value))
        | HirExprKind::Yield(Some(value))
        | HirExprKind::Await(value)
        | HirExprKind::Awake(value)
        | HirExprKind::BlockOn(value)
        | HirExprKind::YieldFrom(value)
        | HirExprKind::Raise(value)
        | HirExprKind::Resume(value) => substitute_expr(value, substitution, owner),
        HirExprKind::TryPropagate { inner, resolved_ok_variant } => {
            substitute_expr(inner, substitution, owner);
            if let Some(variant) = resolved_ok_variant {
                substitute_resolved_call(variant, substitution, owner);
            }
        }
        HirExprKind::Call { callee, args, resolved } => {
            substitute_expr(callee, substitution, owner);
            for arg in args {
                substitute_expr(&mut arg.value, substitution, owner);
            }
            if let Some(resolved) = resolved {
                substitute_resolved_call(resolved, substitution, owner);
            }
        }
        HirExprKind::Construct { args, resolved, .. } => {
            for arg in args {
                substitute_expr(arg, substitution, owner);
            }
            if let Some(resolved) = resolved {
                substitute_resolved_call(resolved, substitution, owner);
            }
        }
        HirExprKind::FieldAccess { object, .. } | HirExprKind::FieldInit { value: object, .. } => {
            substitute_expr(object, substitution, owner);
        }
        HirExprKind::StoreField { object, value, .. } => {
            substitute_expr(object, substitution, owner);
            substitute_expr(value, substitution, owner);
        }
        HirExprKind::GenericApply { callee, arguments } => {
            substitute_expr(callee, substitution, owner);
            for argument in arguments {
                *argument = substitute_type(&declared_type(argument, owner), substitution);
            }
        }
        HirExprKind::ArrayNew { element_type, length } => {
            *element_type = substitute_type(&declared_type(element_type, owner), substitution);
            substitute_expr(length, substitution, owner);
        }
        HirExprKind::ArrayLiteral { items } | HirExprKind::TupleLiteral { items } => {
            for item in items {
                substitute_expr(item, substitution, owner);
            }
        }
        HirExprKind::Match { scrutinee, arms, .. } | HirExprKind::Case { scrutinee, arms, .. } => {
            substitute_expr(scrutinee, substitution, owner);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    substitute_expr(guard, substitution, owner);
                }
                substitute_expr(&mut arm.body, substitution, owner);
            }
        }
        HirExprKind::Loop { iterator, body, condition, .. } => {
            if let Some(iterator) = iterator {
                substitute_expr(iterator, substitution, owner);
            }
            if let Some(condition) = condition {
                substitute_expr(condition, substitution, owner);
            }
            substitute_block(body, substitution, owner);
        }
        HirExprKind::Catch { expr: inner, arms } => {
            substitute_expr(inner, substitution, owner);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    substitute_expr(guard, substitution, owner);
                }
                substitute_expr(&mut arm.body, substitution, owner);
            }
        }
        HirExprKind::With { base, updates } => {
            substitute_expr(base, substitution, owner);
            for (_, update) in updates {
                substitute_expr(update, substitution, owner);
            }
        }
        HirExprKind::SuperCall { args, .. } => {
            for arg in args {
                substitute_expr(arg, substitution, owner);
            }
        }
        HirExprKind::Assign { value, .. } | HirExprKind::Break { expr: Some(value), .. } => {
            substitute_expr(value, substitution, owner);
        }
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                substitute_expr(value, substitution, owner);
            }
            for method in methods {
                substitute_function(method, substitution, owner);
            }
        }
        _ => {}
    }
}

fn substitute_function(function: &mut HirFunction, substitution: &Substitution, owner: Option<&ValkyrieType>) {
    for param in &mut function.params {
        param.ty = substitute_type(&declared_type(&param.ty, owner), substitution);
    }
    function.return_type = substitute_type(&declared_type(&function.return_type, owner), substitution);
    for constraint in &mut function.where_constraints {
        substitute_where_constraint(constraint, substitution, owner);
    }
    substitute_block(&mut function.body, substitution, owner);
}

/// 遍历模块（含子模块）内全部 callable 体内的可调用请求点（调用合同与函数作值引用）。
fn visit_module_calls(module: &mut HirModule, visit: &mut impl FnMut(CallableRequest<'_>)) {
    for function in &mut module.functions {
        visit_block(&mut function.body, visit);
    }
    for structure in &mut module.structs {
        for method in &mut structure.methods {
            visit_block(&mut method.body, visit);
        }
        for property in &mut structure.properties {
            for accessor in property.getter.iter_mut().chain(property.setter.iter_mut()) {
                visit_block(&mut accessor.body, visit);
            }
        }
    }
    for singleton in &mut module.singletons {
        for method in singleton
            .methods
            .iter_mut()
            .chain(singleton.constructor.iter_mut().map(Box::as_mut))
            .chain(singleton.finalizer.iter_mut().map(Box::as_mut))
        {
            visit_block(&mut method.body, visit);
        }
    }
    for trait_definition in &mut module.traits {
        for method in trait_definition.methods.iter_mut().chain(trait_definition.default_methods.iter_mut()) {
            visit_block(&mut method.body, visit);
        }
    }
    for implementation in &mut module.impls {
        for method in &mut implementation.methods {
            visit_block(&mut method.body, visit);
        }
    }
    for widget in &mut module.widgets {
        for method in &mut widget.methods {
            visit_block(&mut method.body, visit);
        }
    }
    for callable in &mut module.instantiated_callables {
        visit_block(&mut callable.function.body, visit);
    }
    for submodule in &mut module.submodules {
        visit_module_calls(submodule, visit);
    }
}

fn visit_block(block: &mut HirBlock, visit: &mut impl FnMut(CallableRequest<'_>)) {
    for statement in &mut block.statements {
        match &mut statement.kind {
            HirStatementKind::Let { initializer, .. } => {
                if let Some(initializer) = initializer {
                    visit_expr(initializer, visit);
                }
            }
            HirStatementKind::Expr(expr) => visit_expr(expr, visit),
        }
    }
    if let Some(expr) = &mut block.expr {
        visit_expr(expr, visit);
    }
}

fn visit_expr(expr: &mut HirExpr, visit: &mut impl FnMut(CallableRequest<'_>)) {
    match &mut expr.kind {
        HirExprKind::Call { callee, args, resolved } => {
            visit_expr(callee, visit);
            for arg in args {
                visit_expr(&mut arg.value, visit);
            }
            if let Some(resolved) = resolved {
                visit(CallableRequest::Call(resolved));
            }
        }
        HirExprKind::Construct { args, resolved, .. } => {
            for arg in args {
                visit_expr(arg, visit);
            }
            if let Some(resolved) = resolved {
                visit(CallableRequest::Call(resolved));
            }
        }
        HirExprKind::FunctionReference(reference) => visit(CallableRequest::Reference(reference)),
        HirExprKind::Cast { expr: inner, .. }
        | HirExprKind::FieldAccess { object: inner, .. }
        | HirExprKind::FieldInit { value: inner, .. }
        | HirExprKind::Await(inner)
        | HirExprKind::Awake(inner)
        | HirExprKind::BlockOn(inner)
        | HirExprKind::YieldFrom(inner)
        | HirExprKind::Raise(inner)
        | HirExprKind::Resume(inner)
        | HirExprKind::Assign { value: inner, .. }
        | HirExprKind::Return(Some(inner))
        | HirExprKind::Yield(Some(inner))
        | HirExprKind::Break { expr: Some(inner), .. } => visit_expr(inner, visit),
        HirExprKind::TryPropagate { inner, resolved_ok_variant } => {
            visit_expr(inner, visit);
            if let Some(variant) = resolved_ok_variant {
                visit(CallableRequest::Call(variant));
            }
        }
        HirExprKind::StoreField { object, value, .. } => {
            visit_expr(object, visit);
            visit_expr(value, visit);
        }
        HirExprKind::GenericApply { callee, .. } => visit_expr(callee, visit),
        HirExprKind::ArrayNew { length, .. } => visit_expr(length, visit),
        HirExprKind::ArrayLiteral { items } | HirExprKind::TupleLiteral { items } => {
            for item in items {
                visit_expr(item, visit);
            }
        }
        // 集合字面量构造槽由 post-binding pass 冻结（声明身份已带、实例待铸造）：
        // 两槽交给固定点登记，字面量项照常下钻。
        HirExprKind::CollectionInit { with_length, append, items, .. } => {
            visit(CallableRequest::Call(with_length));
            visit(CallableRequest::Call(append));
            for item in items {
                visit_expr(item, visit);
            }
        }
        HirExprKind::Block(block) | HirExprKind::TryScope { body: block, .. } => visit_block(block, visit),
        HirExprKind::Lambda { body, .. } => visit_block(body, visit),
        HirExprKind::If { condition, then_branch, else_branch, .. } => {
            visit_expr(condition, visit);
            visit_block(then_branch, visit);
            if let Some(branch) = else_branch {
                visit_block(branch, visit);
            }
        }
        HirExprKind::IfLet { scrutinee, then_branch, else_branch, .. } => {
            visit_expr(scrutinee, visit);
            visit_block(then_branch, visit);
            if let Some(branch) = else_branch {
                visit_block(branch, visit);
            }
        }
        HirExprKind::Match { scrutinee, arms, .. } | HirExprKind::Case { scrutinee, arms, .. } => {
            visit_expr(scrutinee, visit);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    visit_expr(guard, visit);
                }
                visit_expr(&mut arm.body, visit);
            }
        }
        HirExprKind::Loop { iterator, condition, body, .. } => {
            if let Some(iterator) = iterator {
                visit_expr(iterator, visit);
            }
            if let Some(condition) = condition {
                visit_expr(condition, visit);
            }
            visit_block(body, visit);
        }
        HirExprKind::Catch { expr: inner, arms } => {
            visit_expr(inner, visit);
            for arm in arms {
                if let Some(guard) = &mut arm.guard {
                    visit_expr(guard, visit);
                }
                visit_expr(&mut arm.body, visit);
            }
        }
        HirExprKind::With { base, updates } => {
            visit_expr(base, visit);
            for (_, update) in updates {
                visit_expr(update, visit);
            }
        }
        HirExprKind::SuperCall { args, .. } => {
            for arg in args {
                visit_expr(arg, visit);
            }
        }
        HirExprKind::AnonymousClass { fields, methods, .. } => {
            for (_, value) in fields {
                visit_expr(value, visit);
            }
            for method in methods {
                visit_block(&mut method.body, visit);
            }
        }
        _ => {}
    }
}
