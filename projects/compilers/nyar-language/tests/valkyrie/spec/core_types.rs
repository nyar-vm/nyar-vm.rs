use std::path::PathBuf;

use nyar_language::{
    MirModule, ValkyrieCompiler,
    types::{Identifier, hir::HirModule},
};

/// 返回 `valkyrie.v/projects/core/source/types/` 目录的绝对路径。
fn core_types_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../valkyrie.v/projects/core/source/types")
}

/// 读取并编译 `core/source/types` 下的指定 `.v` 文件，返回 HIR 模块。
fn compile_core_type_file(name: &str) -> HirModule {
    let path = core_types_dir().join(name);
    assert!(path.exists(), "missing core type file: {}", path.display());
    let compiler = ValkyrieCompiler::default();
    compiler.compile_path(&path).unwrap_or_else(|error| panic!("failed to compile {}: {error:?}", path.display()))
}

/// 判断模块的 `imply` 块中是否存在指定名称的方法。
fn has_inherent_method(module: &HirModule, name: &str) -> bool {
    let target = Identifier::new(name);
    module.impls.iter().flat_map(|imp| imp.methods.iter()).any(|method| method.name == target)
}

/// 判断模块中是否存在指定名称的顶层函数。
fn has_top_function(module: &HirModule, name: &str) -> bool {
    let target = Identifier::new(name);
    module.functions.iter().any(|function| function.name == target)
}

/// 判断 MIR 中是否存在符号以 `suffix` 结尾的函数。
fn has_mir_function(mir: &MirModule, suffix: &str) -> bool {
    mir.functions.iter().any(|function| function.symbol.ends_with(suffix))
}

// ---------------------------------------------------------------------------
// SubTask 1.3: 可解析性验证 —— 读取真实迁移文件并编译到 HIR
// ---------------------------------------------------------------------------

#[test]
fn core_types_option_file_compiles_to_hir() {
    let module = compile_core_type_file("Option.v");

    let option = module
        .enums
        .iter()
        .find(|enum_def| enum_def.name == Identifier::new("Option"))
        .expect("Option unite should be present in core::types::Option.v");

    let variant_names: Vec<&Identifier> = option.variants.iter().map(|v| &v.name).collect();
    assert_eq!(variant_names, vec![&Identifier::new("Some"), &Identifier::new("None")]);

    let some = &option.variants[0];
    assert_eq!(some.fields.len(), 1);
    assert_eq!(some.fields[0].name, Identifier::new("value"));
}

#[test]
fn core_types_result_file_compiles_to_hir() {
    let module = compile_core_type_file("Result.v");

    let result = module
        .enums
        .iter()
        .find(|enum_def| enum_def.name == Identifier::new("Result"))
        .expect("Result unite should be present in core::types::Result.v");

    let variant_names: Vec<&Identifier> = result.variants.iter().map(|v| &v.name).collect();
    assert_eq!(variant_names, vec![&Identifier::new("Fine"), &Identifier::new("Fail")]);

    assert_eq!(result.variants[0].fields.len(), 1);
    assert_eq!(result.variants[0].fields[0].name, Identifier::new("value"));
    assert_eq!(result.variants[1].fields.len(), 1);
    assert_eq!(result.variants[1].fields[0].name, Identifier::new("error"));
}

// ---------------------------------------------------------------------------
// SubTask 1.4: 结构覆盖 —— 构造函数与 imply 方法齐全性
// ---------------------------------------------------------------------------

#[test]
fn core_types_option_file_exposes_constructors() {
    let module = compile_core_type_file("Option.v");
    assert!(has_top_function(&module, "Some"), "constructor `Some` should be present");
    assert!(has_top_function(&module, "option_none"), "constructor `option_none` should be present");
}

#[test]
fn core_types_option_imply_exposes_all_methods() {
    let module = compile_core_type_file("Option.v");
    for method in ["is_some", "is_none", "unwrap", "unwrap_or", "unwrap_or_else", "map", "map_or", "and_then", "or_else", "filter", "flatten"] {
        assert!(has_inherent_method(&module, method), "Option::{} should be present in imply block", method);
    }
}

