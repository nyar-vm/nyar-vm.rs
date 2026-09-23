use nyar_language::{
    MirLowerer, MirOperation, ValkyrieCompiler,
    types::{SourceID, hir::HirExprKind},
};

#[test]
fn hoists_anonymous_class_with_captures() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9300 })
        .compile_source(
            r#"
micro make_point(radius: f64) {
    class {
        x: 10.0,
        y: 20.0,
        radius: radius,
    }
}
"#,
        )
        .expect("compile");

    let make_point = hir.functions.iter().find(|f| f.name.as_str() == "make_point").expect("make_point");
    let Some(expr) = make_point.body.expr.as_ref()
    else {
        panic!("expected tail expr");
    };
    let HirExprKind::AnonymousClass { class_name: Some(name), captures, parents, .. } = &expr.kind
    else {
        panic!("expected anonymous class expr, got {:?}", expr.kind);
    };
    assert!(parents.is_empty());
    assert!(name.as_str().starts_with("__anon_"));
    assert!(!captures.is_empty());
    assert!(captures.iter().any(|cap| cap.identifier.name.as_str() == "radius"));
    assert!(hir.structs.iter().any(|class| class.name.as_str() == name.as_str()));
}

#[test]
fn lowers_anonymous_class_to_struct_new_in_mir() {
    let source = r#"
micro make_point(radius: f64) {
    class {
        x: 1.0,
        radius: radius,
    }
}
"#;
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9301 }).compile_source(source).expect("compile");
    let mir = MirLowerer::lower_module(&hir);
    assert!(mir.functions.iter().any(|function| {
        function.blocks.iter().any(|block| block.instructions.iter().any(|ins| matches!(ins.kind, MirOperation::StructNew { .. })))
    }));
}

#[test]
fn parses_anonymous_class_with_trait_parents() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9302 })
        .compile_source(
            r#"
trait AnonBase {}

micro make_drawable(radius: f64) {
    class: AnonBase {
        radius: radius,
    }
}
"#,
        )
        .expect("compile");

    let make_drawable = hir.functions.iter().find(|f| f.name.as_str() == "make_drawable").expect("make_drawable");
    let Some(expr) = make_drawable.body.expr.as_ref()
    else {
        panic!("expected tail expr");
    };
    let HirExprKind::AnonymousClass { parents, .. } = &expr.kind
    else {
        panic!("expected anonymous class");
    };
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].name.parts().last().map(|p| p.as_str()), Some("AnonBase"));
}

#[test]
fn parses_anonymous_class_with_parenthesized_inheritance() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9304 })
        .compile_source(
            r#"
class Animal {
    micro speak(self) -> utf8 { "..." }
}

micro make_dog() {
    class(Animal) {
        micro speak(self) -> utf8 { "woof" }
    }
}
"#,
        )
        .expect("compile");

    let make_dog = hir.functions.iter().find(|f| f.name.as_str() == "make_dog").expect("make_dog");
    let Some(expr) = make_dog.body.expr.as_ref()
    else {
        panic!("expected tail expr");
    };
    let HirExprKind::AnonymousClass { parents, class_name: Some(name), .. } = &expr.kind
    else {
        panic!("expected anonymous class");
    };
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].name.parts().last().map(|p| p.as_str()), Some("Animal"));
    let synthetic = hir.structs.iter().find(|s| s.name.as_str() == name.as_str()).expect("synthetic struct");
    assert_eq!(synthetic.parents.len(), 1);
    assert_eq!(synthetic.parents[0].name.parts().last().map(|p| p.as_str()), Some("Animal"));
}

#[test]
fn parses_anonymous_class_with_renamed_parenthesized_inheritance() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9305 })
        .compile_source(
            r#"
class FileReader {}

micro make_reader() {
    class(file: FileReader) {
        micro read(self) -> i32 { 1 }
    }
}
"#,
        )
        .expect("compile");

    let make_reader = hir.functions.iter().find(|f| f.name.as_str() == "make_reader").expect("make_reader");
    let Some(expr) = make_reader.body.expr.as_ref()
    else {
        panic!("expected tail expr");
    };
    let HirExprKind::AnonymousClass { parents, .. } = &expr.kind
    else {
        panic!("expected anonymous class");
    };
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].alias.as_ref().map(|alias| alias.as_str()), Some("file"));
    assert_eq!(parents[0].name.parts().last().map(|p| p.as_str()), Some("FileReader"));
}

