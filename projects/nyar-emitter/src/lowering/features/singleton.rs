//! Singleton instance lowering shared across backends.

use std::collections::BTreeMap;

use crate::nyar_backend_wasi::WasmBinaryModule;
#[cfg(feature = "legacy-lanes")]
use crate::{
    nyar_backend_clr::{
        MsilInstruction, MsilInstructionOperand, MsilMethodBody, MsilMethodRef, MsilMethodSignature, MsilModule, MsilOpcode, MsilType,
    },
    nyar_backend_jvm::{
        JvmClassFile, JvmCodeBody, JvmFieldSignature, JvmInstruction, JvmMethodDescriptor, JvmMethodSignature, JvmTypeDescriptor,
    },
};
#[cfg(feature = "legacy-lanes")]
use nyar::NyarType;
use nyar_types::{AggregateLayout, SINGLETON_CONSTRUCTOR_NAME, SINGLETON_FINALIZER_NAME, SingletonInstancePlan};
#[cfg(feature = "legacy-lanes")]
use nyar_types::{FieldLayout, SINGLETON_UNLOAD_ACCESSOR};
use std_data::binary::{
    nyar_ir::{NyarConstant, NyarExport, NyarExportKind, NyarFunction, NyarGlobal, NyarHeadCode, NyarModuleData},
    pe::NativeImageBuilder,
};
#[cfg(feature = "legacy-lanes")]
use std_data::{
    binary::class::{JvmFieldRef, JvmMethodRef},
    text::msil::{MsilField, MsilTypeDef},
};

#[cfg(feature = "legacy-lanes")]
use super::{clr_mir::lower_mir_function_to_msil, clr_types::nyar_type_to_msil, jvm_mir::lower_mir_function_to_jvm};
use crate::{FragmentSubmission, lowering::backends::wasm};

#[cfg(feature = "legacy-lanes")]
const INIT_LOCK_FIELD: &str = "__singleton_init_lock";
#[cfg(feature = "legacy-lanes")]
const JVM_ACC_PUBLIC: u16 = 0x0001;
#[cfg(feature = "legacy-lanes")]
const JVM_ACC_STATIC: u16 = 0x0008;

#[cfg(feature = "legacy-lanes")]
pub(crate) fn build_jvm_singleton_classes(submission: &FragmentSubmission) -> Vec<JvmClassFile> {
    submission
        .singleton_instances
        .iter()
        .filter_map(|plan| singleton_layout(submission, plan).map(|layout| build_jvm_singleton_class(submission, plan, &layout)))
        .collect()
}

/// 为非 singleton 的聚合布局类型和 sum type 生成占位 JVM 类文件。
///
/// JVM 后端需要所有在方法签名中被引用为 `Object` 的类型都存在对应的 class 文件，
/// 否则运行时会抛出 `NoClassDefFoundError`。值类型在直接使用时会被 `effective_jvm_type`
/// 展平为基础类型，但在数组元素位置时仍以 `Object("TypeName")` 形式出现在描述符中，
/// 因此需要生成占位类。singleton 类型由 `build_jvm_singleton_classes` 单独处理，此处跳过。
#[cfg(feature = "legacy-lanes")]
pub(crate) fn build_jvm_struct_classes(submission: &FragmentSubmission) -> Vec<JvmClassFile> {
    let singleton_names: std::collections::BTreeSet<String> = submission.singleton_instances.iter().map(|plan| plan.name.clone()).collect();
    let sum_type_names: std::collections::BTreeSet<String> = submission.sum_types.iter().map(|sum_type| sum_type.name.clone()).collect();
    let mut generated_names = std::collections::BTreeSet::new();
    let mut classes = Vec::new();

    eprintln!(
        "[jvm_struct_debug] fragment={}, layouts={}, sum_types={}, singletons={}",
        submission.fragment_id,
        submission.aggregate_layouts.layouts.len(),
        submission.sum_types.len(),
        submission.singleton_instances.len()
    );
    for layout in &submission.aggregate_layouts.layouts {
        eprintln!("[jvm_struct_debug] layout: name={}, namespace={}, fields={}", layout.name, layout.namespace, layout.fields.len());
    }

    for layout in &submission.aggregate_layouts.layouts {
        if singleton_names.contains(&layout.name) || generated_names.contains(&layout.name) {
            continue;
        }
        classes.push(build_jvm_placeholder_class(&layout.name, &layout.fields, &sum_type_names));
        generated_names.insert(layout.name.clone());
    }

    for sum_type in &submission.sum_types {
        if generated_names.contains(&sum_type.name) {
            continue;
        }
        eprintln!("[jvm_struct_debug] sum_type: name={}", sum_type.name);
        classes.push(build_jvm_placeholder_class(&sum_type.name, &[], &sum_type_names));
        generated_names.insert(sum_type.name.clone());
    }

    eprintln!("[jvm_struct_debug] generated {} struct classes", classes.len());
    classes
}

/// 构建占位 JVM 类文件，包含字段列表和默认无参构造函数。
///
/// 占位类用于满足 JVM 类加载器对方法签名中被引用类型的解析需求，
/// 不包含任何业务逻辑方法。字段描述符由 `nyar_type_to_jvm_descriptor` 映射，
/// 与方法签名中的类型引用保持一致。
///
/// 对于 unite sum type 字段（如 `VonValue`/`Option`/`Result`），JVM 后端用 int
/// 句柄表示，字段描述符必须是 `I` 而非 `L<Type>;`。否则 `getfield` 会返回对象引用，
/// 而 `store_to_value` 使用 `istore` 期望 int，触发
/// VerifyError: "Expecting to find integer on stack"。
#[cfg(feature = "legacy-lanes")]
fn build_jvm_placeholder_class(
    internal_name: &str,
    fields: &[FieldLayout],
    sum_type_names: &std::collections::BTreeSet<String>,
) -> JvmClassFile {
    let mut class_file =
        JvmClassFile { internal_name: internal_name.to_string(), access_flags: JVM_ACC_PUBLIC | 0x0020, ..JvmClassFile::default() };
    for field in fields {
        let descriptor = field_descriptor_for_placeholder(&field.ty, sum_type_names);
        class_file.fields.push(JvmFieldSignature { name: field.name.clone(), descriptor, access_flags: JVM_ACC_PUBLIC });
    }
    class_file.methods.push(default_jvm_ctor(internal_name));
    class_file
}

