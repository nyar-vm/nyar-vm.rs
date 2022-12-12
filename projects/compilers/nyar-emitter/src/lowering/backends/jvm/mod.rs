use crate::nyar_backend_jvm::{
    JvmClassFile, JvmCodeBody, JvmInstruction, JvmMethodDescriptor, JvmMethodRef, JvmMethodSignature, JvmTypeDescriptor,
};
use miette::{Result, miette};
use nyar::{ExternalCallArgument, ExternalCallEdge, ExternalImportLink, Identifier, QualifiedName};
use std_data::binary::class::JvmFieldRef;

use super::{
    executable::executable_has_state_machine as mir_has_state_machine,
    interop::jvm_host_print_target,
    jvm_mir::{effective_param_descriptors, effective_return_descriptor, enclosing_type_name_from_operation, lower_mir_function_to_jvm},
    jvm_suspend, jvm_witness, sanitize_jvm_method_symbol, sanitize_symbol,
    witness_abi::{INJECTED_RUNTIME_STUBS, is_injected_runtime_stub_symbol},
};
use crate::FragmentSubmission;

mod physical_plan;

/// Collect every operation that must receive a method body in this fragment's class.
///
/// Mirrors CLR `lower_fragment_to_msil`: exports alone are not enough — internal callees
/// (`has_cycle` / `directed_graph_new` / …) must be emitted or InvokeStatic refs stay
/// body-less and the jar fails closed / VerifyError at load.
fn collect_jvm_local_operations(submission: &FragmentSubmission) -> Vec<QualifiedName> {
    let mut local_operations = submission.exported_operations.clone();
    for edge in &submission.internal_call_edges {
        if !local_operations.iter().any(|operation| operation == &edge.caller) {
            local_operations.push(edge.caller.clone());
        }
        if !local_operations.iter().any(|operation| operation == &edge.callee_symbol) {
            local_operations.push(edge.callee_symbol.clone());
        }
    }
    for edge in &submission.external_call_edges {
        if !local_operations.iter().any(|operation| operation == &edge.caller) {
            local_operations.push(edge.caller.clone());
        }
        if !local_operations.iter().any(|operation| operation == &edge.callee_symbol) {
            local_operations.push(edge.callee_symbol.clone());
        }
    }
    for operation in submission.operation_literal_returns.keys() {
        if !local_operations.iter().any(|existing| existing == operation) {
            local_operations.push(operation.clone());
        }
    }
    if let Some(entry_operation) = submission.entry_operation.as_ref() {
        if !local_operations.iter().any(|operation| operation == entry_operation) {
            local_operations.push(entry_operation.clone());
        }
    }
    if let Some(executable) = submission.executable.as_ref() {
        for operation in executable.operations() {
            if !local_operations.iter().any(|existing| existing == &operation) {
                local_operations.push(operation);
            }
        }
    }
    for table in &submission.witness_tables {
        for method in &table.methods {
            let operation = QualifiedName::new(vec![Identifier::new(&table.type_name), Identifier::new(&method.method_name)]);
            if !local_operations.iter().any(|existing| existing == &operation) {
                local_operations.push(operation);
            }
        }
    }
    expand_jvm_operations_with_mir_callees(submission, &mut local_operations);
    local_operations
}

/// Walk MIR Call sites (like CLR `expand_operations_with_mir_callees`) so transitive
/// callees (`has_cycle`, `Utf8Text.infix ==`, …) receive method bodies even when call
/// edges were not recorded on the fragment.
fn expand_jvm_operations_with_mir_callees(submission: &FragmentSubmission, operations: &mut Vec<QualifiedName>) {
    use crate::executable_provider::{ExecutableInstructionKind as MirInstructionKind, ExecutableOperand as MirOperand};

    let Some(exec) = submission.executable.as_ref()
    else {
        return;
    };
    let mut index = 0usize;
    while index < operations.len() {
        let operation = operations[index].clone();
        index += 1;
        let Some(view) = exec.get_function(&operation)
        else {
            continue;
        };
        for block in &view.function.blocks {
            for instruction in &block.instructions {
                let MirInstructionKind::Call { callee, arguments, .. } = &instruction.kind
                else {
                    continue;
                };
                let MirOperand::Symbol(path) = callee
                else {
                    continue;
                };
                if let Some(callee_op) = resolve_jvm_mir_callee_operation(exec.as_ref(), path, arguments) {
                    if !operations.iter().any(|existing| existing == &callee_op) {
                        operations.push(callee_op);
                    }
                }
                // Single-segment method names: include every simple-name match so the
                // call-site resolver cannot pick a candidate whose body was skipped.
                if path.parts().len() == 1 {
                    if let Some(method_name) = path.parts().last() {
                        for op in exec.operations() {
                            if op.parts().last().map(|part| part.as_str()) == Some(method_name.as_str()) {
                                if !operations.iter().any(|existing| existing == &op) {
                                    operations.push(op);
                                }
                            }
                        }
                    }
                }
            }
        }
    }
}

