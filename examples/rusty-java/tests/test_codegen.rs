use rusty_java::MiniJavaFrontend;
use nyar_types::NyarFrontend;

#[test]
fn test_hello_world_conversion() {
    let source = include_str!("hello.java");
    let frontend = MiniJavaFrontend::default();
    
    // 1. 解析
    let ast = frontend.parse(source).expect("Failed to parse Java");
    
    // 2. 转换为 UIR
    let vfs = oak_vfs::MemoryVfs::new();
    let uir = frontend.lower(&ast, &vfs).expect("Failed to lower to UIR");
    
    // 3. 打印 UIR (调试用)
    println!("UIR: {:?}", uir);
    
    // 4. 验证 UIR 结构
    // 期望结构是一个 Seq，包含一个 class 扩展，class 包含 main 方法，main 方法包含 cross_lang_call
}