/// 判断字段类型是否为 unite sum type。
///
/// Unite sum type（如 `VonValue`/`Option<T>`/`Result<T,E>`）在 JVM 后端用 int
/// 句柄表示，字段描述符必须是 `I`。此方法检查 `NyarType::Named` 和
/// `NyarType::Apply(Named, _)` 两种形式，与 `effective_jvm_type` 的判断对齐。
#[cfg(feature = "legacy-lanes")]
fn is_unite_sum_type_field(ty: &NyarType, sum_type_names: &std::collections::BTreeSet<String>) -> bool {
    let name = match ty {
        NyarType::Named(name) => name.as_str(),
        NyarType::Apply(base, _) => {
            if let NyarType::Named(name) = base.as_ref() {
                name.as_str()
            }
            else {
                return false;
            }
        }
        _ => return false,
    };
    sum_type_names.contains(name)
}

/// 占位类字段描述符：标量 unite → `I`，`[Unite]` → `[I`，与方法参数 ABI /
/// `effective_array_element_type` / `jvm_field_descriptor_for_class` 对齐。
#[cfg(feature = "legacy-lanes")]
fn field_descriptor_for_placeholder(ty: &NyarType, sum_type_names: &std::collections::BTreeSet<String>) -> JvmTypeDescriptor {
    if is_unite_sum_type_field(ty, sum_type_names) {
        return JvmTypeDescriptor::Int;
    }
    match ty {
        NyarType::Array(element) | NyarType::FixedArray { element, .. } => {
            JvmTypeDescriptor::array(field_descriptor_for_placeholder(element, sum_type_names))
        }
        _ => nyar_type_to_jvm_descriptor(ty),
    }
}

/// Append singleton metadata into native `.rdata`.
pub(crate) fn append_native_singleton_metadata(builder: &mut NativeImageBuilder, submission: &FragmentSubmission) {
    if submission.singleton_instances.is_empty() {
        return;
    }
    let payload = encode_singleton_metadata_payload(submission);
    builder.add_rdata("legion.singletons", payload.as_bytes());
}

/// Append singleton metadata into ELF `.rodata`.
pub(crate) fn append_elf_singleton_metadata(builder: &mut std_data::binary::elf::NativeElfImageBuilder, submission: &FragmentSubmission) {
    if submission.singleton_instances.is_empty() {
        return;
    }
    let payload = encode_singleton_metadata_payload(submission);
    builder.add_rodata("legion.singletons", payload.as_bytes());
}

/// Augment a NyarVM module with module globals, init thunks, and accessor exports.
///
/// `function_index_by_name` maps already-emitted NyarVM function names (including
/// singleton constructor `Counter__init` and finalizer `Counter__finalize` lowered
/// from MIR) to their indices, so accessor / init / unload thunks can `Call` them.
pub(crate) fn augment_nyar_module_with_singletons(
    submission: &FragmentSubmission,
    module: &mut NyarModuleData,
    function_index_by_name: &BTreeMap<String, i32>,
) {
    if submission.singleton_instances.is_empty() {
        return;
    }

    for plan in &submission.singleton_instances {
        let global_index = module.globals.len() as i32;
        let global_export_name = format!("{}.{}", plan.name, plan.instance_field);
        module.globals.push(NyarGlobal { name: global_export_name.clone(), type_name: plan.name.clone() });
        module.exports.push(NyarExport { kind: NyarExportKind::Global, symbol_name: global_export_name, function_index: global_index });

        let constructor_index = plan.constructor_mir_symbol().as_ref().and_then(|_symbol| {
            let export_name = nyar_singleton_method_export_name(&plan.name, SINGLETON_CONSTRUCTOR_NAME);
            function_index_by_name.get(&export_name).copied()
        });
        let finalizer_index = plan.finalizer_mir_symbol().as_ref().and_then(|_symbol| {
            let export_name = nyar_singleton_method_export_name(&plan.name, SINGLETON_FINALIZER_NAME);
            function_index_by_name.get(&export_name).copied()
        });

        if !plan.is_lazy {
            let init_offset = module.code_bytes.len() as i32;
            let mut emitter = NyarSingletonEmitter::new(module);
            emitter.emit_alloc_record(&plan.name);
            if let Some(ctor_index) = constructor_index {
                emitter.emit_plain(NyarHeadCode::Dup);
                emitter.emit_call_function(ctor_index);
            }
            emitter.emit_store_global(global_index);
            emitter.emit_return();
            let init_index = module.functions.len() as i32;
            module.functions.push(NyarFunction {
                name: format!("__init_singleton_{}", plan.name),
                arity: 0,
                local_count: 0,
                code_offset: init_offset,
                code_length: module.code_bytes.len() as i32 - init_offset,
            });
            module.init_function_indices.push(init_index);
        }

        let accessor_offset = module.code_bytes.len() as i32;
        let mut emitter = NyarSingletonEmitter::new(module);
        if plan.is_lazy {
            emitter.emit_lazy_accessor(global_index, &plan.name, constructor_index);
        }
        else {
            emitter.emit_load_global_return(global_index);
        }
        let accessor_index = module.functions.len() as i32;
        module.functions.push(NyarFunction {
            name: nyar_singleton_export_name(plan),
            arity: 0,
            local_count: 0,
            code_offset: accessor_offset,
            code_length: module.code_bytes.len() as i32 - accessor_offset,
        });
        module.exports.push(NyarExport {
            kind: NyarExportKind::Function,
            symbol_name: nyar_singleton_export_name(plan),
            function_index: accessor_index,
        });

        if plan.supports_unload() {
            let unload_offset = module.code_bytes.len() as i32;
            let mut emitter = NyarSingletonEmitter::new(module);
            emitter.emit_unload(global_index, finalizer_index);
            let unload_index = module.functions.len() as i32;
            module.functions.push(NyarFunction {
                name: nyar_singleton_unload_export_name(plan),
                arity: 0,
                local_count: 0,
                code_offset: unload_offset,
                code_length: module.code_bytes.len() as i32 - unload_offset,
            });
            module.exports.push(NyarExport {
                kind: NyarExportKind::Function,
                symbol_name: nyar_singleton_unload_export_name(plan),
                function_index: unload_index,
            });
        }
    }
}

struct NyarSingletonEmitter<'a> {
    module: &'a mut NyarModuleData,
}

impl<'a> NyarSingletonEmitter<'a> {
    fn new(module: &'a mut NyarModuleData) -> Self {
        Self { module }
    }

