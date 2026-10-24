//! Futamura / partial-evaluation pipeline proofs.
//!
//! Host-script PE product path: specialize(interpret, program) → **native residual**
//! → Windows PE (not OVM). `.nyar` / [`OdinVm`] remain probe-only for JS/Python stubs.

use std::collections::HashMap;

use legend_vm::{
    LegacyValue, LegacyVmRunner,
    compiler::{CompileArtifact, ResidualValue, eval_native_residual},
};
use odin_vm::{OdinVm, Value};

#[test]
fn bytecode_probe_stack_compiler_to_ovm_js_stub() {
    let runner = LegacyVmRunner::new();
    let artifact = runner.compile_module("javascript", "console.log('probe');", "probe").expect("compile module stub");
    let CompileArtifact::BytecodeProbe(module) = artifact
    else {
        panic!("JS stub must stay on bytecode probe lane");
    };
    let bytes = runner.compile_to_nyar(&module).expect("encode nyar bytes");
    let mut vm = OdinVm::new();
    let loaded = vm.load(&bytes).expect("load nyar module");
    let result = vm.run(&loaded, "main", Vec::new()).expect("run nyar module");
    assert_eq!(result, Value::I32(0));
}

#[test]
fn bytecode_probe_python_stub_uses_nyar_path() {
    let runner = LegacyVmRunner::new();
    let artifact = runner.compile_module("python", "print('probe')", "probe").expect("compile module stub");
    let CompileArtifact::BytecodeProbe(module) = artifact
    else {
        panic!("Python stub must stay on bytecode probe lane");
    };
    let bytes = runner.compile_to_nyar(&module).expect("encode nyar bytes");
    let mut vm = OdinVm::new();
    let loaded = vm.load(&bytes).expect("load nyar module");
    let result = vm.run(&loaded, "main", Vec::new()).expect("run nyar module");
    assert_eq!(result, Value::I32(0));
}

#[test]
fn lua_pe_specializes_to_native_residual_not_nyar() {
    let runner = LegacyVmRunner::new();
    let artifact = runner.compile_module("lua", "return 1", "ok").expect("lua compile_module");
    let CompileArtifact::Native(module) = artifact
    else {
        panic!("Lua PE must be Native residual, not bytecode / OVM");
    };
    assert!(module.is_native_bound());
    assert!(module.op_count() > 0);
    assert!(module.function("main").is_some());
}

#[test]
fn lua_pe_first_projection_arithmetic_matches_interpret_via_native_residual() {
    let source = "local x = 1 + 2\nreturn x\n";
    assert_native_pe_matches_interpret(source, |value| matches!(value, ResidualValue::Int(3)));
}

#[test]
fn lua_pe_first_projection_print_matches_interpret_via_native_residual() {
    let source = r#"
local x = 1 + 2
print(x)
"#;
    assert_native_pe_matches_interpret(source, |value| matches!(value, ResidualValue::Int(3)));
}

#[test]
fn lua_pe_first_projection_control_flow_matches_interpret_via_native_residual() {
    let source = r#"
local sum = 0
for i = 1, 3 do
  sum = sum + i
end
if sum == 6 then
  return sum
else
  return 0
end
"#;
    assert_native_pe_matches_interpret(source, |value| matches!(value, ResidualValue::Int(6)));
}

#[test]
fn lua_pe_first_projection_function_and_concat_matches_interpret_via_native_residual() {
    let source = r#"
function add(x, y)
  return x + y
end
local total = add(2, 3)
return "n:" .. total
"#;
    assert_native_pe_matches_interpret(source, |value| matches!(value, ResidualValue::String(text) if text == "n:5"));
}

#[test]
fn lua_pe_emits_windows_pe_from_native_residual() {
    let runner = LegacyVmRunner::new();
    let residual = runner.specialize_native("lua", "return 1 + 2", "lua_pe").expect("specialize");
    assert!(residual.is_native_bound());

    let dir = std::env::temp_dir().join("legend-vm-lua-pe-native");
    let _ = std::fs::create_dir_all(&dir);
    let out = dir.join("lua_pe.exe");
    let pe = runner.emit_native_pe(&residual, &out).expect("emit PE");
    assert!(pe.starts_with(b"MZ"), "native product must be a Windows PE (MZ), got {} bytes", pe.len());
    assert_eq!(std::fs::read(&out).expect("read pe"), pe);
}

fn assert_native_pe_matches_interpret(source: &str, residual_ok: impl FnOnce(&ResidualValue) -> bool) {
    let runner = LegacyVmRunner::new();
    let mut env = HashMap::new();
    let interpreted = runner.run("lua", source, &mut env).expect("interpret");

    let residual = runner.specialize_native("lua", source, "lua_pe").expect("specialize → native");
    assert!(residual.is_native_bound(), "Lua PE residual must be native-bound");
    assert!(residual.op_count() > 0, "specialize must residualize ops");

    let specialized = eval_native_residual(&residual).expect("eval native residual");
    assert!(residual_ok(&specialized), "residual result unexpected: {specialized:?}; interpret={interpreted:?}");
    assert_eq!(specialized.to_key(), legacy_key(&interpreted), "residual={specialized:?} interpret={interpreted:?}");

    let pe = legend_vm::compiler::emit_native_pe(&residual, None).expect("emit PE bytes");
    assert!(pe.starts_with(b"MZ"), "product path must emit PE, not .nyar");
}

fn legacy_key(value: &LegacyValue) -> String {
    value.to_string_value()
}
