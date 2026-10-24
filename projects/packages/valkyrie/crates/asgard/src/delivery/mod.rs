//! VOA 交付组装：`asgard pack`。

mod apk;
mod ipa;
mod minigame;
mod miniprogram;
mod util;

use std::path::PathBuf;

use miette::Result;

pub use apk::pack_apk;
pub use ipa::pack_ipa;
pub use minigame::pack_mini_game;
pub use miniprogram::pack_mini_program;

/// 交付目标。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PackTarget {
    /// Android APK。
    Apk,
    /// iOS IPA。
    Ipa,
    /// 微信小程序。
    MiniProgram,
    /// 微信小游戏。
    MiniGame,
}

impl PackTarget {
    /// 从 CLI 字符串解析。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "apk" => Some(Self::Apk),
            "ipa" => Some(Self::Ipa),
            "mini-program" | "miniprogram" | "mini_program" => Some(Self::MiniProgram),
            "mini-game" | "minigame" | "mini_game" => Some(Self::MiniGame),
            _ => None,
        }
    }
}

/// Pack 选项。
#[derive(Debug, Clone)]
pub struct PackOptions {
    /// 项目目录或 dist。
    pub input: PathBuf,
    /// 输出目录。
    pub output: Option<PathBuf>,
    /// 交付目标。
    pub target: PackTarget,
    /// 小程序/小游戏项目名。
    pub project_name: Option<String>,
    /// 小游戏 WASM 文件名。
    pub wasm_name: Option<String>,
}

/// Pack 报告。
#[derive(Debug, Clone)]
pub struct PackReport {
    /// 主交付物路径。
    pub artifact_path: PathBuf,
    /// 说明信息。
    pub message: String,
}

/// 按目标组装交付物。
pub fn pack_voa_delivery(options: &PackOptions) -> Result<PackReport> {
    match options.target {
        PackTarget::Apk => pack_apk(&options.input, options.output.as_deref()),
        PackTarget::Ipa => pack_ipa(&options.input, options.output.as_deref()),
        PackTarget::MiniProgram => {
            let name = options.project_name.as_deref().unwrap_or("asgard-miniprogram");
            pack_mini_program(&options.input, options.output.as_deref(), name)
        }
        PackTarget::MiniGame => pack_mini_game(&options.input, options.output.as_deref(), options.wasm_name.as_deref()),
    }
}
