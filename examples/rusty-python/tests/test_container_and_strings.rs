use rusty_python::RustyPythonFrontend;
use nyar_types::NyarFrontend;
use oak_vfs::MemoryVfs;

#[test]
fn test_containers_and_comprehensions() {
    let frontend = RustyPythonFrontend::default();
    let source = r#"
# Dict and Set literals
d = {"a": 1, "b": 2, **{"c": 3}}
s = {1, 2, 3, *{4, 5}}

# Comprehensions
l_comp = [x * 2 for x in range(10) if x > 5]
s_comp = {x for x in range(5)}
d_comp = {k: v for k, v in [("a", 1), ("b", 2)]}
gen_exp = (x for x in range(3))
"#;
    let ast = frontend.parse(source).expect("Failed to parse container expressions");
    let vfs = MemoryVfs::new();
    let tree = frontend.lower(&ast, &vfs).expect("Failed to lower AST to UIR");
    
    let tree_str = format!("{:?}", tree);
    println!("IKunTree: {}", tree_str);
    assert!(tree_str.contains("dict"));
    assert!(tree_str.contains("dict_item"));
    assert!(tree_str.contains("dict_unpack"));
    assert!(tree_str.contains("set"));
    assert!(tree_str.contains("list_comp"));
    assert!(tree_str.contains("set_comp"));
    assert!(tree_str.contains("dict_comp"));
    assert!(tree_str.contains("generator_exp"));
    assert!(tree_str.contains("comprehension"));
}

#[test]
fn test_strings_and_slices() {
    let frontend = RustyPythonFrontend::default();
    let source = r#"
# F-strings
name = "World"
f_str = f"Hello, {name}!"

# Bytes
b_str = b"hello"

# Subscript and Slices
item = my_list[0]
slice_obj = my_list[1:5:2]
"#;
    let ast = frontend.parse(source).expect("Failed to parse string and slice expressions");
    let vfs = MemoryVfs::new();
    let tree = frontend.lower(&ast, &vfs).expect("Failed to lower AST to UIR");
    
    let tree_str = format!("{:?}", tree);
    assert!(tree_str.contains("fstring"));
    assert!(tree_str.contains("formatted_value"));
    assert!(tree_str.contains("bytes"));
    assert!(tree_str.contains("subscript"));
    assert!(tree_str.contains("slice"));
}
