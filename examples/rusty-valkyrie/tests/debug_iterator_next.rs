use rusty_valkyrie::ValkyrieCompiler;

fn compile(source: &str) -> Result<(), String> {
    ValkyrieCompiler::default().compile_source(source).map(|_| ()).map_err(|e| format!("{e:?}"))
}

const OPTION_IMPL: &str = r#"
unite Option<T> { Some { value: T }, None }
imply Option<T> {
    micro unwrap(self) -> T { match self { case Some(value): value case None: while true {} } }
}
"#;

#[test]
fn unwrap_on_local_option_resolves() {
    compile(&format!(
        r#"{OPTION_IMPL}
micro step() -> i64 {{
    let x: Option<i64> = Some {{ value: 1 as i64 }}
    return x.unwrap()
}}
"#
    ))
    .expect("local unwrap");
}

#[test]
fn unwrap_on_call_result_resolves() {
    compile(&format!(
        r#"{OPTION_IMPL}
micro make() -> Option<i64> {{ return Some {{ value: 1 as i64 }} }}
micro step() -> i64 {{ return make().unwrap() }}
"#
    ))
    .expect("call result unwrap");
}

#[test]
fn iterator_next_then_unwrap_via_let_resolves() {
    compile(&format!(
        r#"{OPTION_IMPL}
namespace std.iterator;
trait Iterator {{ type Item; micro next(self) -> Option<Item>; }}
micro step<I, T>(self: I) -> T
    where I: Iterator<Item = T>
{{
    let mut iter: I = self
    let value: Option<T> = iter.next()
    return value.unwrap()
}}
"#
    ))
    .expect("let-bound next then unwrap");
}

#[test]
fn iterator_next_unwrap_chain_resolves() {
    compile(&format!(
        r#"{OPTION_IMPL}
namespace std.iterator;
trait Iterator {{ type Item; micro next(self) -> Option<Item>; }}
micro step<I, T>(self: I) -> T
    where I: Iterator<Item = T>
{{
    let mut iter: I = self
    return iter.next().unwrap()
}}
"#
    ))
    .expect("iter.next().unwrap()");
}
