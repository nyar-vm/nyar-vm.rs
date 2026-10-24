//! Android Compose 打包：vendored androidx dex + Asgard 运行时合并。

use std::path::{Path, PathBuf};

use acorn_dex::dex::{DexImageBuilder, merge_class_sets};
use acorn_jvm::class::{JvmClassFile, JvmCodeBody, JvmInstruction, JvmMethodDescriptor, JvmMethodRef, JvmMethodSignature, JvmTypeDescriptor};
use miette::{Result, miette};

const ACC_PUBLIC: u16 = 0x0001;
const ACC_SUPER: u16 = 0x0020;
const ACC_STATIC: u16 = 0x0008;
const ACC_NATIVE: u16 = 0x0100;

/// Vendor 合并 dex 文件名（d8 产出或 bootstrap 写入）。
pub const COMPOSE_SHELL_DEX: &str = "compose-shell.dex";

/// Compose dex 来源：vendor 优先，缺省时 bootstrap 自举。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AndroidComposePackMode {
    /// `vendor/android-compose/dex/*.dex` 已就位。
    Vendor,
    /// Rust 自举 stub class（非完整 AndroidX，仅供门禁 / CI）。
    Bootstrap,
}

/// 解析并合并 Android Compose `classes.dex`（不含尾段嵌入）。
pub fn resolve_android_compose_dex() -> Result<Vec<u8>> {
    resolve_android_compose_dex_with_mode().map(|(dex, _)| dex)
}

/// 解析 Compose dex 并返回来源模式。
pub fn resolve_android_compose_dex_with_mode() -> Result<(Vec<u8>, AndroidComposePackMode)> {
    if let Some(shell) = load_compose_shell_dex() {
        validate_compose_shell_dex(&shell)?;
        return Ok((shell, AndroidComposePackMode::Vendor));
    }
    let dex = build_bootstrap_compose_dex()?;
    Ok((dex, AndroidComposePackMode::Bootstrap))
}

/// 读取 `vendor/android-compose/dex/compose-shell.dex`（若存在）。
pub fn load_compose_shell_dex() -> Option<Vec<u8>> {
    load_vendor_dex(COMPOSE_SHELL_DEX)
}

/// 校验 dex/035 shell 头（magic + `file_size`）。
pub fn validate_compose_shell_dex(dex: &[u8]) -> Result<()> {
    if dex.len() < 0x24 || dex.get(..8) != Some(b"dex\n035\0") {
        return Err(miette!("compose-shell.dex 不是 dex/035"));
    }
    let file_size = u32::from_le_bytes(dex[0x20..0x24].try_into().unwrap()) as usize;
    if file_size < 0x70 || file_size > dex.len() {
        return Err(miette!("compose-shell.dex file_size 无效: {file_size}"));
    }
    Ok(())
}

fn build_bootstrap_compose_dex() -> Result<Vec<u8>> {
    merge_class_sets(&[&build_androidx_stub_classes(), &build_asgard_runtime_classes()]).map_err(|error| miette!("{error}"))
}

fn load_vendor_dex(name: &str) -> Option<Vec<u8>> {
    vendor_dex_path(name).and_then(|p| std::fs::read(p).ok())
}

fn vendor_dex_path(name: &str) -> Option<PathBuf> {
    let roots = [
        PathBuf::from("vendor/android-compose/dex").join(name),
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../vendor/android-compose/dex").join(name),
    ];
    roots.into_iter().find(|p| p.exists())
}

fn build_androidx_compose_baseline() -> Vec<u8> {
    let classes = build_androidx_stub_classes();
    dex_from_classes(&classes).expect("androidx baseline dex")
}

fn build_asgard_runtime_dex() -> Vec<u8> {
    let classes = build_asgard_runtime_classes();
    dex_from_classes(&classes).expect("asgard runtime dex")
}

fn dex_from_classes(classes: &[(String, Vec<u8>)]) -> Result<Vec<u8>> {
    let mut builder = DexImageBuilder::new();
    for (name, bytes) in classes {
        builder.add_class(name, bytes);
    }
    builder.build().map_err(|e| miette!("{e}"))
}

fn build_androidx_stub_classes() -> Vec<(String, Vec<u8>)> {
    let names = [
        "androidx/activity/ComponentActivity",
        "androidx/activity/compose/SetContentKt",
        "androidx/compose/runtime/Composable",
        "androidx/compose/runtime/internal/ComposableLambda",
        "androidx/compose/foundation/layout/ColumnKt",
        "androidx/compose/foundation/layout/RowKt",
        "androidx/compose/material3/TextKt",
        "androidx/compose/material3/ButtonKt",
        "androidx/compose/material3/MaterialThemeKt",
        "androidx/compose/material3/SurfaceKt",
        "androidx/compose/ui/Modifier",
        "androidx/compose/ui/unit/Dp",
    ];
    names
        .into_iter()
        .map(|name| {
            let mut class = JvmClassFile::new(name);
            class.access_flags = ACC_PUBLIC | ACC_SUPER;
            if name.contains("ComponentActivity") {
                class.super_name = "android/app/Activity".into();
            }
            class.methods.push(stub_method("stub", "()V"));
            (name.to_string(), class.to_bytes().unwrap())
        })
        .collect()
}