fn resolve_jvm_mir_callee_operation(
    exec: &dyn crate::executable_provider::ExecutableProvider,
    path: &nyar::NamePath,
    _arguments: &[crate::executable_provider::ExecutableOperand],
) -> Option<QualifiedName> {
    let method_name = path.parts().last()?.as_str();
    if path.parts().len() > 1 {
        let qualified = QualifiedName::new(path.parts().to_vec());
        if exec.get_function(&qualified).is_some() {
            return Some(qualified);
        }
    }
    exec.operations().iter().find(|operation| operation.parts().last().map(|part| part.as_str()) == Some(method_name)).cloned()
}

/// Fail-closed for linkage: every self `InvokeStatic` must resolve to a method body.
///
/// Prefer real MIR bodies from [`collect_jvm_local_operations`]. Remaining call-site
/// invents (`Fine` / unite ctors / missing `graph_theory` when not in this fragment's
/// executable) are materialized as typed default stubs — same shape as
/// [`ensure_jvm_runtime_stubs`] — so the class file stays loadable. Leaving dangling
/// Methodrefs made `--version` die at verification/linkage; inventing zero/null stubs
/// is the honest floor until those callees are lowered or linked into the fragment.
fn ensure_jvm_self_invokes_resolved(class_file: &mut JvmClassFile) -> Result<()> {
    let defined: std::collections::BTreeSet<(String, JvmMethodDescriptor)> =
        class_file.methods.iter().map(|method| (method.name.clone(), method.descriptor.clone())).collect();
    let class_name = class_file.internal_name.clone();
    let mut missing: std::collections::BTreeSet<(String, JvmMethodDescriptor)> = std::collections::BTreeSet::new();
    for method in &class_file.methods {
        let Some(code) = &method.code
        else {
            continue;
        };
        for instruction in &code.instructions {
            let JvmInstruction::InvokeStatic(method_ref) = instruction
            else {
                continue;
            };
            if method_ref.owner != class_name {
                continue;
            }
            if INJECTED_RUNTIME_STUBS.contains(&method_ref.name.as_str()) {
                continue;
            }
            let key = (method_ref.name.clone(), method_ref.descriptor.clone());
            if defined.contains(&key) || missing.contains(&key) {
                continue;
            }
            missing.insert(key);
        }
    }
    for (name, descriptor) in missing {
        if name == "print" {
            class_file.methods.push(print_jvm_runtime_stub(descriptor));
        }
        else {
            class_file.methods.push(default_jvm_runtime_stub(&name, descriptor));
        }
    }
    Ok(())
}