    fn intern_string(&mut self, value: &str) -> i32 {
        let index = self.module.constants.len() as i32;
        self.module.constants.push(NyarConstant::String(value.to_string()));
        index
    }

    fn emit_plain(&mut self, opcode: NyarHeadCode) {
        self.module.code_bytes.push(opcode as u8);
    }

    fn emit_imm1(&mut self, opcode: NyarHeadCode, operand: i32) {
        self.module.code_bytes.push(opcode as u8);
        self.module.code_bytes.extend_from_slice(&operand.to_le_bytes());
    }

    fn emit_call_native(&mut self, name: &str, arg_count: i32) {
        let name_index = self.intern_string(name);
        self.module.code_bytes.push(NyarHeadCode::CallNative as u8);
        self.module.code_bytes.extend_from_slice(&name_index.to_le_bytes());
        self.module.code_bytes.extend_from_slice(&arg_count.to_le_bytes());
    }

    fn emit_alloc_record(&mut self, type_name: &str) {
        let type_index = self.intern_string(type_name);
        self.emit_imm1(NyarHeadCode::Const, type_index);
        self.emit_call_native("alloc_record", 1);
    }

    fn emit_store_global(&mut self, global_index: i32) {
        self.emit_imm1(NyarHeadCode::StoreGlobal, global_index);
    }

    fn emit_load_global_return(&mut self, global_index: i32) {
        self.emit_imm1(NyarHeadCode::LoadGlobal, global_index);
        self.emit_plain(NyarHeadCode::Return);
    }

    fn emit_return(&mut self) {
        self.emit_plain(NyarHeadCode::Return);
    }

    fn emit_lazy_accessor(&mut self, global_index: i32, type_name: &str, constructor_index: Option<i32>) {
        self.emit_imm1(NyarHeadCode::LoadGlobal, global_index);
        self.emit_plain(NyarHeadCode::Dup);
        let jump_pos = self.emit_jump_if_true_placeholder();
        self.emit_plain(NyarHeadCode::Pop);
        self.emit_alloc_record(type_name);
        if let Some(ctor_index) = constructor_index {
            self.emit_plain(NyarHeadCode::Dup);
            self.emit_call_function(ctor_index);
        }
        self.emit_store_global(global_index);
        let return_target = self.module.code_bytes.len();
        self.patch_jump_offset(jump_pos, return_target);
        self.emit_imm1(NyarHeadCode::LoadGlobal, global_index);
        self.emit_plain(NyarHeadCode::Return);
    }

    /// Emits an `unload` thunk for a lazy singleton.
    ///
    /// Stack flow:
    /// 1. `LoadGlobal` pushes the current instance (or Null).
    /// 2. `JumpIfFalse` skips the body when the slot is already empty (Null).
    /// 3. If non-null and a finalizer exists, `Call` the finalizer with `self`
    ///    (finalizer arity is 1, so it pops the instance from the stack).
    /// 4. If non-null but no finalizer, `Pop` the instance.
    /// 5. Push `Null` and `StoreGlobal` to clear the slot.
    /// 6. `Return`.
    fn emit_unload(&mut self, global_index: i32, finalizer_index: Option<i32>) {
        self.emit_imm1(NyarHeadCode::LoadGlobal, global_index);
        let skip_pos = self.emit_jump_if_null_placeholder();
        if let Some(fin_index) = finalizer_index {
            self.emit_imm1(NyarHeadCode::Call, fin_index);
        }
        else {
            self.emit_plain(NyarHeadCode::Pop);
        }
        self.emit_load_null();
        self.emit_store_global(global_index);
        let return_target = self.module.code_bytes.len();
        self.patch_jump_offset(skip_pos, return_target);
        self.emit_plain(NyarHeadCode::Return);
    }

    fn emit_call_function(&mut self, function_index: i32) {
        self.emit_imm1(NyarHeadCode::Call, function_index);
    }

    /// 弹出栈顶一个值（对齐 NyarHeadCode::Pop）。
    fn emit_pop(&mut self) {
        self.emit_plain(NyarHeadCode::Pop);
    }

    fn emit_load_null(&mut self) {
        let null_const_index = self.ensure_null_constant();
        self.emit_imm1(NyarHeadCode::Const, null_const_index);
    }

    fn ensure_null_constant(&mut self) -> i32 {
        for (index, constant) in self.module.constants.iter().enumerate() {
            if matches!(constant, NyarConstant::Null) {
                return index as i32;
            }
        }
        let index = self.module.constants.len() as i32;
        self.module.constants.push(NyarConstant::Null);
        index
    }

    fn emit_jump_if_null_placeholder(&mut self) -> usize {
        let position = self.module.code_bytes.len();
        self.emit_imm1(NyarHeadCode::JumpIfFalse, 0);
        position
    }

    fn emit_jump_if_true_placeholder(&mut self) -> usize {
        let position = self.module.code_bytes.len();
        self.emit_imm1(NyarHeadCode::JumpIfTrue, 0);
        position
    }

    fn patch_jump_offset(&mut self, jump_position: usize, target: usize) {
        let offset = (target as i32) - (jump_position as i32);
        self.module.code_bytes[jump_position + 1..jump_position + 5].copy_from_slice(&offset.to_le_bytes());
    }
}

fn encode_singleton_metadata_payload(submission: &FragmentSubmission) -> String {
    submission.singleton_instances.iter().map(singleton_metadata_line).collect::<Vec<_>>().join("\n")
}

pub(crate) fn singleton_metadata_line(plan: &SingletonInstancePlan) -> String {
    let mode = if plan.is_lazy { "lazy" } else { "static" };
    let ctor = plan.constructor_symbol.as_deref().unwrap_or("-");
    let fin = plan.finalizer_symbol.as_deref().unwrap_or("-");
    let unload = if plan.supports_unload() { "unload" } else { "-" };
    format!("{}|{}|{}|{}|{}|{}|{}|{}", plan.namespace, plan.name, plan.instance_field, mode, plan.accessor_method(), ctor, fin, unload,)
}

fn nyar_singleton_export_name(plan: &SingletonInstancePlan) -> String {
    format!("{}__{}", plan.name, plan.accessor_method())
}

/// Builds the NyarVM export name for a lazy singleton's `unload` accessor.
pub(crate) fn nyar_singleton_unload_export_name(plan: &SingletonInstancePlan) -> String {
    format!("{}__{}", plan.name, nyar_types::SINGLETON_UNLOAD_ACCESSOR)
}