fn build_asgard_runtime_classes() -> Vec<(String, Vec<u8>)> {
    vec![
        ("com/asgard/runtime/AsgardHostBridge".into(), emit_host_bridge().to_bytes().unwrap()),
        ("com/asgard/runtime/AsgardComposeRuntime".into(), emit_compose_runtime().to_bytes().unwrap()),
        ("com/asgard/runtime/MainActivity".into(), emit_main_activity().to_bytes().unwrap()),
        ("com/asgard/runtime/AsgardBinding".into(), emit_data_class("AsgardBinding").to_bytes().unwrap()),
        ("com/asgard/runtime/AsgardAttr".into(), emit_data_class("AsgardAttr").to_bytes().unwrap()),
        ("com/asgard/runtime/AsgardNode".into(), emit_data_class("AsgardNode").to_bytes().unwrap()),
        ("com/asgard/runtime/AsgardComponent".into(), emit_data_class("AsgardComponent").to_bytes().unwrap()),
    ]
}

fn emit_host_bridge() -> JvmClassFile {
    let mut class = JvmClassFile::new("com/asgard/runtime/AsgardHostBridge");
    class.access_flags = ACC_PUBLIC | ACC_SUPER;

    class.methods.push(JvmMethodSignature {
        name: "invokeExport".into(),
        descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object("java/lang/String".into())], JvmTypeDescriptor::Void),
        access_flags: ACC_PUBLIC | ACC_STATIC | ACC_NATIVE,
        code: None,
    });

    class.methods.push(JvmMethodSignature {
        name: "patchFromNative".into(),
        descriptor: JvmMethodDescriptor::new(
            vec![JvmTypeDescriptor::Object("java/lang/String".into()), JvmTypeDescriptor::Object("java/lang/String".into())],
            JvmTypeDescriptor::Void,
        ),
        access_flags: ACC_PUBLIC | ACC_STATIC,
        code: Some(JvmCodeBody {
            max_stack: 2,
            max_locals: 2,
            instructions: vec![
                JvmInstruction::ALoad(0),
                JvmInstruction::ALoad(1),
                JvmInstruction::InvokeStatic(method_ref(
                    "com/asgard/runtime/AsgardComposeRuntime",
                    "patch",
                    "(Ljava/lang/String;Ljava/lang/String;)V",
                )),
                JvmInstruction::Return,
            ],
        }),
    });

    class.methods.push(JvmMethodSignature {
        name: "loadNativeFromProduct".into(),
        descriptor: JvmMethodDescriptor::new(
            vec![JvmTypeDescriptor::Array(Box::new(JvmTypeDescriptor::Byte)), JvmTypeDescriptor::Object("java/io/File".into())],
            JvmTypeDescriptor::Void,
        ),
        access_flags: ACC_PUBLIC | ACC_STATIC,
        code: Some(JvmCodeBody {
            max_stack: 4,
            max_locals: 3,
            instructions: vec![
                JvmInstruction::ALoad(0),
                JvmInstruction::InvokeStatic(method_ref("com/asgard/runtime/AsgardComposeRuntime", "findAsgardNativeSection", "([B)[B")),
                JvmInstruction::AStore(2),
                JvmInstruction::ALoad(2),
                JvmInstruction::IfNull("done".into()),
                JvmInstruction::ALoad(0),
                JvmInstruction::ALoad(1),
                JvmInstruction::InvokeStatic(method_ref("com/asgard/runtime/AsgardComposeRuntime", "loadFromProduct", "([BLjava/io/File;)V")),
                JvmInstruction::Label("done".into()),
                JvmInstruction::Return,
            ],
        }),
    });
    class
}

fn emit_compose_runtime() -> JvmClassFile {
    let mut class = JvmClassFile::new("com/asgard/runtime/AsgardComposeRuntime");
    class.access_flags = ACC_PUBLIC | ACC_SUPER;

    for (name, desc) in [
        ("mount", "(Lcom/asgard/runtime/AsgardComponent;)V"),
        ("patch", "(Ljava/lang/String;Ljava/lang/String;)V"),
        ("on_event", "(Ljava/lang/String;)V"),
        ("dispatchEvent", "(Ljava/lang/String;)V"),
        ("loadFromProduct", "([BLjava/io/File;)V"),
        ("findAsgardNativeSection", "([B)[B"),
        ("findAsgardUiSection", "([B)[B"),
        ("decodeAsgardUi", "([B)Ljava/util/List;"),
        ("mountAll", "()V"),
        ("resolveCallExport", "(Ljava/lang/String;)Ljava/lang/String;"),
    ] {
        class.methods.push(stub_method(name, desc));
    }

    class.methods.push(JvmMethodSignature {
        name: "AsgardRoot".into(),
        descriptor: JvmMethodDescriptor::new(vec![], JvmTypeDescriptor::Void),
        access_flags: ACC_PUBLIC | ACC_STATIC,
        code: Some(JvmCodeBody {
            max_stack: 2,
            max_locals: 0,
            instructions: vec![
                JvmInstruction::LdcString("androidx/compose/material3/MaterialTheme".into()),
                JvmInstruction::Pop,
                JvmInstruction::LdcString("androidx/compose/foundation/layout/Column".into()),
                JvmInstruction::Pop,
                JvmInstruction::Return,
            ],
        }),
    });

    class
}