pub(crate) fn lower_fragment_to_jvm_class(submission: &FragmentSubmission) -> Result<JvmClassFile> {
    crate::lowering::features::semantic_mir_contract::validate_submission(submission).map_err(|error| {
        miette::miette!("semantic MIR contract failed [{}] {} at {}: {}", error.code, error.function, error.location, error.detail)
    })?;
    crate::lowering::features::physical_contract::validate_physical_submission(
        submission,
        crate::lowering::features::physical_contract::PhysicalBackend::Jvm,
    )
    .map_err(|error| miette::miette!("physical contract failed [{}] {} at {}: {}", error.code, error.function, error.location, error.detail))?;
    let local_operations = collect_jvm_local_operations(submission);
    physical_plan::require_operations_planned(submission, &local_operations)?;
    if let Some(entry) = &submission.entry_operation {
        physical_plan::require_exact_function(submission, entry)?;
    }
    validate_jvm_call_contracts(submission)?;
    let mut class_file =
        JvmClassFile::new(format!("{}/{}", sanitize_symbol(&submission.module_name), sanitize_symbol(submission.fragment_id.as_str())));
    let mut emitted_names: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    for operation in &local_operations {
        let method_name = sanitize_jvm_method_symbol(operation);
        if !emitted_names.insert(method_name.clone()) {
            continue;
        }
        // 仅当函数 MIR 含状态机终止符（StateDispatch/YieldToRuntime）时才跳过正常方法
        // 生成，交由 suspend 增强阶段生成状态机包装。catch+resume 等被优化为直接线性流
        // 的场景不含这些终止符，必须走正常 MIR lowering 才能正确返回值。
        let is_state_machine = submission
            .executable
            .as_ref()
            .and_then(|exec| exec.get_function(operation))
            .map(|view| mir_has_state_machine(&view.function))
            .unwrap_or(false);
        if is_state_machine
            && submission.control_flow.as_ref().is_some_and(|payload| payload.functions.iter().any(|function| &function.symbol == operation))
        {
            continue;
        }
        if let Some(exec) = &submission.executable {
            if let Some(view) = exec.get_function(operation) {
                class_file.methods.push(lower_mir_function_to_jvm(submission, operation, &view.function));
                continue;
            }
        }
        // No MIR body: only emit a placeholder for exported/external edges; never invent
        // bodies for unresolved internal callees (fail-closed later via ensure_*).
        if submission.exported_operations.iter().any(|op| op == operation)
            || submission.external_call_edges.iter().any(|edge| &edge.callee_symbol == operation)
        {
            let external_call_edges = outgoing_external_call_edges(operation, &submission.external_call_edges);
            class_file.methods.push(JvmMethodSignature {
                name: method_name,
                descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Int),
                access_flags: 0x0001 | 0x0008,
                code: Some(JvmCodeBody {
                    max_stack: 2,
                    max_locals: 0,
                    instructions: lower_operation_instructions(external_call_edges, &submission.external_import_links),
                }),
            });
        }
    }
    jvm_suspend::append_suspend_state_machine_methods(&mut class_file, submission);
    let witness_main = jvm_witness::append_witness_methods(&mut class_file, submission);
    if submission.witness_calls.is_empty() {
        if let Some(entry) = &submission.entry_operation {
            if !submission.control_flow.as_ref().is_some_and(|payload| payload.functions.iter().any(|function| &function.symbol == entry)) {
                let entry_owner = format!("{}/{}", sanitize_symbol(&submission.module_name), sanitize_symbol(submission.fragment_id.as_str()));
                let entry_name = sanitize_jvm_method_symbol(entry);
                let entry_return_desc = submission
                    .executable
                    .as_ref()
                    .and_then(|exec| exec.get_function(entry))
                    .map(|view| effective_return_descriptor(submission, &view.function, enclosing_type_name_from_operation(entry).as_deref()))
                    .unwrap_or(JvmTypeDescriptor::Int);
                let mut entry_instructions = vec![JvmInstruction::InvokeStatic(JvmMethodRef {
                    owner: entry_owner,
                    name: entry_name.clone(),
                    descriptor: JvmMethodDescriptor::new(Vec::new(), entry_return_desc.clone()),
                })];
                match entry_return_desc {
                    JvmTypeDescriptor::Long => entry_instructions.push(JvmInstruction::L2I),
                    JvmTypeDescriptor::Double => entry_instructions.push(JvmInstruction::D2I),
                    JvmTypeDescriptor::Void => entry_instructions.push(JvmInstruction::IConst(0)),
                    JvmTypeDescriptor::Object(_) | JvmTypeDescriptor::Array(_) => {
                        entry_instructions.push(JvmInstruction::Pop);
                        entry_instructions.push(JvmInstruction::IConst(0));
                    }
                    _ => {}
                }
                entry_instructions.push(JvmInstruction::IReturn);
                class_file.methods.push(JvmMethodSignature {
                    name: format!("entry_{}", entry_name),
                    descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Int),
                    access_flags: 0x0001 | 0x0008,
                    code: Some(JvmCodeBody { max_stack: 2, max_locals: 0, instructions: entry_instructions }),
                });
            }
        }
    }
    let main_body = jvm_suspend::lower_suspend_main_entry(submission)
        .or(witness_main)
        .or_else(|| lower_sync_entry_call(submission))
        .or_else(|| lower_fallback_entry_call(submission))
        .unwrap_or_else(|| vec![JvmInstruction::IConst(0), JvmInstruction::IReturn]);
    let main_body = convert_main_body_to_void(main_body);
    ensure_jvm_runtime_stubs(&mut class_file);
    ensure_jvm_self_invokes_resolved(&mut class_file)?;
    let string_array_desc = JvmTypeDescriptor::Array(Box::new(JvmTypeDescriptor::Object("java/lang/String".to_string())));
    class_file.methods.push(JvmMethodSignature {
        name: "main".to_string(),
        descriptor: JvmMethodDescriptor::new(vec![string_array_desc], JvmTypeDescriptor::Void),
        access_flags: 0x0001 | 0x0008,
        code: Some(JvmCodeBody { max_stack: 4, max_locals: 3, instructions: main_body }),
    });
    for method in &class_file.methods {
        physical_plan::verify_emitted_method(method)?;
    }
    Ok(class_file)
}

