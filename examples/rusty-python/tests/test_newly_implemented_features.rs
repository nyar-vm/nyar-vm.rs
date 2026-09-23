use rusty_python::RustyPythonFrontend;
use nyar_types::NyarFrontend;

fn verify_lowering(source: &str) {
    let frontend = RustyPythonFrontend::new();
    let ast = frontend.parse(source).expect("Failed to parse source");
    
    let mut egraph = chomsky_uir::EGraph::new();
    let vfs = oak_vfs::MemoryVfs::new();
    let mut ctx = nyar_types::NyarContext::new(&mut egraph, &vfs, 1);
    
    let root_id = frontend.lower_unified(&ast, &mut ctx);
    
    assert!(root_id != chomsky_uir::Id::from(0usize));
}

#[test]
fn test_decorators_lowering() {
    let source = r#"
↯deco1
↯deco2(arg)
def func(x):
    pass

↯class_deco
class MyClass:
    pass
"#;
    verify_lowering(source);
    println!("Successfully lowered AST with decorators");
}

#[test]
fn test_async_features_lowering() {
    let source = r#"
async def fetch_data():
    data = await get_data()
    async for item in data_stream:
        process(item)
    
    async with lock:
        save(data)
"#;
    verify_lowering(source);
    println!("Successfully lowered AST with async features");
}

#[test]
fn test_match_case_lowering() {
    let source = r#"
match command.split():
    case ["quit"]:
        quit()
    case ["load", filename]:
        load(filename)
    case ["move", x, y] if x > 0:
        move(x, y)
    case _:
        print("unknown")
"#;
    verify_lowering(source);
    println!("Successfully lowered AST with match-case");
}

#[test]
fn test_comprehensions_and_generators_lowering() {
    let source = r#"
squares = [x*x for x in range(10) if x % 2 == 0]
set_comp = {x for x in items}
dict_comp = {k: v for k, v in pairs}
gen_exp = (x*2 for x in data)

def my_gen():
    yield 1
    yield from other_gen()
"#;
    verify_lowering(source);
    println!("Successfully lowered AST with comprehensions and generators");
}

#[test]
fn test_other_expressions_lowering() {
    let source = r#"
a = x if cond else y
f = lambda x, y: x + y
*rest, last = items
call(1, 2, keyword=3, **kwargs)
"#;
    verify_lowering(source);
    println!("Successfully lowered AST with miscellaneous expressions");
}

#[test]
fn test_compare_and_orelse_lowering() {
    let source = r#"
# Multiple comparisons
if 1 < x <= 10 > y:
    pass

# While with else
while condition:
    do_something()
else:
    do_else()

# For with else
for i in range(5):
    if i == 3:
        break
else:
    print("Loop finished normally")

# Async for with else
async def test():
    async for item in stream:
        process(item)
    else:
        finish()
"#;
    verify_lowering(source);
    println!("Successfully lowered AST with multiple comparisons and orelse clauses");
}