pub(crate) fn nyar_singleton_accessor_export_name(plan: &SingletonInstancePlan) -> String {
    nyar_singleton_export_name(plan)
}

pub(crate) fn nyar_singleton_method_export_name(type_name: &str, method_name: &str) -> String {
    format!("{type_name}__{method_name}")
}

fn singleton_layout<'a>(submission: &'a FragmentSubmission, plan: &SingletonInstancePlan) -> Option<&'a AggregateLayout> {
    submission.aggregate_layouts.layouts.iter().find(|layout| layout.name == plan.name && layout.namespace == plan.namespace)
}

/// 计算 singleton plan 对应的 JVM 内部类名（斜杠分隔），与 `build_jvm_singleton_class` 生成的 `internal_name` 一致。
#[cfg(feature = "legacy-lanes")]
pub(crate) fn jvm_internal_name(plan: &SingletonInstancePlan) -> String {
    if plan.namespace.is_empty() { plan.name.clone() } else { format!("{}/{}", plan.namespace.replace('.', "/"), plan.name) }
}

#[cfg(feature = "legacy-lanes")]
fn nyar_type_to_jvm_descriptor(ty: &NyarType) -> JvmTypeDescriptor {
    match ty {
        NyarType::Bottom | NyarType::Unit => JvmTypeDescriptor::Int,
        NyarType::Boolean => JvmTypeDescriptor::Boolean,
        NyarType::Character => JvmTypeDescriptor::Char,
        NyarType::Integer8 { .. } => JvmTypeDescriptor::Byte,
        NyarType::Integer16 { .. } => JvmTypeDescriptor::Short,
        NyarType::Integer32 { .. } => JvmTypeDescriptor::Int,
        NyarType::Integer64 { .. } => JvmTypeDescriptor::Long,
        NyarType::Float32 => JvmTypeDescriptor::Float,
        NyarType::Float64 => JvmTypeDescriptor::Double,
        NyarType::Utf8 | NyarType::Utf16 => JvmTypeDescriptor::Object("java/lang/String".to_string()),
        // 数组字段必须与 `getfield`/`putfield` 描述符一致（`[B`/`[S`/`[I`…），
        // 不能折叠成 `Ljava/lang/Object;`，否则触发
        // VerifyError: "Incompatible type for getting or setting field"。
        NyarType::Array(element) | NyarType::FixedArray { element, .. } => JvmTypeDescriptor::array(nyar_type_to_jvm_descriptor(element)),
        NyarType::Named(name) => JvmTypeDescriptor::Object(name.to_string().replace('.', "/")),
        _ => JvmTypeDescriptor::Object("java/lang/Object".to_string()),
    }
}

#[cfg(feature = "legacy-lanes")]
fn build_jvm_singleton_class(submission: &FragmentSubmission, plan: &SingletonInstancePlan, layout: &AggregateLayout) -> JvmClassFile {
    let internal_name = jvm_internal_name(plan);
    let instance_descriptor = JvmTypeDescriptor::Object(internal_name.clone());
    let instance_field_ref =
        JvmFieldRef { owner: internal_name.clone(), name: plan.instance_field.clone(), descriptor: instance_descriptor.clone() };

    let mut class_file =
        JvmClassFile { internal_name: internal_name.clone(), access_flags: JVM_ACC_PUBLIC | 0x0020, ..JvmClassFile::default() };

    for field in &layout.fields {
        class_file.fields.push(JvmFieldSignature {
            name: field.name.clone(),
            descriptor: nyar_type_to_jvm_descriptor(&field.ty),
            access_flags: JVM_ACC_PUBLIC,
        });
    }
    class_file.fields.push(JvmFieldSignature {
        name: plan.instance_field.clone(),
        descriptor: instance_descriptor.clone(),
        access_flags: JVM_ACC_PUBLIC | JVM_ACC_STATIC,
    });

    class_file.methods.push(default_jvm_ctor(&internal_name));
    if plan.is_lazy {
        class_file.methods.push(jvm_lazy_get_instance(&internal_name, &instance_field_ref, &instance_descriptor, plan));
        if plan.supports_unload() {
            class_file.methods.push(jvm_unload(&internal_name, &instance_field_ref, plan));
        }
    }
    else {
        class_file.methods.push(jvm_eager_clinit(&internal_name, &instance_field_ref, plan));
        class_file.methods.push(jvm_eager_instance_accessor(&instance_field_ref, &instance_descriptor));
    }

    emit_jvm_singleton_instance_methods(submission, plan, &mut class_file);
    class_file
}

#[cfg(feature = "legacy-lanes")]
fn default_jvm_ctor(_internal_name: &str) -> JvmMethodSignature {
    JvmMethodSignature {
        name: "<init>".to_string(),
        descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
        access_flags: JVM_ACC_PUBLIC,
        code: Some(JvmCodeBody {
            max_stack: 1,
            max_locals: 1,
            instructions: vec![
                JvmInstruction::ALoad0,
                JvmInstruction::InvokeSpecial(JvmMethodRef {
                    owner: "java/lang/Object".to_string(),
                    name: "<init>".to_string(),
                    descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
                }),
                JvmInstruction::Return,
            ],
        }),
    }
}

#[cfg(feature = "legacy-lanes")]
fn jvm_eager_clinit(internal_name: &str, instance_field: &JvmFieldRef, plan: &SingletonInstancePlan) -> JvmMethodSignature {
    let mut instructions = vec![
        JvmInstruction::New(internal_name.to_string()),
        JvmInstruction::Dup,
        JvmInstruction::InvokeSpecial(JvmMethodRef {
            owner: internal_name.to_string(),
            name: "<init>".to_string(),
            descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
        }),
    ];
    if plan.constructor_symbol.is_some() {
        instructions.push(JvmInstruction::Dup);
        instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: internal_name.to_string(),
            name: SINGLETON_CONSTRUCTOR_NAME.to_string(),
            descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
        }));
    }
    instructions.push(JvmInstruction::PutStatic(instance_field.clone()));
    instructions.push(JvmInstruction::Return);
    JvmMethodSignature {
        name: "<clinit>".to_string(),
        descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
        access_flags: JVM_ACC_STATIC,
        code: Some(JvmCodeBody { max_stack: 3, max_locals: 0, instructions }),
    }
}

