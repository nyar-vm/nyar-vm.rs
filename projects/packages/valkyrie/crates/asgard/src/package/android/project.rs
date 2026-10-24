//! Android 二进制交付：`classes.dex` 为唯一逻辑制品（native + UI wire 均编入尾段）。

use crate::{
    awsl::LoweredComponent,
    codegen::{embed_asgard_ui_section, embed_host_native_section, encode_mobile_ui_package, resolve_android_compose_dex},
    package::artifact::PackageArtifact,
};

/// Android 打包产物。
#[derive(Debug, Clone)]
pub struct AndroidPackageOutput {
    /// applicationId。
    pub application_id: String,
    /// 相对路径 → 产物。
    pub files: Vec<(String, PackageArtifact)>,
}

/// 生成 Android dist：`classes.dex` 内含 JVM 壳 + 尾段嵌入（native AOT + Asgard UI）。
pub fn package_android_project(components: &[LoweredComponent], module_name: &str, native_logic: &[u8]) -> AndroidPackageOutput {
    let application_id = bundle_id(module_name);
    let ui = encode_mobile_ui_package(components);

    let mut dex = resolve_android_compose_dex().expect("Android Compose dex 构建失败");
    embed_host_native_section(&mut dex, native_logic);
    embed_asgard_ui_section(&mut dex, &ui);

    let files = vec![
        ("AndroidManifest.xml".into(), PackageArtifact::Text(android_manifest(&application_id, module_name))),
        ("classes.dex".into(), PackageArtifact::Bytes(dex)),
    ];
    AndroidPackageOutput { application_id, files }
}

fn bundle_id(module_name: &str) -> String {
    let mut parts = Vec::new();
    for part in module_name.split(|c: char| !c.is_ascii_alphanumeric()) {
        if part.is_empty() {
            continue;
        }
        let mut s = part.to_ascii_lowercase();
        if s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
            s = format!("n{s}");
        }
        parts.push(s);
    }
    if parts.is_empty() { "com.asgard.app".into() } else { format!("com.asgard.{}", parts.join("_")) }
}

fn android_manifest(application_id: &str, label: &str) -> String {
    format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android"
    package="{application_id}">
    <uses-sdk android:minSdkVersion="24" android:targetSdkVersion="34" />
    <application
        android:allowBackup="true"
        android:label="{label}"
        android:theme="@android:style/Theme.Material.Light.NoActionBar"
        android:supportsRtl="true">
        <activity
            android:name="com.asgard.runtime.MainActivity"
            android:exported="true">
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
"#
    )
}
