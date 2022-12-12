use virtual_python::lexer::PythonLexer;

#[test]
pub fn test_lexer_functionality() {
    println!("开始测试词法分析器...");

    // 测试简单的赋值语句
    let input = "x = 42";
    let mut lexer = PythonLexer::new(input);
    let diagnostics = lexer.tokenize();

    match diagnostics.result {
        Ok(token_stream) => {
            println!("✓ 词法分析成功");
            println!("  输入: {}", input);
            let tokens_vec = token_stream.tokens.get_ref();
            println!("  生成的 token 数量: {}", tokens_vec.len());
            assert!(tokens_vec.len() >= 3);

            // 打印前几个 token
            for (i, token) in tokens_vec.iter().take(5).enumerate() {
                println!("  Token {}: {:?}", i, token.token_type);
            }
        }
        Err(e) => {
            println!("✗ 词法分析失败: {:?}", e);
            panic!("词法分析失败");
        }
    }

    // 测试更复杂的代码
    let complex_input = r#"
        def add(a: int, b: int):
            return a + b
    "#;

    let mut lexer2 = PythonLexer::new(complex_input);
    let diagnostics2 = lexer2.tokenize();

    match diagnostics2.result {
        Ok(token_stream) => {
            println!("✓ 复杂代码词法分析成功");
            let tokens_vec = token_stream.tokens.get_ref();
            println!("  生成的 token 数量: {}", tokens_vec.len());
            assert!(!tokens_vec.is_empty());
        }
        Err(e) => {
            println!("✗ 复杂代码词法分析失败: {:?}", e);
            panic!("复杂代码词法分析失败");
        }
    }

    println!("词法分析器测试完成");
}
