use nyar_language::{ValkyrieCompiler, types::SourceID};

#[test]
fn nullable_some_on_utf8text() {
    let compiler = ValkyrieCompiler::new(SourceID { version_id: 9001 });
    let source = r#"
namespace demo;
structure Utf8Text { }

micro take(x: Utf8Text?) -> i32 {
    match x {
        case Some(v):
            return 1
        case None:
            return 0
    }
}
"#;
    match compiler.compile_source(source) {
        Ok(_) => println!("COMPILE_OK"),
        Err(e) => {
            println!("COMPILE_ERR: {e}");
            panic!("{e}");
        }
    }
}
