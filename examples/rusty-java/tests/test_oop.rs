use rusty_java::MiniJavaFrontend;
use nyar_types::NyarFrontend;

#[test]
fn test_oop_conversion() {
    let source = include_str!("oop_test.java");
    let frontend = MiniJavaFrontend::default();
    
    // 1. 解析
    let ast = frontend.parse(source).expect("Failed to parse Java");
    
    // 2. 转换为 UIR
    let vfs = oak_vfs::MemoryVfs::new();
    let uir = frontend.lower(&ast, &vfs).expect("Failed to lower to UIR");
    
    // 3. 打印 UIR (调试用)
    println!("UIR: {:?}", uir);
    
    // 4. 验证 UIR 结构
    // 检查是否包含 interface, class, constructor, new, this, super 等
}
