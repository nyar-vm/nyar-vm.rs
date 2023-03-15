//! 加载期模块校验：`CallImport` 下标、`ObjectNew` layout、字段槽与宿主导入边界失败关闭。

use std_data::binary::nyar_ir::{NyarHeadCode, NyarImport, NyarModuleData, NYAR_VERSION, decode_at};

use crate::{
    error::NyarRuntimeError,
    host::resolve_import,
};

/// 已删除的 `CallNative` 操作码（v1）；v2 模块不得再出现。
const OBSOLETE_CALL_NATIVE: u8 = 0xD1;

/// 校验已解码模块：版本、导入白名单、`CallImport` / layout / field 下标、禁止旧 `CallNative`。
pub fn verify_module(data: &NyarModuleData) -> Result<(), NyarRuntimeError> {
    if data.version != NYAR_VERSION {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "unsupported module version {}; expected {NYAR_VERSION}",
            data.version
        )));
    }

    for (index, layout) in data.layouts.iter().enumerate() {
        if layout.field_count < 0 {
            return Err(NyarRuntimeError::ModuleLoad(format!(
                "layout[{index}] has negative field_count {}",
                layout.field_count
            )));
        }
    }

    for (index, import) in data.imports.iter().enumerate() {
        verify_import(index, import)?;
    }

    let mut pc = 0usize;
    while pc < data.code_bytes.len() {
        let opcode = data.code_bytes[pc];
        if opcode == OBSOLETE_CALL_NATIVE {
            return Err(NyarRuntimeError::ModuleLoad(format!(
                "obsolete CallNative opcode 0x{OBSOLETE_CALL_NATIVE:02X} at pc {pc}; use CallImport"
            )));
        }

        let Some(code) = NyarHeadCode::from_u8(opcode)
        else {
            return Err(NyarRuntimeError::UnknownOpcode(opcode));
        };

        let instruction = decode_at(&data.code_bytes, pc);
        if instruction.size == 0 {
            return Err(NyarRuntimeError::ModuleLoad(format!("truncated instruction at pc {pc}")));
        }

        match code {
            NyarHeadCode::CallImport => {
                let import_index = instruction.operand1;
                if import_index < 0 || (import_index as usize) >= data.imports.len() {
                    return Err(NyarRuntimeError::ImportIndexOutOfRange(import_index));
                }
            }
            NyarHeadCode::ObjectNew => {
                let layout_id = instruction.operand1;
                if layout_id < 0 || (layout_id as usize) >= data.layouts.len() {
                    return Err(NyarRuntimeError::LayoutIndexOutOfRange(layout_id));
                }
            }
            NyarHeadCode::FieldGet | NyarHeadCode::FieldSet => {
                let field_slot = instruction.operand1;
                if field_slot < 0 {
                    return Err(NyarRuntimeError::FieldSlotOutOfRange(field_slot));
                }
                // 热路径按对象自身 layout 校验上界；加载期仅拒绝负槽，并要求至少存在能容纳该槽的布局。
                let fits_some_layout = data.layouts.iter().any(|layout| field_slot < layout.field_count);
                if !fits_some_layout {
                    return Err(NyarRuntimeError::FieldSlotOutOfRange(field_slot));
                }
            }
            _ => {}
        }

        pc = pc.saturating_add(instruction.size as usize);
    }

    Ok(())
}

fn verify_import(index: usize, import: &NyarImport) -> Result<(), NyarRuntimeError> {
    if import.module_name.is_empty() || import.symbol_name.is_empty() {
        return Err(NyarRuntimeError::ModuleLoad(format!("import[{index}] has empty module or symbol name")));
    }
    // 解析一次：未知 `nyar.host` 符号在此失败；结果在 `LoadedModule` 侧缓存。
    let _ = resolve_import(import)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::host::HOST_IMPORT_MODULE;
    use std_data::binary::nyar_ir::{NyarImportKind, NyarLayout, NyarModuleData};

    fn empty_module() -> NyarModuleData {
        NyarModuleData {
            version: NYAR_VERSION,
            name: "test".into(),
            constants: Vec::new(),
            functions: Vec::new(),
            imports: Vec::new(),
            exports: Vec::new(),
            witness_entries: Vec::new(),
            code_bytes: Vec::new(),
            globals: Vec::new(),
            init_function_indices: Vec::new(),
            layouts: Vec::new(),
        }
    }

    #[test]
    fn accepts_empty_v2_module() {
        verify_module(&empty_module()).expect("empty v2 ok");
    }

    #[test]
    fn rejects_wrong_version() {
        let mut module = empty_module();
        module.version = 1;
        assert!(matches!(verify_module(&module), Err(NyarRuntimeError::ModuleLoad(_))));
    }

    #[test]
    fn rejects_unknown_host_import() {
        let mut module = empty_module();
        module.imports.push(NyarImport {
            kind: NyarImportKind::Function,
            module_name: HOST_IMPORT_MODULE.into(),
            symbol_name: "not_a_real_host_op".into(),
        });
        let err = verify_module(&module).expect_err("unknown host");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("unknown host import")));
    }

    #[test]
    fn rejects_call_import_out_of_range() {
        let mut module = empty_module();
        // CallImport import=0 argc=0，但 imports 表为空。
        let mut code = Vec::new();
        code.push(NyarHeadCode::CallImport as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&0i32.to_le_bytes());
        module.code_bytes = code;
        assert!(matches!(verify_module(&module), Err(NyarRuntimeError::ImportIndexOutOfRange(0))));
    }

    #[test]
    fn rejects_obsolete_call_native_byte() {
        let mut module = empty_module();
        module.code_bytes = vec![OBSOLETE_CALL_NATIVE, 0, 0, 0, 0, 0, 0, 0, 0];
        let err = verify_module(&module).expect_err("CallNative");
        assert!(matches!(err, NyarRuntimeError::ModuleLoad(message) if message.contains("CallNative")));
    }

    #[test]
    fn accepts_in_range_call_import() {
        let mut module = empty_module();
        module.imports.push(NyarImport {
            kind: NyarImportKind::Function,
            module_name: HOST_IMPORT_MODULE.into(),
            symbol_name: "print".into(),
        });
        let mut code = Vec::new();
        code.push(NyarHeadCode::CallImport as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes());
        module.code_bytes = code;
        verify_module(&module).expect("in-range CallImport");
    }

    #[test]
    fn rejects_object_new_out_of_range() {
        let mut module = empty_module();
        let mut code = Vec::new();
        code.push(NyarHeadCode::ObjectNew as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        module.code_bytes = code;
        assert!(matches!(verify_module(&module), Err(NyarRuntimeError::LayoutIndexOutOfRange(0))));
    }

    #[test]
    fn accepts_object_new_with_layout() {
        let mut module = empty_module();
        module.layouts.push(NyarLayout { field_count: 2 });
        let mut code = Vec::new();
        code.push(NyarHeadCode::ObjectNew as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        module.code_bytes = code;
        verify_module(&module).expect("ObjectNew ok");
    }

    #[test]
    fn rejects_field_slot_without_fitting_layout() {
        let mut module = empty_module();
        module.layouts.push(NyarLayout { field_count: 1 });
        let mut code = Vec::new();
        code.push(NyarHeadCode::FieldGet as u8);
        code.extend_from_slice(&1i32.to_le_bytes());
        module.code_bytes = code;
        assert!(matches!(verify_module(&module), Err(NyarRuntimeError::FieldSlotOutOfRange(1))));
    }
}
