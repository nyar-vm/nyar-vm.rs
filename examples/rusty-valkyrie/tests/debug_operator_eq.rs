use rusty_valkyrie::ValkyrieCompiler;

#[test]
fn generic_eq_operator_resolves() {
    let source = r#"
trait Iterator { type Item; micro has_next(self) -> bool; micro next(self) -> Option<Item>; }
[primitive("core::primitive::i32")]
structure i32 { }
imply i32 { infix `==`(self, rhs: Self): bool { return true } }
unite Option<T> { Some { value: T }, None }
imply Option<T> {
    micro unwrap(self, fallback: T) -> T { match self { case Some(value): value case None: fallback } }
}
micro contains<I>(self: I, value: i32): bool
    where I: Iterator<Item = i32>
{
    let mut iter: I = self
    while iter.has_next() {
        if iter.next().unwrap(value) == value {
            return true
        }
    }
    return false
}
"#;
    ValkyrieCompiler::default().compile_source(source).expect("generic == in contains must resolve");
}
