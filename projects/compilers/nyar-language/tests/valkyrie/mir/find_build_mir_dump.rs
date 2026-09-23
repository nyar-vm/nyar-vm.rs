use nyar_language::{MirLowerer, SourceID, ValkyrieCompiler};

#[test]
fn dump_find_build_target_mir() {
    let source = r#"
structure LegionBuildTargetOptions {
    public empty: bool
}
structure LegionBuildTarget {
    public name: utf8
    public options: LegionBuildTargetOptions
}
micro empty_opts() -> LegionBuildTargetOptions {
    return LegionBuildTargetOptions { empty: true }
}
micro canonical(name: utf8) -> utf8 { return name }
micro find_build_target(targets: [LegionBuildTarget], requested_target: utf8, canonical_target: utf8) -> LegionBuildTarget {
    let mut i: usize = 0
    while i < targets.length() {
        let candidate: LegionBuildTarget = targets[i]
        let candidate_canonical: utf8 = canonical(candidate.name)
        if candidate.name == requested_target || candidate_canonical == canonical_target {
            return candidate
        }
        i = i + 1
    }
    return LegionBuildTarget {
        name: requested_target,
        options: empty_opts()
    }
}
"#;
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9501 }).compile_source(source).expect("hir");
    let mir = MirLowerer::lower_module_semantic(&hir);
    let f = mir.functions.iter().find(|f| f.symbol.contains("find_build_target")).expect("fn");
    eprintln!("symbol={}", f.symbol);
    for block in &f.blocks {
        eprintln!("-- {} params={:?}", block.label, block.parameters);
        for ins in &block.instructions {
            eprintln!("  {:?}", ins);
        }
        eprintln!("  term={:?}", block.terminator);
    }
}
