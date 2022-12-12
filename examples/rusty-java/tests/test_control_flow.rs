use rusty_java::MiniJavaFrontend;
use nyar_types::NyarFrontend;

#[test]
fn test_control_flow_conversion() {
    let source = include_str!("control_flow.java");
    let frontend = MiniJavaFrontend::default();
    
    // 1. 解析
    let ast = frontend.parse(source).expect("Failed to parse Java");
    
    // 2. 转换为 UIR
    let vfs = oak_vfs::MemoryVfs::new();
    let uir = frontend.lower(&ast, &vfs).expect("Failed to lower to UIR");
    
    // 3. 打印 UIR (调试用)
    println!("UIR: {:?}", uir);
    
    // 4. 验证 UIR 结构
    let uir_str = format!("{:?}", uir);
    assert!(uir_str.contains("\"while\""));
    assert!(uir_str.contains("\"do_while\""));
    assert!(uir_str.contains("\"for\""));
    assert!(uir_str.contains("\"switch\""));
    assert!(uir_str.contains("\"break\""));
    assert!(uir_str.contains("\"continue\""));
}
