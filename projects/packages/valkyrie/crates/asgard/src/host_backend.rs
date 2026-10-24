//! VOA 宿主后端抽象：Package 阶段按宿主分流。

use crate::host::HostPlatform;

/// 宿主后端：决定 dist 产物形态。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HostBackend {
    /// 浏览器 DOM + `index.html` + `boot.js`。
    BrowserDom,
    /// 微信小程序 WXML/WXSS + `setData`。
    WechatMiniProgram,
    /// Android：`classes.dex` 为唯一制品（尾段嵌入 native + UI wire）。
    AndroidCompose,
    /// iOS：`AsgardHost` Mach-O 为唯一制品。
    IosSwiftUi,
    /// Windows 桌面原生 GUI。
    WindowsNative,
    /// Linux 桌面原生 GUI。
    LinuxNative,
    /// macOS 桌面原生 GUI。
    MacOsNative,
    /// 终端 TUI（C 运行时 + ASGARDUI wire sidecar，产出原生可执行文件）。
    Terminal,
}

impl HostBackend {
    /// 从平台枚举选择后端。小游戏不走 VOA UI，应使用 `legion build` + `asgard pack --target mini-game`。
    pub fn from_platform(platform: HostPlatform) -> Result<Self, String> {
        match platform {
            HostPlatform::Browser => Ok(Self::BrowserDom),
            HostPlatform::WechatMiniProgram => Ok(Self::WechatMiniProgram),
            HostPlatform::Android => Ok(Self::AndroidCompose),
            HostPlatform::Ios => Ok(Self::IosSwiftUi),
            HostPlatform::Windows => Ok(Self::WindowsNative),
            HostPlatform::Linux => Ok(Self::LinuxNative),
            HostPlatform::MacOs => Ok(Self::MacOsNative),
            HostPlatform::Terminal => Ok(Self::Terminal),
            HostPlatform::WechatMiniGame => {
                Err("platform wechat-minigame 不走 VOA UI；请使用 `legion build` + `asgard pack --target mini-game`".into())
            }
        }
    }
}
#[cfg(test)]
#[path = "../tests/path_wired/host_backend.rs"]
mod host_backend_tests;
