//! JVM-only physical preparation derived from the canonical physical plan.
//!
//! This module owns JVM descriptors and local-slot facts. It does not resolve
//! language types, symbols, or text carriers; those decisions must already be
//! present in the Semantic MIR physical contract.

use miette::{Result, miette};

use crate::{
    FragmentSubmission,
    lowering::features::physical_contract::{PhysicalBackend, PhysicalFunctionPlan, PhysicalValueCategory, build_physical_plan},
    nyar_backend_jvm::{JvmInstruction, JvmMethodDescriptor, JvmMethodRef, JvmMethodSignature, JvmTypeDescriptor},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JvmLocalSlotPlan {
    pub value: u32,
    pub slot: u16,
    pub category: PhysicalValueCategory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct JvmFunctionPlan {
    pub symbol: String,
    pub descriptor: JvmMethodDescriptor,
    pub locals: Vec<JvmLocalSlotPlan>,
}

pub(crate) fn prepare(submission: &FragmentSubmission) -> Result<Vec<JvmFunctionPlan>> {
    let plans = build_physical_plan(submission, PhysicalBackend::Jvm)
        .map_err(|error| miette!("physical contract failed [{}] {} at {}: {}", error.code, error.function, error.location, error.detail))?;
    plans.iter().map(to_jvm_plan).collect()
}

pub(crate) fn require_operations_planned(submission: &FragmentSubmission, operations: &[nyar::QualifiedName]) -> Result<()> {
    let plans = prepare(submission)?;
    for operation in operations {
        let symbol = operation.to_string();
        if !plans.iter().any(|plan| plan.symbol == symbol) {
            return Err(miette!(
                "physical contract failed [BPHYS004] {} at function: JVM operation has no exact private physical plan",
                operation
            ));
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum StackKind {
    Int,
    Long,
    Float,
    Double,
    Reference,
}

pub(crate) fn verify_emitted_method(method: &JvmMethodSignature) -> Result<()> {
    let Some(code) = &method.code
    else {
        return Ok(());
    };
    let mut locals = Vec::<Option<StackKind>>::new();
    for parameter in &method.descriptor.parameter_types {
        let kind = stack_kind(parameter)?;
        locals.push(Some(kind));
        if matches!(kind, StackKind::Long | StackKind::Double) {
            locals.push(None);
        }
    }
    let mut stack = Vec::new();
    let mut max_slots = 0u16;
    for (index, instruction) in code.instructions.iter().enumerate() {
        let pop = |stack: &mut Vec<StackKind>, expected: Option<StackKind>| -> Result<StackKind> {
            let actual = stack.pop().ok_or_else(|| {
                miette!("physical contract failed [BPHYS012] {} instruction {index}: JVM operand stack underflow", method.name)
            })?;
            if expected.is_some_and(|expected| expected != actual) {
                return Err(miette!("physical contract failed [BPHYS012] {} instruction {index}: JVM operand category mismatch", method.name));
            }
            Ok(actual)
        };
        match instruction {
            JvmInstruction::Label(_) | JvmInstruction::Goto(_) => {}
            JvmInstruction::ALoad(slot) => {
                check_local(&locals, *slot, StackKind::Reference, method, index)?;
                stack.push(StackKind::Reference);
            }
            JvmInstruction::ALoad0 => {
                check_local(&locals, 0, StackKind::Reference, method, index)?;
                stack.push(StackKind::Reference);
            }
            JvmInstruction::ILoad(slot) => {
                check_local(&locals, *slot, StackKind::Int, method, index)?;
                stack.push(StackKind::Int);
            }
            JvmInstruction::LLoad(slot) => {
                check_local(&locals, *slot, StackKind::Long, method, index)?;
                stack.push(StackKind::Long);
            }
            JvmInstruction::FLoad(slot) => {
                check_local(&locals, *slot, StackKind::Float, method, index)?;
                stack.push(StackKind::Float);
            }
            JvmInstruction::DLoad(slot) => {
                check_local(&locals, *slot, StackKind::Double, method, index)?;
                stack.push(StackKind::Double);
            }
            JvmInstruction::AStore(slot) => {
                pop(&mut stack, Some(StackKind::Reference))?;
                set_local(&mut locals, *slot, StackKind::Reference, method, index);
            }
            JvmInstruction::IStore(slot) => {
                pop(&mut stack, Some(StackKind::Int))?;
                set_local(&mut locals, *slot, StackKind::Int, method, index);
            }
            JvmInstruction::LStore(slot) => {
                pop(&mut stack, Some(StackKind::Long))?;
                set_local(&mut locals, *slot, StackKind::Long, method, index);
            }
            JvmInstruction::FStore(slot) => {
                pop(&mut stack, Some(StackKind::Float))?;
                set_local(&mut locals, *slot, StackKind::Float, method, index);
            }
            JvmInstruction::DStore(slot) => {
                pop(&mut stack, Some(StackKind::Double))?;
                set_local(&mut locals, *slot, StackKind::Double, method, index);
            }
            JvmInstruction::AConstNull | JvmInstruction::LdcString(_) | JvmInstruction::New(_) => stack.push(StackKind::Reference),
            JvmInstruction::IConst(_) => stack.push(StackKind::Int),
            JvmInstruction::LConst0 | JvmInstruction::LConst1 | JvmInstruction::LdcLong(_) => stack.push(StackKind::Long),
            JvmInstruction::FConst0 => stack.push(StackKind::Float),
            JvmInstruction::DConst0 | JvmInstruction::DConst1 | JvmInstruction::LdcDouble(_) => stack.push(StackKind::Double),
            JvmInstruction::Pop => {
                pop(&mut stack, None)?;
            }
            JvmInstruction::Dup => {
                let value = *stack
                    .last()
                    .ok_or_else(|| miette!("physical contract failed [BPHYS012] {} instruction {index}: dup underflow", method.name))?;
                stack.push(value);
            }
            JvmInstruction::IAdd
            | JvmInstruction::ISub
            | JvmInstruction::IMul
            | JvmInstruction::IDiv
            | JvmInstruction::IRem
            | JvmInstruction::IAnd
            | JvmInstruction::IOr
            | JvmInstruction::IXor
            | JvmInstruction::IShl
            | JvmInstruction::IShr
            | JvmInstruction::IUShr => {
                pop(&mut stack, Some(StackKind::Int))?;
                pop(&mut stack, Some(StackKind::Int))?;
                stack.push(StackKind::Int);
            }
            JvmInstruction::LAdd
            | JvmInstruction::LSub
            | JvmInstruction::LMul
            | JvmInstruction::LDiv
            | JvmInstruction::LRem
            | JvmInstruction::LAnd
            | JvmInstruction::LOr
            | JvmInstruction::LXor
            | JvmInstruction::LShl
            | JvmInstruction::LShr
            | JvmInstruction::LUShr => {
                pop(&mut stack, None)?;
                pop(&mut stack, Some(StackKind::Long))?;
                stack.push(StackKind::Long);
            }
            JvmInstruction::FAdd | JvmInstruction::FSub | JvmInstruction::FMul | JvmInstruction::FDiv | JvmInstruction::FRem => {
                pop(&mut stack, Some(StackKind::Float))?;
                pop(&mut stack, Some(StackKind::Float))?;
                stack.push(StackKind::Float);
            }
            JvmInstruction::DAdd | JvmInstruction::DSub | JvmInstruction::DMul | JvmInstruction::DDiv | JvmInstruction::DRem => {
                pop(&mut stack, Some(StackKind::Double))?;
                pop(&mut stack, Some(StackKind::Double))?;
                stack.push(StackKind::Double);
            }
            JvmInstruction::INeg => {
                pop(&mut stack, Some(StackKind::Int))?;
                stack.push(StackKind::Int);
            }
            JvmInstruction::LNeg => {
                pop(&mut stack, Some(StackKind::Long))?;
                stack.push(StackKind::Long);
            }
            JvmInstruction::FNeg => {
                pop(&mut stack, Some(StackKind::Float))?;
                stack.push(StackKind::Float);
            }
            JvmInstruction::DNeg => {
                pop(&mut stack, Some(StackKind::Double))?;
                stack.push(StackKind::Double);
            }
            JvmInstruction::I2L => {
                pop(&mut stack, Some(StackKind::Int))?;
                stack.push(StackKind::Long);
            }
            JvmInstruction::L2I => {
                pop(&mut stack, Some(StackKind::Long))?;
                stack.push(StackKind::Int);
            }
            JvmInstruction::I2D => {
                pop(&mut stack, Some(StackKind::Int))?;
                stack.push(StackKind::Double);
            }
            JvmInstruction::L2D => {
                pop(&mut stack, Some(StackKind::Long))?;
                stack.push(StackKind::Double);
            }
            JvmInstruction::D2I => {
                pop(&mut stack, Some(StackKind::Double))?;
                stack.push(StackKind::Int);
            }
            JvmInstruction::IfEq(_)
            | JvmInstruction::IfNe(_)
            | JvmInstruction::IfLt(_)
            | JvmInstruction::IfLe(_)
            | JvmInstruction::IfGt(_)
            | JvmInstruction::IfGe(_)
            | JvmInstruction::IfNull(_)
            | JvmInstruction::IfNonNull(_) => {
                pop(&mut stack, None)?;
            }
            JvmInstruction::IfICmpEq(_)
            | JvmInstruction::IfICmpNe(_)
            | JvmInstruction::IfICmpLt(_)
            | JvmInstruction::IfICmpLe(_)
            | JvmInstruction::IfICmpGt(_)
            | JvmInstruction::IfICmpGe(_)
            | JvmInstruction::IfACmpEq(_)
            | JvmInstruction::IfACmpNe(_) => {
                pop(&mut stack, None)?;
                pop(&mut stack, None)?;
            }
            JvmInstruction::IReturn => {
                pop(&mut stack, Some(StackKind::Int))?;
                check_return(&method.descriptor.return_type, StackKind::Int, method, index)?;
            }
            JvmInstruction::LReturn => {
                pop(&mut stack, Some(StackKind::Long))?;
                check_return(&method.descriptor.return_type, StackKind::Long, method, index)?;
            }
            JvmInstruction::FReturn => {
                pop(&mut stack, Some(StackKind::Float))?;
                check_return(&method.descriptor.return_type, StackKind::Float, method, index)?;
            }
            JvmInstruction::DReturn => {
                pop(&mut stack, Some(StackKind::Double))?;
                check_return(&method.descriptor.return_type, StackKind::Double, method, index)?;
            }
            JvmInstruction::AReturn => {
                pop(&mut stack, Some(StackKind::Reference))?;
                check_return(&method.descriptor.return_type, StackKind::Reference, method, index)?;
            }
            JvmInstruction::Return => {
                if method.descriptor.return_type != JvmTypeDescriptor::Void {
                    return Err(miette!(
                        "physical contract failed [BPHYS012] {} instruction {index}: void return conflicts with descriptor",
                        method.name
                    ));
                }
            }
            JvmInstruction::InvokeStatic(reference) | JvmInstruction::InvokeVirtual(reference) | JvmInstruction::InvokeSpecial(reference) => {
                verify_call(&mut stack, reference, method, index)?
            }
            _ => {
                return Err(miette!(
                    "physical contract failed [BPHYS012] {} instruction {index}: unsupported JVM opcode in pre-emission verifier",
                    method.name
                ));
            }
        }
        max_slots = max_slots.max(stack.iter().map(|kind| if matches!(kind, StackKind::Long | StackKind::Double) { 2 } else { 1 }).sum());
    }
    if max_slots > code.max_stack || locals.len() as u16 > code.max_locals {
        return Err(miette!("physical contract failed [BPHYS012] {} at code limits: JVM max stack/local limits are insufficient", method.name));
    }
    Ok(())
}

fn stack_kind(ty: &JvmTypeDescriptor) -> Result<StackKind> {
    Ok(match ty {
        JvmTypeDescriptor::Long => StackKind::Long,
        JvmTypeDescriptor::Float => StackKind::Float,
        JvmTypeDescriptor::Double => StackKind::Double,
        JvmTypeDescriptor::Object(_) | JvmTypeDescriptor::Array(_) => StackKind::Reference,
        JvmTypeDescriptor::Void => return Err(miette!("physical contract failed [BPHYS012] void is not a JVM stack value")),
        _ => StackKind::Int,
    })
}
fn check_local(locals: &[Option<StackKind>], slot: u16, expected: StackKind, method: &JvmMethodSignature, index: usize) -> Result<()> {
    if locals.get(slot as usize).copied().flatten() != Some(expected) {
        return Err(miette!("physical contract failed [BPHYS012] {} instruction {index}: JVM local category mismatch", method.name));
    }
    Ok(())
}
fn set_local(locals: &mut Vec<Option<StackKind>>, slot: u16, kind: StackKind, _method: &JvmMethodSignature, _index: usize) {
    let needed = slot as usize + if matches!(kind, StackKind::Long | StackKind::Double) { 2 } else { 1 };
    locals.resize(needed, None);
    locals[slot as usize] = Some(kind);
}
fn check_return(ty: &JvmTypeDescriptor, kind: StackKind, method: &JvmMethodSignature, index: usize) -> Result<()> {
    if stack_kind(ty)? != kind {
        return Err(miette!("physical contract failed [BPHYS012] {} instruction {index}: JVM return category mismatch", method.name));
    }
    Ok(())
}
fn verify_call(stack: &mut Vec<StackKind>, reference: &JvmMethodRef, method: &JvmMethodSignature, index: usize) -> Result<()> {
    for parameter in reference.descriptor.parameter_types.iter().rev() {
        let expected = stack_kind(parameter)?;
        if stack.pop() != Some(expected) {
            return Err(miette!("physical contract failed [BPHYS012] {} instruction {index}: JVM call operand category mismatch", method.name));
        }
    }
    if reference.descriptor.return_type != JvmTypeDescriptor::Void {
        stack.push(stack_kind(&reference.descriptor.return_type)?);
    }
    Ok(())
}

fn to_jvm_plan(plan: &PhysicalFunctionPlan) -> Result<JvmFunctionPlan> {
    let parameters = plan.parameters.iter().map(jvm_descriptor).collect::<Result<Vec<_>>>()?;
    let result = jvm_descriptor(&plan.result)?;
    let mut locals = Vec::new();
    let mut slot = 0u16;
    for (value, category) in &plan.values {
        locals.push(JvmLocalSlotPlan { value: value.0, slot, category: *category });
        slot = slot
            .checked_add(if matches!(category, PhysicalValueCategory::I64 | PhysicalValueCategory::F64) { 2 } else { 1 })
            .ok_or_else(|| miette!("physical contract failed [BPHYS011] {} at locals: JVM local slot overflow", plan.symbol))?;
    }
    Ok(JvmFunctionPlan { symbol: plan.symbol.clone(), descriptor: JvmMethodDescriptor::new(parameters, result), locals })
}

fn jvm_descriptor(category: &PhysicalValueCategory) -> Result<JvmTypeDescriptor> {
    Ok(match category {
        PhysicalValueCategory::Void => JvmTypeDescriptor::Void,
        PhysicalValueCategory::I32 => JvmTypeDescriptor::Int,
        PhysicalValueCategory::I64 => JvmTypeDescriptor::Long,
        PhysicalValueCategory::F32 => JvmTypeDescriptor::Float,
        PhysicalValueCategory::F64 => JvmTypeDescriptor::Double,
        PhysicalValueCategory::Reference => JvmTypeDescriptor::Object("java/lang/Object".to_string()),
    })
}

pub(crate) fn require_exact_function(submission: &FragmentSubmission, operation: &nyar::QualifiedName) -> Result<()> {
    let Some(executable) = &submission.executable
    else {
        return Err(miette!("physical contract failed [BPHYS008] {} at entry: JVM entry requires Semantic MIR", operation));
    };
    executable
        .get_function(operation)
        .map(|_| ())
        .ok_or_else(|| miette!("physical contract failed [BPHYS008] {} at entry: JVM entry has no exact semantic function", operation))
}

#[cfg(test)]
mod tests {
    use super::{prepare, require_exact_function, verify_emitted_method};
    use crate::executable_provider::MirFunctionMapProvider;
    use nyar::{Identifier, QualifiedName};
    use nyar_types::{Block, BlockRef, ExecutableFunction, NyarType, Operand, Terminator, ValueRef};
    use std::{collections::BTreeMap, sync::Arc};

    fn function(symbol: &str, return_type: NyarType, parameters: Vec<NyarType>) -> ExecutableFunction {
        let values = parameters.iter().enumerate().map(|(index, ty)| (ValueRef(index as u32), ty.clone())).collect();
        ExecutableFunction {
            symbol: symbol.to_string(),
            return_type,
            param_types: parameters,
            value_types: values,
            entry: BlockRef(0),
            values: Vec::new(),
            intrinsic: None,
            suspend_points: Vec::new(),
            frame_layouts: Vec::new(),
            continuations: Vec::new(),
            case_chains: Vec::new(),
            #[allow(deprecated)]
            state_machine: None,
            suspend_plan: None,
            state_machine_lowered: true,
            blocks: vec![Block {
                id: BlockRef(0),
                label: "entry".to_string(),
                parameters: Vec::new(),
                instructions: Vec::new(),
                terminator: Terminator::Return { value: None },
            }],
            diagnostics: Vec::new(),
        }
    }

    #[test]
    fn plan_maps_categories_to_exact_jvm_signature_and_slots() {
        let operation = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("shape")]);
        let function = function(
            "neutral.shape",
            NyarType::Integer32 { signed: true },
            vec![
                NyarType::Integer64 { signed: true },
                NyarType::Integer32 { signed: true },
                NyarType::Named(nyar_types::Identifier::new("Payload")),
            ],
        );
        let submission = crate::FragmentSubmission {
            executable: Some(Arc::new(MirFunctionMapProvider::new(BTreeMap::from([(operation, function)])))),
            ..Default::default()
        };
        let plan = prepare(&submission).expect("JVM physical plan").pop().expect("function plan");
        assert_eq!(plan.descriptor.parameter_types.len(), 3);
        assert_eq!(plan.descriptor.parameter_types[0], crate::nyar_backend_jvm::JvmTypeDescriptor::Long);
        assert_eq!(plan.descriptor.parameter_types[1], crate::nyar_backend_jvm::JvmTypeDescriptor::Int);
        assert_eq!(plan.descriptor.parameter_types[2], crate::nyar_backend_jvm::JvmTypeDescriptor::Object("java/lang/Object".to_string()));
        assert_eq!(plan.locals[0].slot, 0);
        assert_eq!(plan.locals[1].slot, 2);
        assert_eq!(plan.locals[2].slot, 3);
    }

    #[test]
    fn entry_plan_requires_exact_semantic_function() {
        let operation = QualifiedName::new(vec![Identifier::new("neutral"), Identifier::new("missing")]);
        let submission = crate::FragmentSubmission { entry_operation: Some(operation.clone()), ..Default::default() };
        assert!(require_exact_function(&submission, &operation).expect_err("missing entry must fail closed").to_string().contains("BPHYS008"));
    }

    fn method(
        descriptor: crate::nyar_backend_jvm::JvmMethodDescriptor,
        instructions: Vec<crate::nyar_backend_jvm::JvmInstruction>,
    ) -> crate::nyar_backend_jvm::JvmMethodSignature {
        crate::nyar_backend_jvm::JvmMethodSignature {
            name: "neutral".to_string(),
            descriptor,
            access_flags: 0x0001 | 0x0008,
            code: Some(crate::nyar_backend_jvm::JvmCodeBody { max_stack: 8, max_locals: 8, instructions }),
        }
    }

    #[test]
    fn verifier_rejects_stack_underflow_before_class_emission() {
        let method = method(
            crate::nyar_backend_jvm::JvmMethodDescriptor::new(Vec::new(), crate::nyar_backend_jvm::JvmTypeDescriptor::Int),
            vec![crate::nyar_backend_jvm::JvmInstruction::IReturn],
        );
        assert!(verify_emitted_method(&method).expect_err("return without value must fail").to_string().contains("BPHYS012"));
    }

    #[test]
    fn verifier_rejects_wrong_call_operand_category() {
        let reference = crate::nyar_backend_jvm::JvmMethodRef {
            owner: "neutral/Host".to_string(),
            name: "target".to_string(),
            descriptor: crate::nyar_backend_jvm::JvmMethodDescriptor::new(
                vec![crate::nyar_backend_jvm::JvmTypeDescriptor::Long],
                crate::nyar_backend_jvm::JvmTypeDescriptor::Int,
            ),
        };
        let method = method(
            crate::nyar_backend_jvm::JvmMethodDescriptor::new(Vec::new(), crate::nyar_backend_jvm::JvmTypeDescriptor::Int),
            vec![
                crate::nyar_backend_jvm::JvmInstruction::IConst(1),
                crate::nyar_backend_jvm::JvmInstruction::InvokeStatic(reference),
                crate::nyar_backend_jvm::JvmInstruction::IReturn,
            ],
        );
        assert!(verify_emitted_method(&method).expect_err("int cannot satisfy long call parameter").to_string().contains("BPHYS012"));
    }
}