#[test]
fn core_types_result_imply_exposes_all_methods() {
    let module = compile_core_type_file("Result.v");
    for method in ["unwrap", "unwrap_fail", "unwrap_or", "unwrap_or_else", "map", "and_then", "fold"] {
        assert!(has_inherent_method(&module, method), "Result::{} should be present in imply block", method);
    }
}

// ---------------------------------------------------------------------------
// SubTask 1.4: 行为覆盖 —— 构造 / match / 组合子模式 lowering 到 MIR
// ---------------------------------------------------------------------------

#[test]
fn option_construction_and_match_lower_to_mir() {
    let source = r#"namespace test;
unite Option<T> {
    Some { value: T },
    None,
}
micro make_some(value: i64) -> Option<i64> {
    let wrapped: Option<i64> = Some(value)
    return wrapped
}
micro classify(opt: Option<i64>) -> i64 {
    match opt {
        case Some(value):
            value
        case None:
            0
    }
}
"#;
    let compiler = ValkyrieCompiler::default();
    let mir = compiler.compile_source_to_mir(source).expect("option construction + match should lower to mir");
    assert!(has_mir_function(&mir, "::make_some"));
    assert!(has_mir_function(&mir, "::classify"));
}

#[test]
fn result_construction_and_match_lower_to_mir() {
    let source = r#"namespace test;
unite Result<T, E> {
    Fine { value: T },
    Fail { error: E },
}
micro make_fine(value: i64) -> Result<i64, i64> {
    let wrapped: Result<i64, i64> = Fine(value)
    return wrapped
}
micro classify(res: Result<i64, i64>) -> i64 {
    match res {
        case Fine(value):
            value
        case Fail(error):
            error
    }
}
"#;
    let compiler = ValkyrieCompiler::default();
    let mir = compiler.compile_source_to_mir(source).expect("result construction + match should lower to mir");
    assert!(has_mir_function(&mir, "::make_fine"));
    assert!(has_mir_function(&mir, "::classify"));
}

#[test]
fn option_unwrap_or_and_map_patterns_lower_to_mir() {
    let source = r#"namespace test;
unite Option<T> {
    Some { value: T },
    None,
}
micro unwrap_or(opt: Option<i64>, default: i64) -> i64 {
    match opt {
        case Some(value):
            value
        case None:
            default
    }
}
micro map_to_string(opt: Option<i64>) -> Option<i64> {
    match opt {
        case Some(value):
            Some(value)
        case None:
            None
    }
}
"#;
    let compiler = ValkyrieCompiler::default();
    let mir = compiler.compile_source_to_mir(source).expect("unwrap_or + map patterns should lower to mir");
    assert!(has_mir_function(&mir, "::unwrap_or"));
    assert!(has_mir_function(&mir, "::map_to_string"));
}

#[test]
fn option_filter_and_flatten_patterns_lower_to_mir() {
    let source = r#"namespace test;
unite Option<T> {
    Some { value: T },
    None,
}
micro keep_if_positive(opt: Option<i64>) -> Option<i64> {
    match opt {
        case Some(value) if value > 0:
            Some(value)
        else:
            None
    }
}
micro flatten(opt: Option<Option<i64>>) -> Option<i64> {
    match opt {
        case Some(value):
            value
        case None:
            None
    }
}
"#;
    let compiler = ValkyrieCompiler::default();
    let mir = compiler.compile_source_to_mir(source).expect("filter + flatten patterns should lower to mir");
    assert!(has_mir_function(&mir, "::keep_if_positive"));
    assert!(has_mir_function(&mir, "::flatten"));
}

#[test]
fn result_fold_and_unwrap_patterns_lower_to_mir() {
    let source = r#"namespace test;
unite Result<T, E> {
    Fine { value: T },
    Fail { error: E },
}
micro fold_result(res: Result<i64, i64>) -> i64 {
    match res {
        case Fine(value):
            value
        case Fail(error):
            error
    }
}
micro unwrap_or_default(res: Result<i64, i64>, default: i64) -> i64 {
    match res {
        case Fine(value):
            value
        case Fail(error):
            default
    }
}
"#;
    let compiler = ValkyrieCompiler::default();
    let mir = compiler.compile_source_to_mir(source).expect("fold + unwrap_or patterns should lower to mir");
    assert!(has_mir_function(&mir, "::fold_result"));
    assert!(has_mir_function(&mir, "::unwrap_or_default"));
}
