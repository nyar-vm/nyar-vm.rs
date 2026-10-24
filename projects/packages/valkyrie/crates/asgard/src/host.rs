//! VOA 宿主平台分流。

/// 交付宿主平台（Package 阶段分流）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HostPlatform {
    /// 浏览器 DOM + boot.js。
    #[default]
    Browser,
    /// 微信小游戏（WASM + wx API，非 VOA DOM）。
    WechatMiniGame,
    /// 微信小程序（WXML/WXSS + setData）。
    WechatMiniProgram,
    /// Android（原生字节码交付，无 WASM；`asgard pack --target apk`）。
    Android,
    /// iOS（原生字节码交付，无 WASM；`asgard pack --target ipa`）。
    Ios,
    /// Windows 桌面原生 GUI（WinUI + asgard.ui.windows UiHost）。
    Windows,
    /// Linux 桌面原生 GUI（ELF + asgard.ui.linux UiHost）。
    Linux,
    /// macOS 桌面原生 GUI（SwiftUI + asgard.ui.macos UiHost）。
    MacOs,
    /// 终端 TUI（ASCII 盒模型 + 事件循环，asgard.ui.terminal UiHost）。
    Terminal,
}

impl HostPlatform {
    /// 从 `asgard.config.v` 的 `platform` 字段解析。
    pub fn parse(value: &str) -> Self {
        match value.trim().to_ascii_lowercase().as_str() {
            "wechat-miniprogram" | "wechat_miniprogram" | "miniprogram" | "mini-program" => Self::WechatMiniProgram,
            "wechat-minigame" | "wechat_minigame" | "minigame" | "mini-game" => Self::WechatMiniGame,
            "android" | "apk" => Self::Android,
            "ios" | "ipa" => Self::Ios,
            "windows" | "win32" | "win" => Self::Windows,
            "linux" => Self::Linux,
            "macos" | "mac" | "darwin" => Self::MacOs,
            "terminal" | "tui" => Self::Terminal,
            _ => Self::Browser,
        }
    }

    /// 稳定字符串标识。
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::WechatMiniGame => "wechat-minigame",
            Self::WechatMiniProgram => "wechat-miniprogram",
            Self::Android => "android",
            Self::Ios => "ios",
            Self::Windows => "windows",
            Self::Linux => "linux",
            Self::MacOs => "macos",
            Self::Terminal => "terminal",
        }
    }
}
#[cfg(test)]
#[path = "../tests/path_wired/host.rs"]
mod host_tests;
