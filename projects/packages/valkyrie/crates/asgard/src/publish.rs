//! `asgard publish` — 统一发布入口（对接各商店 CLI 占位）。

use std::path::Path;

use miette::Result;

use crate::delivery::{PackOptions, PackTarget, pack_voa_delivery};

/// 发布目标。
#[derive(Debug, Clone, Copy)]
pub enum PublishTarget {
    /// Google Play（需 `bundletool` / `apksigner`）。
    Apk,
    /// App Store Connect（需 `xcrun altool`）。
    Ipa,
    /// 微信小程序开发者工具上传。
    MiniProgram,
    /// 静态 Web CDN。
    Web,
}

impl PublishTarget {
    /// 从 CLI 字符串解析。
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "apk" | "play" => Some(Self::Apk),
            "ipa" | "appstore" => Some(Self::Ipa),
            "mini-program" | "wechat" => Some(Self::MiniProgram),
            "web" => Some(Self::Web),
            _ => None,
        }
    }
}

/// 发布报告。
#[derive(Debug, Clone)]
pub struct PublishReport {
    /// 制品路径。
    pub artifact_path: std::path::PathBuf,
    /// 说明。
    pub message: String,
}

/// 执行发布流程：先 `pack` 再调用平台 CLI（若可用）。
pub fn publish_voa(input: &Path, target: PublishTarget, output: Option<&Path>) -> Result<PublishReport> {
    let pack_target = match target {
        PublishTarget::Apk => PackTarget::Apk,
        PublishTarget::Ipa => PackTarget::Ipa,
        PublishTarget::MiniProgram => PackTarget::MiniProgram,
        PublishTarget::Web => {
            return Ok(PublishReport {
                artifact_path: input.to_path_buf(),
                message: "Web 发布：将 dist/ 同步至 CDN（见 documentation/platforms/web-cdn.md）".into(),
            });
        }
    };
    let pack = pack_voa_delivery(&PackOptions {
        input: input.to_path_buf(),
        output: output.map(std::path::PathBuf::from),
        target: pack_target,
        project_name: None,
        wasm_name: None,
    })?;
    let hint = match target {
        PublishTarget::Apk => "下一步: apksigner sign --ks release.jks",
        PublishTarget::Ipa => "下一步: xcrun altool --upload-app",
        PublishTarget::MiniProgram => "下一步: 微信开发者工具上传",
        PublishTarget::Web => "",
    };
    Ok(PublishReport { artifact_path: pack.artifact_path, message: format!("{}\n{hint}", pack.message) })
}
