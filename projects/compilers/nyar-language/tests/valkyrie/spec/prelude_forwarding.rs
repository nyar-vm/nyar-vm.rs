//! SubTask 2.6: `_prelude` 转发机制验证。
//!
//! 验证 `_prelude/Option.v` 与 `_prelude/Result.v` 已改为纯转发层：
//! - 文件本身可解析，且不含任何 nominal type / 函数 / imply 定义。
//! - 与 `core::types` 真实定义合并后，未限定名（`option_none`、`Some` 等）
//!   仍能通过编译单元全局后缀匹配解析到 `core::types` 的定义。

use std::path::PathBuf;

use nyar_language::{MirFunction, MirModule, MirOperand, MirOperation, ValkyrieCompiler, types::hir::HirModule};

/// 返回 `valkyrie.v/projects/` 目录的绝对路径。
fn projects_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../valkyrie.v/projects")
}

/// 返回 `_prelude/Option.v` 的绝对路径。
fn prelude_option_path() -> PathBuf {
    projects_dir().join("std/source/_prelude/Option.v")
}

/// 返回 `_prelude/Result.v` 的绝对路径。
fn prelude_result_path() -> PathBuf {
    projects_dir().join("std/source/_prelude/Result.v")
}

/// 返回 `core/source/types/Option.v` 的绝对路径。
fn core_option_path() -> PathBuf {
    projects_dir().join("core/source/types/Option.v")
}

/// 返回 `core/source/types/Result.v` 的绝对路径。
fn core_result_path() -> PathBuf {
    projects_dir().join("core/source/types/Result.v")
}

/// 读取文件内容，缺失时 panic。
fn read_file(path: &PathBuf) -> String {
    std::fs::read_to_string(path).unwrap_or_else(|_| panic!("missing file: {}", path.display()))
}

/// 编译单个 `.v` 文件到 HIR。
fn compile_path(path: &PathBuf) -> HirModule {
    let compiler = ValkyrieCompiler::default();
    compiler.compile_path(path).unwrap_or_else(|error| panic!("failed to compile {}: {error:?}", path.display()))
}

// ---------------------------------------------------------------------------
// SubTask 2.5 / 2.6: 转发文件可解析性与纯转发性
// ---------------------------------------------------------------------------

#[test]
fn prelude_option_file_compiles_as_pure_forwarding() {
    let module = compile_path(&prelude_option_path());

    assert!(module.enums.is_empty(), "prelude Option.v must not define any unite/enum — it should be pure forwarding");
    assert!(module.functions.is_empty(), "prelude Option.v must not define any top-level function — it should be pure forwarding");
    assert!(module.impls.is_empty(), "prelude Option.v must not define any imply block — it should be pure forwarding");
    assert!(!module.imports.is_empty(), "prelude Option.v should contain at least one `using` import for forwarding");
}

#[test]
fn prelude_result_file_compiles_as_pure_forwarding() {
    let module = compile_path(&prelude_result_path());

    assert!(module.enums.is_empty(), "prelude Result.v must not define any unite/enum — it should be pure forwarding");
    assert!(module.functions.is_empty(), "prelude Result.v must not define any top-level function — it should be pure forwarding");
    assert!(module.impls.is_empty(), "prelude Result.v must not define any imply block — it should be pure forwarding");
    assert!(!module.imports.is_empty(), "prelude Result.v should contain at least one `using` import for forwarding");
}

// ---------------------------------------------------------------------------
// SubTask 2.5 / 2.6: 合并编译单元后未限定调用仍可解析
// ---------------------------------------------------------------------------

/// 在合并源码中查找符号以 `suffix` 结尾的函数。
fn find_function_by_suffix<'a>(mir: &'a MirModule, suffix: &str) -> Option<&'a MirFunction> {
    mir.functions.iter().find(|f| f.symbol == suffix || f.symbol.ends_with(&format!("::{suffix}")) || f.symbol.ends_with(&format!(".{suffix}")))
}

/// 收集函数内所有 `Call` 指令的被调用符号字符串。
fn call_callee_symbols(function: &MirFunction) -> Vec<String> {
    function
        .blocks
        .iter()
        .flat_map(|block| block.instructions.iter())
        .filter_map(|ins| match &ins.kind {
            MirOperation::Call { callee: MirOperand::Symbol(path), .. } => Some(path.to_string()),
            _ => None,
        })
        .collect()
}

#[test]
fn combined_option_source_resolves_unqualified_option_none() {
    let core_option = read_file(&core_option_path());
    let prelude_option = read_file(&prelude_option_path());
    let caller = r#"
namespace test.caller;

micro caller() -> Option<i32> {
    return option_none::<i32>()
}
"#;
    let combined = format!("{core_option}\n{prelude_option}\n{caller}");

    let compiler = ValkyrieCompiler::default();
    let mir = compiler.compile_source_to_mir(&combined).expect("combined core+prelude+caller source should compile to MIR");

    let caller_fn = find_function_by_suffix(&mir, "caller").expect("caller function should exist in MIR");

    let callees = call_callee_symbols(caller_fn);
    assert!(callees.iter().any(|symbol| symbol.ends_with("option_none")), "caller should invoke `option_none`, got callees: {callees:?}");
}

#[test]
fn combined_result_source_resolves_unqualified_fine() {
    let core_result = read_file(&core_result_path());
    let prelude_result = read_file(&prelude_result_path());
    let caller = r#"
namespace test.caller;

micro caller() -> Result<i32, i32> {
    return Fine(42)
}
"#;
    let combined = format!("{core_result}\n{prelude_result}\n{caller}");

    let compiler = ValkyrieCompiler::default();
    let mir = compiler.compile_source_to_mir(&combined).expect("combined core+prelude+caller source should compile to MIR");

    let caller_fn = find_function_by_suffix(&mir, "caller").expect("caller function should exist in MIR");

    let callees = call_callee_symbols(caller_fn);
    assert!(callees.iter().any(|symbol| symbol.ends_with("Fine")), "caller should invoke `Fine` constructor, got callees: {callees:?}");
}

#[test]
fn prelude_option_imports_reference_core_types() {
    let module = compile_path(&prelude_option_path());

    let has_core_types_import =
        module.imports.iter().any(|import| import.path.to_string().contains("core") && import.path.to_string().contains("types"));
    assert!(
        has_core_types_import,
        "prelude Option.v should import from `core::types`, got imports: {:?}",
        module.imports.iter().map(|i| i.path.to_string()).collect::<Vec<_>>()
    );
}

#[test]
fn prelude_result_imports_reference_core_types() {
    let module = compile_path(&prelude_result_path());

    let has_core_types_import =
        module.imports.iter().any(|import| import.path.to_string().contains("core") && import.path.to_string().contains("types"));
    assert!(
        has_core_types_import,
        "prelude Result.v should import from `core::types`, got imports: {:?}",
        module.imports.iter().map(|i| i.path.to_string()).collect::<Vec<_>>()
    );
}