#[cfg(feature = "legacy-lanes")]
fn jvm_eager_instance_accessor(instance_field: &JvmFieldRef, return_type: &JvmTypeDescriptor) -> JvmMethodSignature {
    JvmMethodSignature {
        name: "instance".to_string(),
        descriptor: JvmMethodDescriptor::new(Vec::new(), return_type.clone()),
        access_flags: JVM_ACC_PUBLIC | JVM_ACC_STATIC,
        code: Some(JvmCodeBody {
            max_stack: 1,
            max_locals: 0,
            instructions: vec![JvmInstruction::GetStatic(instance_field.clone()), JvmInstruction::AReturn],
        }),
    }
}

#[cfg(feature = "legacy-lanes")]
fn jvm_lazy_get_instance(
    internal_name: &str,
    instance_field: &JvmFieldRef,
    return_type: &JvmTypeDescriptor,
    plan: &SingletonInstancePlan,
) -> JvmMethodSignature {
    let mut instructions = vec![
        JvmInstruction::GetStatic(instance_field.clone()),
        JvmInstruction::Dup,
        JvmInstruction::IfNonNull("return_instance".to_string()),
        JvmInstruction::Pop,
        JvmInstruction::New(internal_name.to_string()),
        JvmInstruction::Dup,
        JvmInstruction::InvokeSpecial(JvmMethodRef {
            owner: internal_name.to_string(),
            name: "<init>".to_string(),
            descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
        }),
    ];
    if plan.constructor_symbol.is_some() {
        instructions.push(JvmInstruction::Dup);
        instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: internal_name.to_string(),
            name: SINGLETON_CONSTRUCTOR_NAME.to_string(),
            descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
        }));
    }
    instructions.extend([
        JvmInstruction::PutStatic(instance_field.clone()),
        JvmInstruction::Label("return_instance".to_string()),
        JvmInstruction::GetStatic(instance_field.clone()),
        JvmInstruction::AReturn,
    ]);
    JvmMethodSignature {
        name: "get_instance".to_string(),
        descriptor: JvmMethodDescriptor::new(Vec::new(), return_type.clone()),
        access_flags: JVM_ACC_PUBLIC | JVM_ACC_STATIC,
        code: Some(JvmCodeBody { max_stack: 3, max_locals: 0, instructions }),
    }
}

/// 构建 lazy singleton 的 `unload` 静态方法。
///
/// 栈流：
/// 1. `GetStatic` 加载当前实例（或 null）。
/// 2. `IfNull` 为 null 时跳到 `unload_done`。
/// 3. 非空且存在终结器时，`GetStatic` + `InvokeVirtual finalize`（消费实例）。
/// 4. 非空但无终结器时不做额外操作。
/// 5. `AConstNull` + `PutStatic` 清空全局槽。
/// 6. `Label unload_done` + `Return`。
#[cfg(feature = "legacy-lanes")]
fn jvm_unload(internal_name: &str, instance_field: &JvmFieldRef, plan: &SingletonInstancePlan) -> JvmMethodSignature {
    let mut instructions = vec![JvmInstruction::GetStatic(instance_field.clone()), JvmInstruction::IfNull("unload_done".to_string())];
    if plan.finalizer_symbol.is_some() {
        instructions.push(JvmInstruction::GetStatic(instance_field.clone()));
        instructions.push(JvmInstruction::InvokeVirtual(JvmMethodRef {
            owner: internal_name.to_string(),
            name: SINGLETON_FINALIZER_NAME.to_string(),
            descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
        }));
    }
    instructions.extend([
        JvmInstruction::AConstNull,
        JvmInstruction::PutStatic(instance_field.clone()),
        JvmInstruction::Label("unload_done".to_string()),
        JvmInstruction::Return,
    ]);
    JvmMethodSignature {
        name: SINGLETON_UNLOAD_ACCESSOR.to_string(),
        descriptor: JvmMethodDescriptor::new(Vec::new(), JvmTypeDescriptor::Void),
        access_flags: JVM_ACC_PUBLIC | JVM_ACC_STATIC,
        code: Some(JvmCodeBody { max_stack: 2, max_locals: 0, instructions }),
    }
}

/// 将 singleton 实例方法体（含 `init`/`finalize`）从 `mir_functions` 降低到 JVM 类文件。
///
/// 每个限定名为 `SingletonName.method` 的 MIR 函数通过 `lower_mir_function_to_jvm` 降低，
/// 然后调整为实例方法：剥离首个 `self` 参数描述符、移除 `STATIC` 标志、重命名为方法名。
/// 访问器（`get_instance`/`instance`）和 `unload` 由调用方直接生成，在此跳过。
#[cfg(feature = "legacy-lanes")]
fn emit_jvm_singleton_instance_methods(submission: &FragmentSubmission, plan: &SingletonInstancePlan, class_file: &mut JvmClassFile) {
    let accessor = plan.accessor_method();
    let Some(exec) = &submission.executable
    else {
        return;
    };
    for operation in exec.operations() {
        let parts = operation.parts();
        if parts.len() != 2 || parts[0].as_str() != plan.name {
            continue;
        }
        let method_name = parts[1].as_str();
        if method_name == accessor || method_name == SINGLETON_UNLOAD_ACCESSOR {
            continue;
        }
        let Some(view) = exec.get_function(&operation)
        else {
            continue;
        };
        let mut method = lower_mir_function_to_jvm(submission, &operation, &view.function);
        method.name = method_name.to_string();
        if !method.descriptor.parameter_types.is_empty() {
            method.descriptor.parameter_types.remove(0);
        }
        method.access_flags = JVM_ACC_PUBLIC;
        class_file.methods.push(method);
    }
}

/// Augment an MSIL module with singleton static fields and initialization methods.
#[cfg(feature = "legacy-lanes")]
pub(crate) fn augment_msil_with_singletons(submission: &FragmentSubmission, module: &mut MsilModule) -> miette::Result<()> {
    for plan in &submission.singleton_instances {
        let Some(type_def) = find_msil_type(module, &plan.namespace, &plan.name)
        else {
            continue;
        };
        let qualified = plan.qualified_name();
        let instance_ty = MsilType::Named(plan.name.clone());

        type_def.fields.push(MsilField { name: plan.instance_field.clone(), ty: instance_ty.clone(), is_static: true });

        if plan.is_lazy {
            type_def.fields.push(MsilField { name: INIT_LOCK_FIELD.to_string(), ty: MsilType::Object, is_static: true });
            type_def.methods.push(lazy_get_instance_method(&qualified, plan));
            if plan.supports_unload() {
                type_def.methods.push(unload_method(&qualified, plan));
            }
        }
        else {
            type_def.methods.push(eager_static_constructor(&qualified, plan));
            type_def.methods.push(eager_instance_accessor(&qualified, plan));
        }

        emit_singleton_instance_methods(submission, plan, &qualified, type_def)?;
    }
    Ok(())
}

