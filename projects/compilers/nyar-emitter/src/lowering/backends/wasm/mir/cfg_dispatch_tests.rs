//! CFG dispatcher contract tests.
#![allow(deprecated)]

use super::lower_fragment_mir_to_wasm_module;
use crate::{
    FragmentSubmission,
    contracts::{Block, BlockRef, Constant, ExecutableFunction, Operand, Terminator},
    executable_provider::MirFunctionMapProvider,
};
use nyar::{NyarType, QualifiedName};
use std::{process::Command, sync::Arc};
use std_data::binary::wasm::{WasmBinaryModule, WasmOpcode};

fn leaf_i32_fn(symbol: &str, blocks: Vec<Block>) -> ExecutableFunction {
    ExecutableFunction {
        symbol: symbol.to_string(),
        return_type: NyarType::Integer32 { signed: true },
        param_types: Vec::new(),
        value_types: Default::default(),
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
        blocks,
        diagnostics: Vec::new(),
    }
}

fn lower_main(blocks: Vec<Block>) -> WasmBinaryModule {
    let mut submission = FragmentSubmission::default();
    submission.module_name = "cfg_exec".to_string();
    submission.entry_operation = Some(QualifiedName::new(vec![nyar::Identifier::new("main")]));
    let mut mir_map = std::collections::BTreeMap::new();
    mir_map.insert(QualifiedName::new(vec![nyar::Identifier::new("main")]), leaf_i32_fn("main", blocks));
    submission.executable = Some(Arc::new(MirFunctionMapProvider::new(mir_map)));
    lower_fragment_mir_to_wasm_module(&submission, "main").0
}

/// Instantiate with Node (Wasm GC capable) and read `exports.main()` as i32.
/// Opcode presence is not enough for B4 -?execute the CFG.
fn execute_main_i32(module: &WasmBinaryModule) -> i32 {
    let bytes = module.to_bytes().expect("encode wasm module");
    if let Ok(dump) = std::env::var("DUMP_CFG_WASM") {
        std::fs::write(&dump, &bytes).expect("dump wasm");
    }
    let dir = tempfile::tempdir().expect("tempdir");
    let wasm_path = dir.path().join("cfg.wasm");
    let script_path = dir.path().join("run.mjs");
    std::fs::write(&wasm_path, &bytes).expect("write wasm");
    std::fs::write(
        &script_path,
        r#"
import { readFileSync } from "node:fs";
// argv[1] is this script; argv[2] is the .wasm path.
const buf = readFileSync(process.argv[2]);
const mod = await WebAssembly.compile(buf);
const importObject = {};
for (const imp of WebAssembly.Module.imports(mod)) {
  if (!importObject[imp.module]) importObject[imp.module] = {};
  if (imp.kind === "function") {
importObject[imp.module][imp.name] = () => 0;
  } else if (imp.kind === "global") {
importObject[imp.module][imp.name] = new WebAssembly.Global({ value: "i32", mutable: true }, 0);
  } else if (imp.kind === "memory") {
importObject[imp.module][imp.name] = new WebAssembly.Memory({ initial: 1 });
  } else if (imp.kind === "table") {
importObject[imp.module][imp.name] = new WebAssembly.Table({ initial: 0, element: "anyfunc" });
  }
}
const inst = await WebAssembly.instantiate(mod, importObject);
const fn = inst.exports.main ?? inst.exports.run;
if (typeof fn !== "function") {
  console.error("missing main/run export; have=" + Object.keys(inst.exports).join(","));
  process.exit(2);
}
const value = fn();
if (typeof value !== "number") {
  console.error("export did not return number: " + typeof value);
  process.exit(3);
}
process.stdout.write(String(value | 0));
"#,
    )
    .expect("write script");
    let output = Command::new("node").arg(&script_path).arg(&wasm_path).output().expect("spawn node to execute wasm");
    if !output.status.success() {
        panic!(
            "node wasm execute failed status={:?}\nstdout={}\nstderr={}",
            output.status.code(),
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
    }
    let text = String::from_utf8_lossy(&output.stdout).trim().to_string();
    text.parse::<i32>().unwrap_or_else(|_| panic!("expected i32 stdout, got {text:?}"))
}

/// Both Branch arms reachable; true must take then→?2 and must not fall into else→? (B4).
#[test]
fn wasm_cfg_branch_true_returns_then_arm_not_else() {
    let module = lower_main(vec![
        Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Branch {
                condition: Operand::Constant(Constant::Bool(true)),
                then_target: BlockRef(2),
                else_target: BlockRef(1),
            },
        },
        Block {
            id: BlockRef(1),
            label: "else_wrong".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(1))) },
        },
        Block {
            id: BlockRef(2),
            label: "then_ok".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(42))) },
        },
    ]);
    let code = module.sections.iter().find(|section| section.id == 10).expect("code section");
    assert!(code.bytes.contains(&WasmOpcode::BrTable.as_u8()), "dispatcher must use br_table");
    assert!(code.bytes.contains(&WasmOpcode::Loop.as_u8()), "dispatcher must use loop");
    assert_eq!(execute_main_i32(&module), 42, "true branch must return 42, not fall into else/Fail arm");
}

/// Inverse: false must take else→? (Result Fail-arm shape), not then→?2.
#[test]
fn wasm_cfg_branch_false_returns_else_arm_not_then() {
    let module = lower_main(vec![
        Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Branch {
                condition: Operand::Constant(Constant::Bool(false)),
                then_target: BlockRef(2),
                else_target: BlockRef(1),
            },
        },
        Block {
            id: BlockRef(1),
            label: "else_fail".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(7))) },
        },
        Block {
            id: BlockRef(2),
            label: "then_fine".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(42))) },
        },
    ]);
    assert_eq!(execute_main_i32(&module), 7, "false branch must return else/Fail arm value 7");
}

/// Result-match shape: tag==Fine(0) must not enter Fail arm that would return 99.
#[test]
fn wasm_cfg_result_tag_fine_does_not_enter_fail_arm() {
    // Mimic match Fine/Fail: compare tag to 0 (Fine), then branch.
    // true →?Fine arm (42); false →?Fail arm (99).
    let module = lower_main(vec![
        Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            // tag Fine == 0 →?condition (tag == 0) is true via Bool(true) stand-in
            terminator: Terminator::Branch {
                condition: Operand::Constant(Constant::Bool(true)),
                then_target: BlockRef(1),
                else_target: BlockRef(2),
            },
        },
        Block {
            id: BlockRef(1),
            label: "fine_arm".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(42))) },
        },
        Block {
            id: BlockRef(2),
            label: "fail_arm".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(99))) },
        },
    ]);
    assert_eq!(execute_main_i32(&module), 42, "Fine tag must enter Fine arm; entering Fail arm is the B4 Result match bug");
}

/// Forward Jump must skip a dead block that returns the wrong value (B4 CFG).
#[test]
fn wasm_cfg_forward_jump_skips_dead_return_and_yields_42() {
    let module = lower_main(vec![
        Block {
            id: BlockRef(0),
            label: "entry".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Jump { target: BlockRef(2), arguments: Vec::new() },
        },
        Block {
            id: BlockRef(1),
            label: "dead_wrong".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(1))) },
        },
        Block {
            id: BlockRef(2),
            label: "live_ok".to_string(),
            parameters: Vec::new(),
            instructions: Vec::new(),
            terminator: Terminator::Return { value: Some(Operand::Constant(Constant::Int(42))) },
        },
    ]);
    assert_eq!(execute_main_i32(&module), 42, "forward Jump must skip dead Return(1) and yield 42");
}