/// Reject calls whose JVM ABI cannot be obtained from an exact semantic
/// contract.  JVM descriptors are physical projections; they are never a
/// source of language semantics and must not be reconstructed from a short
/// callee name, actual operands, or an output slot.
fn validate_jvm_call_contracts(submission: &FragmentSubmission) -> Result<()> {
    let Some(executable) = &submission.executable
    else {
        return Ok(());
    };
    for operation in executable.operations() {
        let Some(view) = executable.get_function(&operation)
        else {
            continue;
        };
        for block in &view.function.blocks {
            for (index, instruction) in block.instructions.iter().enumerate() {
                let crate::contracts::InstructionKind::Call { callee, intrinsic_opcode, dispatch, .. } = &instruction.kind
                else {
                    continue;
                };
                if intrinsic_opcode.is_some() || !matches!(dispatch, crate::contracts::DispatchKind::Static) {
                    continue;
                }
                let crate::contracts::Operand::Symbol(path) = callee
                else {
                    return Err(miette!(
                        "JVM pre-emission verifier: function `{}` block {} instruction {index} requires an exact static callee symbol",
                        view.function.symbol,
                        block.id.0,
                    ));
                };
                let symbol = path.to_string();
                let local = executable.find_by_symbol(&symbol).is_some();
                let external_link = submission.external_import_links.get(&nyar::QualifiedName::new(path.parts().to_vec()));
                let runtime_stub = is_injected_runtime_stub_symbol(&path.parts().iter().map(|part| part.as_str()).collect::<Vec<_>>());
                let host_print = external_link.is_some_and(|link| jvm_host_print_target(link).is_some());
                if !local && !host_print && !runtime_stub {
                    return Err(miette!(
                        "JVM pre-emission verifier: function `{}` block {} instruction {index} static callee `{symbol}` has no exact JVM ABI contract",
                        view.function.symbol,
                        block.id.0,
                    ));
                }
            }
        }
    }
    Ok(())
}

/// 将 `main` 方法体从 `int` 返回（`IReturn`）转换为 `void` 返回（`Return`）。
///
/// 所有 main body 生产者（`lower_sync_entry_call`、`lower_witness_main_instructions`、
/// `jvm_entry_wrapper_instructions`、默认 fallback）均以 `IReturn` 结尾并在栈上留一个
/// `int` 值。由于 `main` 方法签名改为 `([Ljava/lang/String;)V`（void 返回），需移除
/// 尾部 `IReturn`，插入 `Pop` 消费栈上残留的 `int`，再追加 `Return`。
fn convert_main_body_to_void(mut instructions: Vec<JvmInstruction>) -> Vec<JvmInstruction> {
    if matches!(instructions.last(), Some(JvmInstruction::IReturn)) {
        instructions.pop();
        instructions.push(JvmInstruction::Pop);
    }
    instructions.push(JvmInstruction::Return);
    instructions
}