/// Lower singleton instance method bodies from `mir_functions` onto the singleton type def.
///
/// Each MIR function whose qualified name has the form `SingletonName.method` is lowered
/// via `lower_mir_function_to_msil`, then attached to the type def as an instance method.
/// The first MIR parameter (`self`) is stripped from the public signature because CLR
/// instance methods receive `this` implicitly via `ldarg.0`.
#[cfg(feature = "legacy-lanes")]
fn emit_singleton_instance_methods(
    submission: &FragmentSubmission,
    plan: &SingletonInstancePlan,
    qualified: &str,
    type_def: &mut MsilTypeDef,
) -> miette::Result<()> {
    let accessor = plan.accessor_method();
    let Some(exec) = &submission.executable
    else {
        return Ok(());
    };
    for operation in exec.operations() {
        let parts = operation.parts();
        if parts.len() != 2 || parts[0].as_str() != plan.name {
            continue;
        }
        let method_name = parts[1].as_str();
        if method_name == accessor {
            continue;
        }
        let Some(view) = exec.get_function(&operation)
        else {
            continue;
        };
        let mir_fn = &view.function;
        let mut body = lower_mir_function_to_msil(submission, &operation, mir_fn)?;
        body.method.owner = Some(qualified.to_string());
        body.method.name = method_name.to_string();
        let return_type = nyar_type_to_msil(&mir_fn.return_type, &submission.aggregate_layouts);
        let param_types = mir_fn.param_types.iter().skip(1).map(|ty| nyar_type_to_msil(ty, &submission.aggregate_layouts)).collect();
        body.method.signature = MsilMethodSignature::new(return_type, param_types);
        type_def.methods.push(body);
    }
    Ok(())
}

/// Append legion singleton metadata sections for backends without dedicated singleton slots.
pub(crate) fn append_singleton_metadata_sections(module: &mut WasmBinaryModule, submission: &FragmentSubmission) {
    for plan in &submission.singleton_instances {
        let payload = singleton_metadata_line(plan);
        module.sections.insert(
            0,
            crate::nyar_backend_wasi::WasmSection { id: 0, name: Some(format!("legion.singleton.{}", plan.name)), bytes: payload.into_bytes() },
        );
    }
}

/// 为 WASM 模块注入 singleton INSTANCE 全局与 accessor 导出函数。
///
/// 对每个 singleton plan 生成：
/// - 一个可变 `i32` 全局，存储 INSTANCE 线性内存指针（与 heap global 共存；heap 占 index 0）。
/// - 一个导出函数 `{name}__{accessor}`：
///   - eager 模式：直接 `global.get` 返回全局。
///   - lazy 模式：判空后经 `cabi_realloc(0,0,align,size)` 真实 bump 分配，写回全局。
pub(crate) fn augment_wasm_with_singleton_accessors(module: &mut WasmBinaryModule, submission: &FragmentSubmission) {
    if submission.singleton_instances.is_empty() {
        return;
    }

    let realloc_index = ensure_wasm_cabi_realloc(module);

    let import_count = wasm::count_wasm_function_imports(module);
    let decl_count = wasm::count_wasm_function_decls(module);
    let first_new_function_index = import_count + decl_count;

    let accessor_type_index = wasm::count_wasm_types(module);
    let accessor_type = wasm::wasm_function_type(&[], &[0x7F]);
    wasm::append_wasm_types(module, &[accessor_type]);

    let singleton_count = submission.singleton_instances.len() as u32;
    let global_base = wasm::append_wasm_i32_globals(module, &vec![0; singleton_count as usize]);

    let type_indices: Vec<u32> = vec![accessor_type_index; singleton_count as usize];
    wasm::append_wasm_function_decls(module, &type_indices);

    let exports: Vec<(String, u8, u32)> = submission
        .singleton_instances
        .iter()
        .enumerate()
        .map(|(index, plan)| (nyar_singleton_export_name(plan), 0x00, first_new_function_index + index as u32))
        .collect();
    wasm::append_wasm_exports(module, &exports);

    let bodies: Vec<Vec<u8>> = submission
        .singleton_instances
        .iter()
        .enumerate()
        .map(|(index, plan)| {
            let layout = singleton_layout(submission, plan);
            let size = layout.map(|layout| layout.size.max(8)).unwrap_or(8);
            let align = layout.map(|layout| layout.align.max(1)).unwrap_or(8);
            wasm_singleton_accessor_body(global_base + index as u32, plan.is_lazy, realloc_index, size, align)
        })
        .collect();
    wasm::append_wasm_code_bodies(module, &bodies);
}

fn ensure_wasm_cabi_realloc(module: &mut WasmBinaryModule) -> u32 {
    if wasm::count_wasm_globals(module) == 0 {
        wasm::insert_wasm_section(module, wasm::cabi_heap_global_section(wasm::CABI_HEAP_DEFAULT_BASE));
    }
    if let Some(index) = find_wasm_export_func(module, "cabi_realloc") {
        return index;
    }
    let realloc_type = wasm::count_wasm_types(module);
    wasm::append_wasm_types(module, &[wasm::wasm_function_type(&[0x7F, 0x7F, 0x7F, 0x7F], &[0x7F])]);
    let func_index = wasm::count_wasm_function_imports(module) + wasm::count_wasm_function_decls(module);
    wasm::append_wasm_function_decls(module, &[realloc_type]);
    wasm::append_wasm_code_bodies(module, &[wasm::wasm_cabi_realloc_bump_body()]);
    wasm::append_wasm_exports(module, &[("cabi_realloc".to_string(), 0x00, func_index)]);
    func_index
}

