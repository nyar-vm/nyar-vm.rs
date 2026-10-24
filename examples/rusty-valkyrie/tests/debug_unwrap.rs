use rusty_valkyrie::ValkyrieCompiler;

#[test]
fn unwrap_on_concrete_option() {
    ValkyrieCompiler::default()
        .compile_source(
            r#"
unite Option<T> { Some { value: T }, None }
imply Option<T> {
    micro unwrap(self) -> T { match self { case Some(value): value case None: return None } }
}
micro test(x: Option<i64>) -> i64 { return x.unwrap() }
"#,
        )
        .expect("concrete option unwrap");
}

#[test]
fn unwrap_on_generic_option_param() {
    ValkyrieCompiler::default()
        .compile_source(
            r#"
unite Option<T> { Some { value: T }, None }
imply Option<T> {
    micro unwrap(self) -> T { match self { case Some(value): value case None: return None } }
}
micro test<T>(x: Option<T>) -> T { return x.unwrap() }
"#,
        )
        .expect("generic option unwrap");
}

#[test]
fn unwrap_on_chained_next() {
    ValkyrieCompiler::default()
        .compile_source(
            r#"
unite Option<T> { Some { value: T }, None }
imply Option<T> {
    micro unwrap(self) -> T { match self { case Some(value): value case None: return None } }
}
trait Iterator { type Item; micro next(self) -> Option<Item>; }
micro step<I, T>(self: I) -> T
    where I: Iterator<Item = T>
{
    let mut iter: I = self
    return iter.next().unwrap()
}
"#,
        )
        .expect("iter.next().unwrap()");
}