#[test]
fn parses_empty_parenthesized_anonymous_class() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9306 })
        .compile_source(
            r#"
micro make_point() {
    class() {
        x: 1.0,
        y: 2.0,
    }
}
"#,
        )
        .expect("compile");

    let make_point = hir.functions.iter().find(|f| f.name.as_str() == "make_point").expect("make_point");
    let Some(expr) = make_point.body.expr.as_ref()
    else {
        panic!("expected tail expr");
    };
    let HirExprKind::AnonymousClass { parents, class_name: Some(name), .. } = &expr.kind
    else {
        panic!("expected anonymous class");
    };
    assert!(parents.is_empty());
    assert!(name.as_str().starts_with("__anon_"));
}

#[test]
fn rejects_anonymous_class_with_colon_but_no_trait_bound() {
    let err = ValkyrieCompiler::new(SourceID { version_id: 9303 })
        .compile_source(
            r#"
micro demo() {
    class: {}
}
"#,
        )
        .expect_err("class: {} must fail without trait bound");
    let message = err.to_string();
    assert!(message.contains("trait bound"), "unexpected error: {message}");
}

#[test]
fn hoists_anonymous_structure_with_captures() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9400 })
        .compile_source(
            r#"
micro make_point(radius: f64) {
    structure {
        x: 10.0,
        y: 20.0,
        radius: radius,
    }
}
"#,
        )
        .expect("compile");

    let make_point = hir.functions.iter().find(|f| f.name.as_str() == "make_point").expect("make_point");
    let Some(expr) = make_point.body.expr.as_ref()
    else {
        panic!("expected tail expr");
    };
    let HirExprKind::AnonymousClass { is_value_type, class_name: Some(name), captures, parents, .. } = &expr.kind
    else {
        panic!("expected anonymous structure expr, got {:?}", expr.kind);
    };
    assert!(*is_value_type);
    assert!(parents.is_empty());
    assert!(name.as_str().starts_with("__anon_"));
    assert!(captures.iter().any(|cap| cap.identifier.name.as_str() == "radius"));
    let synthetic = hir.structs.iter().find(|s| s.name.as_str() == name.as_str()).expect("synthetic structure");
    assert!(synthetic.is_value_type);
}

#[test]
fn parses_anonymous_structure_with_parenthesized_inheritance() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9401 })
        .compile_source(
            r#"
structure Vec2 {
    x: f64,
    y: f64,
}

micro make_vec3() {
    structure(Vec2) {
        x: 1.0,
        y: 2.0,
        z: 3.0,
    }
}
"#,
        )
        .expect("compile");

    let make_vec3 = hir.functions.iter().find(|f| f.name.as_str() == "make_vec3").expect("make_vec3");
    let Some(expr) = make_vec3.body.expr.as_ref()
    else {
        panic!("expected tail expr");
    };
    let HirExprKind::AnonymousClass { is_value_type, parents, class_name: Some(name), .. } = &expr.kind
    else {
        panic!("expected anonymous structure");
    };
    assert!(*is_value_type);
    assert_eq!(parents.len(), 1);
    assert_eq!(parents[0].name.parts().last().map(|p| p.as_str()), Some("Vec2"));
    let synthetic = hir.structs.iter().find(|s| s.name.as_str() == name.as_str()).expect("synthetic structure");
    assert!(synthetic.is_value_type);
    assert_eq!(synthetic.parents.len(), 1);
}

#[test]
fn parses_empty_parenthesized_anonymous_structure() {
    let hir = ValkyrieCompiler::new(SourceID { version_id: 9402 })
        .compile_source(
            r#"
micro make_point() {
    structure() {
        x: 1.0,
    }
}
"#,
        )
        .expect("compile");

    let make_point = hir.functions.iter().find(|f| f.name.as_str() == "make_point").expect("make_point");
    let Some(expr) = make_point.body.expr.as_ref()
    else {
        panic!("expected tail expr");
    };
    let HirExprKind::AnonymousClass { is_value_type, parents, .. } = &expr.kind
    else {
        panic!("expected anonymous structure");
    };
    assert!(*is_value_type);
    assert!(parents.is_empty());
}
