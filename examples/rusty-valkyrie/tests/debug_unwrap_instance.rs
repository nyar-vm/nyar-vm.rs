use rusty_valkyrie::ValkyrieCompiler;

#[test]
fn inspect_option_impl_unwrap_contract() {
    let source = r#"
unite Option<T> { Some { value: T }, None }
imply Option<T> {
    micro unwrap(self) -> T { match self { case Some(value): value case None: panic("x") } }
}
micro test(x: Option<i64>) -> i64 { return x.unwrap() }
"#;
    let compiler = ValkyrieCompiler::default();
    let root = rusty_valkyrie::frontend::parse_source(source).expect("parse");
    let hir = compiler.lower_root_with_semantic_exports_and_name(&root, &[], None).expect("lower");
    let impl_block = &hir.impls[0];
    eprintln!("impl target: {:?}", impl_block.target);
    let unwrap = impl_block.methods.iter().find(|m| m.name.as_str() == "unwrap").expect("unwrap");
    eprintln!("unwrap self ty: {:?}", unwrap.params[0].ty);
    eprintln!("unwrap return: {:?}", unwrap.return_type);
    let test = hir.functions.iter().find(|f| f.name.as_str() == "test").expect("test");
    eprintln!("x param ty: {:?}", test.params[0].ty);
}