fn find_wasm_export_func(module: &WasmBinaryModule, name: &str) -> Option<u32> {
    let section = module.sections.iter().find(|item| item.id == 7)?;
    let bytes = &section.bytes;
    let mut pos = 0;
    let count = wasm::decode_uleb128(bytes, &mut pos);
    for _ in 0..count {
        let name_len = wasm::decode_uleb128(bytes, &mut pos) as usize;
        let export_name = std::str::from_utf8(&bytes[pos..pos + name_len]).ok()?;
        pos += name_len;
        let kind = *bytes.get(pos)?;
        pos += 1;
        let index = wasm::decode_uleb128(bytes, &mut pos);
        if export_name == name && kind == 0x00 {
            return Some(index);
        }
    }
    None
}

/// 生成 WASM accessor 函数体字节。
///
/// eager：`global.get N; end`
/// lazy：判空后 `cabi_realloc(0,0,align,size)` 分配并写回。
fn wasm_singleton_accessor_body(global_index: u32, is_lazy: bool, realloc_index: u32, size: u32, align: u32) -> Vec<u8> {
    let mut body = vec![0x00];
    if is_lazy {
        body.push(0x23);
        wasm::encode_uleb128(global_index, &mut body);
        body.push(0x45);
        body.push(0x04);
        body.push(0x40);
        body.push(0x41);
        body.push(0x00);
        body.push(0x41);
        body.push(0x00);
        body.push(0x41);
        wasm::encode_sleb128_i32(align.max(1) as i32, &mut body);
        body.push(0x41);
        wasm::encode_sleb128_i32(size.max(8) as i32, &mut body);
        body.push(0x10);
        wasm::encode_uleb128(realloc_index, &mut body);
        body.push(0x24);
        wasm::encode_uleb128(global_index, &mut body);
        body.push(0x0B);
        body.push(0x23);
        wasm::encode_uleb128(global_index, &mut body);
    }
    else {
        body.push(0x23);
        wasm::encode_uleb128(global_index, &mut body);
    }
    body.push(0x0B);
    body
}

/// 为 PE 镜像在 `.rdata` 中预留 singleton INSTANCE 存储槽。
///
/// 每个 singleton 预留 8 字节（指针大小），初始为 `0`（null）。
/// 槽标签为 `singleton_slot_{name}`。当前 PE 构建器不支持真正的 `.bss` 段，
/// 因此使用 `.rdata` 作为占位；当 Native runtime 支持可写数据段后，
/// 应迁移到 `.bss` 以获得真正的可变存储。
pub(crate) fn reserve_native_singleton_slots(builder: &mut NativeImageBuilder, submission: &FragmentSubmission) {
    for plan in &submission.singleton_instances {
        let label = format!("singleton_slot_{}", plan.name);
        builder.add_rdata(&label, &[0u8; 8]);
    }
}

/// 为 ELF 镜像在 `.rodata` 中预留 singleton INSTANCE 存储槽。
///
/// 与 `reserve_native_singleton_slots` 对称，使用 `.rodata` 作为占位。
/// 当 Native runtime 支持可写数据段后，应迁移到 `.bss`。
pub(crate) fn reserve_elf_singleton_slots(builder: &mut std_data::binary::elf::NativeElfImageBuilder, submission: &FragmentSubmission) {
    for plan in &submission.singleton_instances {
        let label = format!("singleton_slot_{}", plan.name);
        builder.add_rodata(&label, &[0u8; 8]);
    }
}

#[cfg(feature = "legacy-lanes")]
fn find_msil_type<'a>(module: &'a mut MsilModule, namespace: &str, name: &str) -> Option<&'a mut MsilTypeDef> {
    module.types.iter_mut().find(|type_def| type_def.namespace == namespace && type_def.full_name == name)
}

#[cfg(feature = "legacy-lanes")]
fn ctor_ref(qualified: &str) -> MsilMethodRef {
    MsilMethodRef {
        owner: Some(qualified.to_string()),
        name: ".ctor".to_string(),
        signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
    }
}

/// 构建 singleton 用户定义构造器 `init` 的实例方法引用。
///
/// `init` 由 `emit_singleton_instance_methods` 作为无参实例方法发出，
/// 此引用供 `.cctor` / `get_instance` 在 `Newobj` 之后调用以执行额外初始化逻辑。
#[cfg(feature = "legacy-lanes")]
fn init_method_ref(qualified: &str) -> MsilMethodRef {
    MsilMethodRef {
        owner: Some(qualified.to_string()),
        name: SINGLETON_CONSTRUCTOR_NAME.to_string(),
        signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
    }
}

/// 构建 singleton 用户定义终结器 `finalize` 的实例方法引用。
///
/// `finalize` 同样由 `emit_singleton_instance_methods` 作为无参实例方法发出，
/// 此引用供 lazy `unload` 访问器在清空全局槽之前调用以释放资源。
#[cfg(feature = "legacy-lanes")]
fn finalize_method_ref(qualified: &str) -> MsilMethodRef {
    MsilMethodRef {
        owner: Some(qualified.to_string()),
        name: SINGLETON_FINALIZER_NAME.to_string(),
        signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
    }
}