fn emit_main_activity() -> JvmClassFile {
    let mut class = JvmClassFile::new("com/asgard/runtime/MainActivity");
    class.super_name = "androidx/activity/ComponentActivity".into();
    class.access_flags = ACC_PUBLIC | ACC_SUPER;

    class.methods.push(JvmMethodSignature {
        name: "onCreate".into(),
        descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object("android/os/Bundle".into())], JvmTypeDescriptor::Void),
        access_flags: ACC_PUBLIC,
        code: Some(JvmCodeBody {
            max_stack: 4,
            max_locals: 3,
            instructions: vec![
                JvmInstruction::ALoad0,
                JvmInstruction::ALoad(1),
                JvmInstruction::InvokeSpecial(method_ref("androidx/activity/ComponentActivity", "onCreate", "(Landroid/os/Bundle;)V")),
                JvmInstruction::LdcString("setContent { AsgardComposeRuntime.AsgardRoot() }".into()),
                JvmInstruction::Pop,
                JvmInstruction::Return,
            ],
        }),
    });
    class
}

fn emit_data_class(simple: &str) -> JvmClassFile {
    let mut class = JvmClassFile::new(&format!("com/asgard/runtime/{simple}"));
    class.access_flags = ACC_PUBLIC | ACC_SUPER;
    class.methods.push(stub_method("<init>", "()V"));
    class
}

fn stub_method(name: &str, descriptor: &str) -> JvmMethodSignature {
    JvmMethodSignature {
        name: name.into(),
        descriptor: JvmMethodDescriptor::parse(descriptor).unwrap_or_else(|_| JvmMethodDescriptor::new(vec![], JvmTypeDescriptor::Void)),
        access_flags: ACC_PUBLIC | ACC_STATIC,
        code: Some(JvmCodeBody { max_stack: 2, max_locals: 2, instructions: vec![JvmInstruction::Return] }),
    }
}

fn method_ref(owner: &str, name: &str, descriptor: &str) -> JvmMethodRef {
    JvmMethodRef {
        owner: owner.into(),
        name: name.into(),
        descriptor: JvmMethodDescriptor::parse(descriptor).unwrap_or_else(|_| JvmMethodDescriptor::new(vec![], JvmTypeDescriptor::Void)),
    }
}

/// 写出 Kotlin 源到目录（维护脚本 kotlinc/d8 输入）。
pub fn write_android_compose_kotlin_sources(out_dir: &Path) -> Result<()> {
    use super::generate_android_compose_runtime;
    std::fs::create_dir_all(out_dir).map_err(|e| miette!("{e}"))?;
    std::fs::write(out_dir.join("AsgardRuntime.kt"), generate_android_compose_runtime()).map_err(|e| miette!("{e}"))?;
    Ok(())
}

/// 写出 vendored dex 到 `vendor/android-compose/dex/`（维护脚本 / 测试 bootstrap）。
pub fn write_vendor_dex_bootstrap(out_dir: &Path) -> Result<()> {
    std::fs::create_dir_all(out_dir.join("dex")).map_err(|e| miette!("{e}"))?;
    let shell = build_bootstrap_compose_dex()?;
    std::fs::write(out_dir.join("dex").join(COMPOSE_SHELL_DEX), &shell).map_err(|e| miette!("{e}"))?;
    std::fs::write(out_dir.join("dex/androidx-compose-baseline.dex"), build_androidx_compose_baseline()).map_err(|e| miette!("{e}"))?;
    std::fs::write(out_dir.join("dex/asgard-runtime.dex"), build_asgard_runtime_dex()).map_err(|e| miette!("{e}"))?;
    let class_defs = acorn_dex::dex::dex_class_defs_count(&shell).unwrap_or(0);
    let manifest = format!(
        r#"{{
  "compose": "1.6.0",
  "activity": "1.8.2",
  "material3": "1.2.0",
  "minSdk": 24,
  "dex": {{
    "{COMPOSE_SHELL_DEX}": "d8 合并产物（resolve 优先加载）；bootstrap class_defs={class_defs}",
    "androidx-compose-baseline.dex": "中间产物（deprecated；SDK 编译可替换）",
    "asgard-runtime.dex": "中间产物（deprecated）"
  }},
  "note": "由 write_vendor_dex_bootstrap 生成"
}}
"#
    );
    std::fs::write(out_dir.join("manifest.json"), manifest).map_err(|e| miette!("{e}"))?;
    Ok(())
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_android_compose_pack.rs"]
mod codegen_android_compose_pack_tests;
