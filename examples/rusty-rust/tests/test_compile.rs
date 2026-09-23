use rusty_rust::*;

fn main() {
    let source = r#"
fn main() {
    let x = 42;
    let y = x + 10;
    return y;
}
"#;

    match MiniRustParser::parse(source) {
        Ok(program) => {
            println!("解析成功！");
            println!("程序名称: {}", program.name);
            println!("函数数量: {}", program.functions.len());

            for function in &program.functions {
                println!("函数: {}", function.name);
                println!("指令数量: {}", function.instructions.len());
            }
        }
        Err(e) => {
            println!("解析失败: {:?}", e);
        }
    }
}