#[cfg(feature = "legacy-lanes")]
fn eager_static_constructor(qualified: &str, plan: &SingletonInstancePlan) -> MsilMethodBody {
    let mut instructions =
        vec![MsilInstruction { label: None, opcode: MsilOpcode::Newobj, operand: Some(MsilInstructionOperand::Method(ctor_ref(qualified))) }];
    if plan.constructor_symbol.is_some() {
        instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
        instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Call,
            operand: Some(MsilInstructionOperand::Method(init_method_ref(qualified))),
        });
    }
    instructions.push(MsilInstruction {
        label: None,
        opcode: MsilOpcode::Stsfld,
        operand: Some(MsilInstructionOperand::Field(qualified.to_string(), plan.instance_field.clone())),
    });
    instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None });
    MsilMethodBody {
        method: MsilMethodRef {
            owner: Some(qualified.to_string()),
            name: ".cctor".to_string(),
            signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
        },
        locals: Vec::new(),
        instructions,
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

#[cfg(feature = "legacy-lanes")]
fn eager_instance_accessor(qualified: &str, plan: &SingletonInstancePlan) -> MsilMethodBody {
    static_accessor(qualified, plan, "instance")
}

#[cfg(feature = "legacy-lanes")]
fn object_ctor_ref() -> MsilMethodRef {
    MsilMethodRef {
        owner: Some("[mscorlib]System.Object".to_string()),
        name: ".ctor".to_string(),
        signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
    }
}

#[cfg(feature = "legacy-lanes")]
fn monitor_method(name: &str) -> MsilMethodRef {
    MsilMethodRef {
        owner: Some("[mscorlib]System.Threading.Monitor".to_string()),
        name: name.to_string(),
        signature: MsilMethodSignature::new(MsilType::Void, vec![MsilType::Object]),
    }
}

#[cfg(feature = "legacy-lanes")]
fn lazy_get_instance_method(qualified: &str, plan: &SingletonInstancePlan) -> MsilMethodBody {
    let instance_field = MsilInstructionOperand::Field(qualified.to_string(), plan.instance_field.clone());
    let lock_field = MsilInstructionOperand::Field(qualified.to_string(), INIT_LOCK_FIELD.to_string());
    let mut instructions = vec![
        MsilInstruction { label: None, opcode: MsilOpcode::Ldsfld, operand: Some(instance_field.clone()) },
        MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Brtrue,
            operand: Some(MsilInstructionOperand::BranchTarget("return_instance".to_string())),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::Pop, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldsfld, operand: Some(lock_field.clone()) },
        MsilInstruction {
            label: Some("has_lock".to_string()),
            opcode: MsilOpcode::Brtrue,
            operand: Some(MsilInstructionOperand::BranchTarget("has_lock".to_string())),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::Newobj, operand: Some(MsilInstructionOperand::Method(object_ctor_ref())) },
        MsilInstruction { label: None, opcode: MsilOpcode::Stsfld, operand: Some(lock_field.clone()) },
        MsilInstruction { label: Some("has_lock".to_string()), opcode: MsilOpcode::Ldsfld, operand: Some(lock_field.clone()) },
        MsilInstruction { label: None, opcode: MsilOpcode::Call, operand: Some(MsilInstructionOperand::Method(monitor_method("Enter"))) },
        MsilInstruction { label: None, opcode: MsilOpcode::Ldsfld, operand: Some(instance_field.clone()) },
        MsilInstruction {
            label: Some("unlock_return".to_string()),
            opcode: MsilOpcode::Brtrue,
            operand: Some(MsilInstructionOperand::BranchTarget("unlock_return".to_string())),
        },
        MsilInstruction { label: None, opcode: MsilOpcode::Newobj, operand: Some(MsilInstructionOperand::Method(ctor_ref(qualified))) },
    ];
    if plan.constructor_symbol.is_some() {
        instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Dup, operand: None });
        instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Call,
            operand: Some(MsilInstructionOperand::Method(init_method_ref(qualified))),
        });
    }
    instructions.extend([
        MsilInstruction { label: None, opcode: MsilOpcode::Stsfld, operand: Some(instance_field.clone()) },
        MsilInstruction { label: Some("unlock_return".to_string()), opcode: MsilOpcode::Ldsfld, operand: Some(lock_field.clone()) },
        MsilInstruction { label: None, opcode: MsilOpcode::Call, operand: Some(MsilInstructionOperand::Method(monitor_method("Exit"))) },
        MsilInstruction { label: Some("return_instance".to_string()), opcode: MsilOpcode::Ldsfld, operand: Some(instance_field) },
        MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
    ]);
    MsilMethodBody {
        method: MsilMethodRef {
            owner: Some(qualified.to_string()),
            name: "get_instance".to_string(),
            signature: MsilMethodSignature::new(MsilType::Named(plan.name.clone()), Vec::new()),
        },
        locals: Vec::new(),
        instructions,
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

/// 构建 lazy singleton 的 `unload` 实例方法体。
///
/// 栈流：
/// 1. `Ldsfld` 加载当前实例（或 null）。
/// 2. `Brfalse` 为 null 时跳过终结逻辑。
/// 3. 非空且存在终结器时，重新 `Ldsfld` 加载实例并 `Call finalize`（消费 `this`）。
/// 4. 非空但无终结器时，不做额外操作（实例引用已被 `Brfalse` 弹出）。
/// 5. `Ldnull` + `Stsfld` 清空全局槽。
/// 6. `Ret`。
#[cfg(feature = "legacy-lanes")]
fn unload_method(qualified: &str, plan: &SingletonInstancePlan) -> MsilMethodBody {
    let instance_field = MsilInstructionOperand::Field(qualified.to_string(), plan.instance_field.clone());
    let mut instructions = vec![
        MsilInstruction { label: None, opcode: MsilOpcode::Ldsfld, operand: Some(instance_field.clone()) },
        MsilInstruction {
            label: None,
            opcode: MsilOpcode::Brfalse,
            operand: Some(MsilInstructionOperand::BranchTarget("unload_done".to_string())),
        },
    ];
    if plan.finalizer_symbol.is_some() {
        instructions.push(MsilInstruction { label: None, opcode: MsilOpcode::Ldsfld, operand: Some(instance_field.clone()) });
        instructions.push(MsilInstruction {
            label: None,
            opcode: MsilOpcode::Call,
            operand: Some(MsilInstructionOperand::Method(finalize_method_ref(qualified))),
        });
    }
    instructions.extend([
        MsilInstruction { label: None, opcode: MsilOpcode::Ldnull, operand: None },
        MsilInstruction { label: None, opcode: MsilOpcode::Stsfld, operand: Some(instance_field) },
        MsilInstruction { label: Some("unload_done".to_string()), opcode: MsilOpcode::Ret, operand: None },
    ]);
    MsilMethodBody {
        method: MsilMethodRef {
            owner: Some(qualified.to_string()),
            name: nyar_types::SINGLETON_UNLOAD_ACCESSOR.to_string(),
            signature: MsilMethodSignature::new(MsilType::Void, Vec::new()),
        },
        locals: Vec::new(),
        instructions,
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}

#[cfg(feature = "legacy-lanes")]
fn static_accessor(qualified: &str, plan: &SingletonInstancePlan, method_name: &str) -> MsilMethodBody {
    MsilMethodBody {
        method: MsilMethodRef {
            owner: Some(qualified.to_string()),
            name: method_name.to_string(),
            signature: MsilMethodSignature::new(MsilType::Named(plan.name.clone()), Vec::new()),
        },
        locals: Vec::new(),
        instructions: vec![
            MsilInstruction {
                label: None,
                opcode: MsilOpcode::Ldsfld,
                operand: Some(MsilInstructionOperand::Field(qualified.to_string(), plan.instance_field.clone())),
            },
            MsilInstruction { label: None, opcode: MsilOpcode::Ret, operand: None },
        ],
        max_stack: 8,
        is_entry_point: false,
        is_async: false,
    }
}