/// Inject only the allowlisted runtime stubs that Call sites actually reference.
///
/// Unlike inventing a stub for every missing self `InvokeStatic`, this only
/// materializes names in [`super::witness_abi::INJECTED_RUNTIME_STUBS`]
/// (`panic` / `is_null` / `unwrap_null` / `print`), matching CLR
/// `ensure_runtime_stubs`. `print` forwards to `System.out.println`; others
/// use [`default_jvm_runtime_stub`].
fn ensure_jvm_runtime_stubs(class_file: &mut JvmClassFile) {
    use super::witness_abi::INJECTED_RUNTIME_STUBS;

    let defined: std::collections::BTreeSet<(String, JvmMethodDescriptor)> =
        class_file.methods.iter().map(|method| (method.name.clone(), method.descriptor.clone())).collect();
    let mut needed: std::collections::BTreeSet<(String, JvmMethodDescriptor)> = std::collections::BTreeSet::new();
    let class_name = &class_file.internal_name;
    for method in &class_file.methods {
        let Some(code) = &method.code
        else {
            continue;
        };
        for instruction in &code.instructions {
            if let JvmInstruction::InvokeStatic(method_ref) = instruction {
                if method_ref.owner != *class_name {
                    continue;
                }
                if !INJECTED_RUNTIME_STUBS.contains(&method_ref.name.as_str()) {
                    continue;
                }
                let key = (method_ref.name.clone(), method_ref.descriptor.clone());
                if defined.contains(&key) {
                    continue;
                }
                needed.insert(key);
            }
        }
    }
    for (name, descriptor) in &needed {
        if name == "print" {
            class_file.methods.push(print_jvm_runtime_stub(descriptor.clone()));
        }
        else {
            class_file.methods.push(default_jvm_runtime_stub(name, descriptor.clone()));
        }
    }
}

/// 生成默认返回值的运行时桩方法。
///
/// 根据 [`JvmTypeDescriptor`] 返回类型选择对应的返回指令：
/// - `Int` / `Boolean` / `Byte` / `Short` / `Char`：`iconst_0; ireturn`
/// - `Long`：`lconst_0; lreturn`
/// - `Float`：`fconst_0; freturn`
/// - `Double`：`dconst_0; dreturn`
/// - `Object` / `Array`：`aconst_null; areturn`
/// - `Void`：`return`
///
/// Count JVM local slots occupied by method parameters (`long`/`double` = 2).
///
/// Class loading rejects `Code.max_locals` below this (`Arguments can't fit into
/// locals`). Counting `parameter_types.len()` alone under-sizes `(JJ)I` / `(IJJ)I`
/// stubs invented by [`ensure_jvm_self_invokes_resolved`].
fn jvm_method_parameter_slots(descriptor: &JvmMethodDescriptor) -> u16 {
    descriptor.parameter_types.iter().fold(0u16, |slots, ty| {
        slots.saturating_add(match ty {
            JvmTypeDescriptor::Long | JvmTypeDescriptor::Double => 2,
            _ => 1,
        })
    })
}

/// 参数列表由 `descriptor.parameter_types` 决定；`max_locals` 按 JVM 槽位宽度
/// （`J`/`D` = 2）计算，不能只用 `parameter_types.len()`。
fn default_jvm_runtime_stub(name: &str, descriptor: JvmMethodDescriptor) -> JvmMethodSignature {
    let (max_stack, instructions) = match &descriptor.return_type {
        JvmTypeDescriptor::Void => (0, vec![JvmInstruction::Return]),
        JvmTypeDescriptor::Long => (2, vec![JvmInstruction::LConst0, JvmInstruction::LReturn]),
        JvmTypeDescriptor::Double => (2, vec![JvmInstruction::DConst0, JvmInstruction::DReturn]),
        JvmTypeDescriptor::Float => (1, vec![JvmInstruction::FConst0, JvmInstruction::FReturn]),
        JvmTypeDescriptor::Object(_) | JvmTypeDescriptor::Array(_) => (1, vec![JvmInstruction::AConstNull, JvmInstruction::AReturn]),
        _ => (1, vec![JvmInstruction::IConst(0), JvmInstruction::IReturn]),
    };
    let max_locals = jvm_method_parameter_slots(&descriptor).max(1);
    JvmMethodSignature {
        name: name.to_string(),
        descriptor,
        access_flags: 0x0001 | 0x0008,
        code: Some(JvmCodeBody { max_stack, max_locals, instructions }),
    }
}

