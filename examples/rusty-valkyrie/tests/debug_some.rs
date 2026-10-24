use rusty_valkyrie::ValkyrieCompiler;

const SOURCE: &str = r#"
namespace core::types;
unite Option<T> { Some { value: T }, None }
namespace core::text;
structure char { }
imply char {
    micro ascii_alnum_key(self) -> Option<i64> {
        if true { return Some(1 as i64) }
        return None
    }
}
"#;

#[test]
fn debug_some_call_resolves_in_return_context() {
    ValkyrieCompiler::default().compile_source(SOURCE).expect("Some(value) in Option<i64> return context must resolve without SMIR003");
}
