use rusty_java::MiniJavaFrontend;
use nyar_types::NyarFrontend;

#[test]
fn test_ops_conversion() {
    let source = include_str!("ops_test.java");
    let frontend = MiniJavaFrontend::default();
    
    // 1. 解析
    let ast = frontend.parse(source).expect("Failed to parse Java");
    
    // 2. 转换为 UIR
    let vfs = oak_vfs::MemoryVfs::new();
    let uir = frontend.lower(&ast, &vfs).expect("Failed to lower to UIR");
    
    // 3. 打印 UIR (调试用)
    let uir_str = format!("{:?}", uir);
    
    // 4. 验证算术运算符
    assert!(uir_str.contains("\"add\""));
    assert!(uir_str.contains("\"sub\""));
    assert!(uir_str.contains("\"mul\""));
    assert!(uir_str.contains("\"div\""));
    assert!(uir_str.contains("\"rem\""));
    
    // 5. 验证比较运算符
    assert!(uir_str.contains("\"eq\""));
    assert!(uir_str.contains("\"ne\""));
    assert!(uir_str.contains("\"lt\""));
    assert!(uir_str.contains("\"le\""));
    assert!(uir_str.contains("\"gt\""));
    assert!(uir_str.contains("\"ge\""));
    
    // 6. 验证逻辑运算符
    assert!(uir_str.contains("\"land\""));
    assert!(uir_str.contains("\"lor\""));
    assert!(uir_str.contains("\"lnot\""));
    
    // 7. 验证位运算符
    assert!(uir_str.contains("\"and\""));
    assert!(uir_str.contains("\"or\""));
    assert!(uir_str.contains("\"xor\""));
    assert!(uir_str.contains("\"not\""));
    
    // 8. 验证移位运算符
    assert!(uir_str.contains("\"shl\""));
    assert!(uir_str.contains("\"shr\""));
    assert!(uir_str.contains("\"ushr\""));
    
    // 9. 验证 instanceof
    assert!(uir_str.contains("\"instanceof\""));
    
    // 10. 验证三元运算符 (转换为 branch)
    // assert!(uir_str.contains("\"branch\""));
}
