//! Optional bytecode probe (`NYAR_DEMO_MODULE`).

use std::{env, fs};

use nyar_vm::NyarVm;
use std_data::binary::nyar_ir::{decode_at, NyarHeadCode};

#[test]
fn demo_disasm_max_area() {
    let Some(path) = env::var_os("NYAR_DEMO_MODULE") else {
        return;
    };
    let bytes = fs::read(path).expect("read");
    let module = NyarVm::new().load(&bytes).expect("load");
    for function in &module.functions {
        eprintln!("fn {} off={} len={}", function.name, function.code_offset, function.code_length);
    }
    for function in &module.functions {
        eprintln!("=== {} off={} len={} ===", function.name, function.code_offset, function.code_length);
        disasm_function(&module, function);
    }
}

fn disasm_function(module: &nyar_vm::module::LoadedModule, function: &std_data::binary::nyar_ir::NyarFunction) {
    let start = function.code_offset as usize;
    let end = start + function.code_length as usize;
    let mut ip = start;
    while ip < end {
        let instruction = decode_at(&module.code_bytes, ip);
        if !instruction.is_valid() {
            break;
        }
        match instruction.code {
            NyarHeadCode::CallImport => {
                let name = module
                    .imports
                    .get(instruction.operand1 as usize)
                    .map(|import| import.symbol_name.clone())
                    .unwrap_or_else(|| "?".to_string());
                eprintln!("{ip:04x}: CallImport {name} argc={}", instruction.operand2);
            }
            NyarHeadCode::Const => {
                let value = module.constant_at(instruction.operand1);
                eprintln!("{ip:04x}: Const {value:?}");
            }
            NyarHeadCode::Call => eprintln!("{ip:04x}: Call fn={}", instruction.operand1),
            NyarHeadCode::LoadLocal => eprintln!("{ip:04x}: LoadLocal local{}", instruction.operand1),
            NyarHeadCode::StoreLocal => eprintln!("{ip:04x}: StoreLocal local{}", instruction.operand1),
            NyarHeadCode::Pop => eprintln!("{ip:04x}: Pop"),
            NyarHeadCode::Return => eprintln!("{ip:04x}: Return"),
            other => eprintln!("{ip:04x}: {:?}", other),
        }
        ip += instruction.size as usize;
    }
}
