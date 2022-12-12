use rusty_rust::*;

#[test]
fn test_complex_compilation() {
    let source = r#"
struct Point {
    x: i32,
    y: i32,
}

struct Line {
    start: Point,
    end: Point,
}

fn main() {
    let p1 = Point { x: 1, y: 2 };
    let p2 = Point { x: 10, y: 20 };
    let line = Line { start: p1, end: p2 };
    
    let arr = [1, 2, 3, 4, 5];
    let len = arr.len();
    
    let sum = line.start.x + line.end.y + arr[0] + len;
    println(sum);
}
"#;

    match MiniRustParser::parse(source) {
        Ok(program) => {
            println!("解析成功！");

            // 验证生成的指令
            let main_fn = program
                .functions
                .iter()
                .find(|f| f.name == "main")
                .expect("找不到 main 函数");

            // 打印所有指令用于调试
            for (i, inst) in main_fn.instructions.iter().enumerate() {
                println!("  {}: {:?}", i, inst);
            }
        }
        Err(e) => {
            panic!("解析失败: {:?}", e);
        }
    }
}
