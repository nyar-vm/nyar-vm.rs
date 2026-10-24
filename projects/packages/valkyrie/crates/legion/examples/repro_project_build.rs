//! 通过正式宿主 API 复现完整工程构建；不截断源码闭包、不解释错误文本、不删除输出目录。
//!
//! 用法：cargo run -p legion --example repro_project_build -- <project> <target> <output>。
use std::{path::PathBuf, process::ExitCode};

fn main() -> ExitCode {
    let mut arguments = std::env::args().skip(1);
    let Some(project) = arguments.next().map(PathBuf::from)
    else {
        eprintln!("需要参数：<project> <target> <output>");
        return ExitCode::FAILURE;
    };
    let Some(target) = arguments.next()
    else {
        eprintln!("缺少 target");
        return ExitCode::FAILURE;
    };
    let Some(output) = arguments.next().map(PathBuf::from)
    else {
        eprintln!("缺少 output");
        return ExitCode::FAILURE;
    };
    if arguments.next().is_some() {
        eprintln!("参数过多");
        return ExitCode::FAILURE;
    }
    let target = match legion::CanonicalTarget::parse(&target) {
        Ok(target) => target,
        Err(error) => {
            eprintln!("目标错误：{error:?}");
            return ExitCode::FAILURE;
        }
    };
    match legion::api::build_project(&project, target, Some(output)) {
        Ok(summary) => {
            println!("构建成功：{} 个产物", summary.artifacts.len());
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("构建失败：{error:?}");
            ExitCode::FAILURE
        }
    }
}
