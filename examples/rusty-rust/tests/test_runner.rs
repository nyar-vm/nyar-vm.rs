//! Mini Rust 测试运行器
//!
//! 独立运行核心功能测试，不依赖外部汇编器

use rusty_rust::{ast, lexer, parser};

mod test_core;

fn main() {
    println!("Mini Rust 核心功能测试运行器");
    println!("================================");

    match test_core::run_all_tests() {
        Ok(()) => {
            println!("\n✅ 所有测试通过！Mini Rust 核心功能正常工作。");
            std::process::exit(0);
        }
        Err(error) => {
            println!("\n❌ 测试失败: {}", error);
            std::process::exit(1);
        }
    }
}
