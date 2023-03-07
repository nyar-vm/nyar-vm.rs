//! 加载期模块校验：`CallImport` 下标与宿主导入边界失败关闭。

use std_data::binary::nyar_ir::{NyarHeadCode, NyarImport, NyarModuleData, NYAR_VERSION, decode_at};

use crate::error::NyarRuntimeError;

/// 与 emitter 约定的宿主导入模块名。
pub const HOST_IMPORT_MODULE: &str = "nyar.host";

/// 已删除的 `CallNative` 操作码（v1）；v2 模块不得再出现。
const OBSOLETE_CALL_NATIVE: u8 = 0xD1;

/// 种子宿主符号白名单（`nyar.host`）。未知符号在加载期失败，禁止热路径再猜。
fn is_known_host_symbol(symbol: &str) -> bool {
    matches!(
        symbol,
        "alloc_record"
            | "record_get"
            | "record_set"
            | "print"
            | "string_concat"
            | "console_log"
            | "i32_to_i64"
            | "i32_div"
            | "i64_add"
            | "i64_sub"
            | "i64_mul"
            | "i64_div"
            | "i64_rem"
            | "i64_neg"
            | "i64_eq"
            | "i64_ne"
            | "i64_lt"
            | "i64_le"
            | "i64_gt"
            | "i64_ge"
            | "bool_not"
            | "bool_and"
            | "bool_or"
    )
}

/// 校验已解码模块：版本、导入白名单、`CallImport` 下标、禁止旧 `CallNative`。
pub fn verify_module(data: &NyarModuleData) -> Result<(), NyarRuntimeError> {
    if data.version != NYAR_VERSION {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "unsupported module version {}; expected {NYAR_VERSION}",
            data.version
        )));
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

        if code == NyarHeadCode::CallImport {
            let import_index = instruction.operand1;
            if import_index < 0 || (import_index as usize) >= data.imports.len() {
                return Err(NyarRuntimeError::ImportIndexOutOfRange(import_index));
            }
        }

        pc = pc.saturating_add(instruction.size as usize);
    }

    Ok(())
}

fn verify_import(index: usize, import: &NyarImport) -> Result<(), NyarRuntimeError> {
    if import.module_name.is_empty() || import.symbol_name.is_empty() {
        return Err(NyarRuntimeError::ModuleLoad(format!("import[{index}] has empty module or symbol name")));
    }
    if import.module_name == HOST_IMPORT_MODULE && !is_known_host_symbol(&import.symbol_name) {
        return Err(NyarRuntimeError::ModuleLoad(format!(
            "unknown host import `{HOST_IMPORT_MODULE}::{}`",
            import.symbol_name
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std_data::binary::nyar_ir::{NyarImportKind, NyarModuleData};

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
            symbol_name: "alloc_record".into(),
        });
        let mut code = Vec::new();
        code.push(NyarHeadCode::CallImport as u8);
        code.extend_from_slice(&0i32.to_le_bytes());
        code.extend_from_slice(&1i32.to_le_bytes());
        module.code_bytes = code;
        verify_module(&module).expect("in-range CallImport");
    }
}