/// `print` 运行时桩方法（JVM 字节码）。
///
/// 描述符 `(Ljava/lang/Object;)I` 与 [`super::jvm_mir::runtime_stub_descriptor`] 对齐：
/// 接受 `Object` 参数以兼容字符串及其他引用类型，返回 `Int 0`（退出码）。
/// 实现：`System.out.println(arg0); return 0;`
fn print_jvm_runtime_stub(descriptor: JvmMethodDescriptor) -> JvmMethodSignature {
    let max_locals = jvm_method_parameter_slots(&descriptor).max(1);
    JvmMethodSignature {
        name: "print".to_string(),
        descriptor,
        access_flags: 0x0001 | 0x0008,
        code: Some(JvmCodeBody {
            max_stack: 2,
            max_locals,
            instructions: vec![
                JvmInstruction::GetStatic(JvmFieldRef {
                    owner: "java/lang/System".to_string(),
                    name: "out".to_string(),
                    descriptor: JvmTypeDescriptor::Object("java/io/PrintStream".to_string()),
                }),
                JvmInstruction::ALoad(0),
                JvmInstruction::InvokeVirtual(JvmMethodRef {
                    owner: "java/io/PrintStream".to_string(),
                    name: "println".to_string(),
                    descriptor: JvmMethodDescriptor::new(
                        vec![JvmTypeDescriptor::Object("java/lang/Object".to_string())],
                        JvmTypeDescriptor::Void,
                    ),
                }),
                JvmInstruction::IConst(0),
                JvmInstruction::IReturn,
            ],
        }),
    }
}

/// 为 sync 入口函数生成 `InvokeStatic` 调用指令。
///
/// 当入口操作不在 `control_flow.functions` 中（即非 suspend 函数）时，
/// `main` 方法应直接调用入口方法（如 `main__main`）并返回其结果。
/// `main` 方法固定返回 `int`（退出码），因此入口返回 long/double 时需插入转换指令，
/// 入口返回 void 时需补 `IConst(0)` 占位。
///
/// 入口方法的参数描述符从 MIR `param_types` 推导，与 [`lower_mir_function_to_jvm`]
/// 中的构造逻辑保持一致。`main` 方法的 `args`（local 0）在 `InvokeStatic` 前通过
/// `ALoad(0)` 传入入口方法。
fn lower_sync_entry_call(submission: &FragmentSubmission) -> Option<Vec<JvmInstruction>> {
    let entry = submission.entry_operation.as_ref()?;
    let entry_view = submission.executable.as_ref().and_then(|exec| exec.get_function(entry))?;
    let mir_fn = &entry_view.function;
    let entry_has_state_machine = mir_has_state_machine(mir_fn);
    if entry_has_state_machine
        && submission.control_flow.as_ref().is_some_and(|payload| payload.functions.iter().any(|function| &function.symbol == entry))
    {
        return None;
    }
    let owner = format!("{}/{}", sanitize_symbol(&submission.module_name), sanitize_symbol(submission.fragment_id.as_str()));
    let method_name = sanitize_jvm_method_symbol(entry);
    let return_descriptor = effective_return_descriptor(submission, mir_fn, enclosing_type_name_from_operation(entry).as_deref());
    let param_descriptors = effective_param_descriptors(submission, mir_fn, enclosing_type_name_from_operation(entry).as_deref());
    let mut instructions = Vec::new();
    if !param_descriptors.is_empty() {
        instructions.push(JvmInstruction::ALoad(0));
    }
    instructions.push(JvmInstruction::InvokeStatic(JvmMethodRef {
        owner,
        name: method_name,
        descriptor: JvmMethodDescriptor::new(param_descriptors, return_descriptor.clone()),
    }));
    match return_descriptor {
        JvmTypeDescriptor::Long => instructions.push(JvmInstruction::L2I),
        JvmTypeDescriptor::Double => instructions.push(JvmInstruction::D2I),
        JvmTypeDescriptor::Void => instructions.push(JvmInstruction::IConst(0)),
        JvmTypeDescriptor::Object(_) | JvmTypeDescriptor::Array(_) => {
            instructions.push(JvmInstruction::Pop);
            instructions.push(JvmInstruction::IConst(0));
        }
        _ => {}
    }
    instructions.push(JvmInstruction::IReturn);
    Some(instructions)
}

