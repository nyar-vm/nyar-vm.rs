use virtual_python::lexer::PythonLexer;

fn main() {
    // 测试简单赋值
    println!("=== Testing assignment: x = 42 ===");
    let mut lexer = PythonLexer::new("x = 42");
    let result = lexer.tokenize();
    if let Ok(token_stream) = result.result {
        for token in token_stream.tokens.get_ref() {
            println!("{:?}", token);
        }
    } else {
        println!("Lexer failed: {:?}", result.diagnostics);
    }

    println!("\n=== Testing function: def hello(): ===");
    let mut lexer = PythonLexer::new("def hello():");
    let result = lexer.tokenize();
    if let Ok(token_stream) = result.result {
        for token in token_stream.tokens.get_ref() {
            println!("{:?}", token);
        }
    } else {
        println!("Lexer failed: {:?}", result.diagnostics);
    }

    println!("\n=== Testing multiline function ===");
    let code = r#"
def hello():
    print("Hello, World!")
"#;
    let mut lexer = PythonLexer::new(code);
    let result = lexer.tokenize();
    if let Ok(token_stream) = result.result {
        for token in token_stream.tokens.get_ref() {
            println!("{:?}", token);
        }
    } else {
        println!("Lexer failed: {:?}", result.diagnostics);
    }
}