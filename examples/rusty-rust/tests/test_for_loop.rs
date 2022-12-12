use rusty_rust::*;

#[test]
fn test_for_loop_compilation() {
    let source = r#"
fn main() {
    let mut sum = 0;
    for i in 0..10 {
        sum = sum + i;
    }
    println(sum);
}
"#;

    match MiniRustParser::parse(source) {
        Ok(program) => {
            println!("解析成功！");

            let main_fn = program
                .functions
                .iter()
                .find(|f| f.name == "main")
                .expect("找不到 main 函数");

            // 打印所有指令用于调试
            for (i, inst) in main_fn.instructions.iter().enumerate() {
                println!("  {}: {:?}", i, inst);
            }

            // 验证是否生成了跳转和比较指令
            let has_jump = main_fn
                .instructions
                .iter()
                .any(|inst| matches!(inst, gaia_assembler::instruction::GaiaInstruction::Jump(_)));
            let has_compare = main_fn
                .instructions
                .iter()
                .any(|inst| matches!(inst, gaia_assembler::instruction::GaiaInstruction::LessThan));

            assert!(has_jump, "应该包含跳转指令");
            assert!(has_compare, "应该包含比较指令");
        }
        Err(e) => {
            panic!("解析失败: {:?}", e);
        }
    }
}