/// 无 MIR 时的入口回退：调用已为 `entry_operation` 生成的 `entry_*` 包装方法。
///
/// 纯 external-call fragment（无 `executable`）仍会生成 `demo__main` 与 `entry_demo__main`，
/// 但 [`lower_sync_entry_call`] 依赖 MIR，此时需把 `main` 接到 `entry_*`，否则 JAR 启动空跑。
fn lower_fallback_entry_call(submission: &FragmentSubmission) -> Option<Vec<JvmInstruction>> {
    let entry = submission.entry_operation.as_ref()?;
    if submission.executable.as_ref().and_then(|exec| exec.get_function(entry)).is_some() {
        return None;
    }
    if submission.control_flow.as_ref().is_some_and(|payload| payload.functions.iter().any(|function| &function.symbol == entry)) {
        return None;
    }
    let entry_name = sanitize_jvm_method_symbol(entry);
    let owner = format!("{}/{}", sanitize_symbol(&submission.module_name), sanitize_symbol(submission.fragment_id.as_str()));
    Some(vec![
        JvmInstruction::InvokeStatic(JvmMethodRef {
            owner,
            name: format!("entry_{}", entry_name),
            descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Int),
        }),
        JvmInstruction::IReturn,
    ])
}

fn lower_operation_instructions(
    external_call_edges: Vec<&ExternalCallEdge>,
    external_import_links: &std::collections::BTreeMap<QualifiedName, ExternalImportLink>,
) -> Vec<JvmInstruction> {
    let mut instructions = Vec::new();
    for edge in external_call_edges {
        let Some(target) = jvm_print_target(external_import_links.get(&edge.callee_symbol))
        else {
            continue;
        };
        let Some(message) = edge.arguments.iter().find_map(string_literal_argument)
        else {
            continue;
        };

        instructions.push(JvmInstruction::GetStatic(JvmFieldRef {
            owner: target.field_owner,
            name: target.field_name,
            descriptor: JvmTypeDescriptor::Object(target.stream_owner.clone()),
        }));
        instructions.push(JvmInstruction::LdcString(message));
        instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: target.stream_owner,
            name: target.method_name,
            descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object("java/lang/String".to_string())], JvmTypeDescriptor::Void),
        }));
    }
    instructions.push(JvmInstruction::IConst(0));
    instructions.push(JvmInstruction::IReturn);
    instructions
}

fn string_literal_argument(argument: &ExternalCallArgument) -> Option<String> {
    match argument {
        ExternalCallArgument::StringLiteral(value) => Some(value.clone()),
    }
}

fn outgoing_external_call_edges<'a>(operation: &QualifiedName, edges: &'a [ExternalCallEdge]) -> Vec<&'a ExternalCallEdge> {
    edges.iter().filter(|edge| &edge.caller == operation).collect()
}

fn jvm_print_target(external_import_link: Option<&ExternalImportLink>) -> Option<JvmPrintTarget> {
    let target = jvm_host_print_target(external_import_link?)?;
    let (field_name, method_name, stream_owner) = if let Some(dot_pos) = target.method_name.rfind('.') {
        (target.method_name[..dot_pos].to_string(), target.method_name[dot_pos + 1..].to_string(), "java/io/PrintStream".to_string())
    }
    else {
        (target.field_name.to_string(), target.method_name.to_string(), target.stream_owner.replace('.', "/"))
    };
    Some(JvmPrintTarget { field_owner: target.field_owner.replace('.', "/"), field_name, stream_owner, method_name })
}

struct JvmPrintTarget {
    field_owner: String,
    field_name: String,
    stream_owner: String,
    method_name: String,
}
