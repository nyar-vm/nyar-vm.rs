//! Android JVM 壳类发射（`JvmClassFile` → dex/035）。
//!
//! 已由 [`super::android_compose_pack`] 取代；保留供历史对照。

use acorn_dex::dex::DexImageBuilder;
use acorn_jvm::class::{JvmClassFile, JvmCodeBody, JvmInstruction, JvmMethodDescriptor, JvmMethodRef, JvmMethodSignature, JvmTypeDescriptor};
use miette::Result;

const ACC_PUBLIC: u16 = 0x0001;
const ACC_SUPER: u16 = 0x0020;
const ACC_STATIC: u16 = 0x0008;
const ACC_NATIVE: u16 = 0x0100;

/// 发射 Android 运行时壳类并写出 `classes.dex` 字节（不含尾段嵌入）。
pub fn emit_android_shell_dex() -> Result<Vec<u8>> {
    let mut builder = DexImageBuilder::new();
    for class in [emit_host_bridge(), emit_compose_runtime(), emit_main_activity()] {
        let bytes = class.to_bytes().map_err(|e| miette::miette!("{e}"))?;
        builder.add_class(&class.internal_name, &bytes);
    }
    builder.build().map_err(|e| miette::miette!("{e}"))
}

fn method_ref(owner: &str, name: &str, descriptor: &str) -> JvmMethodRef {
    JvmMethodRef {
        owner: owner.into(),
        name: name.into(),
        descriptor: JvmMethodDescriptor::parse(descriptor).unwrap_or_else(|_| JvmMethodDescriptor::new(vec![], JvmTypeDescriptor::Void)),
    }
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
            max_locals: 2,
            instructions: vec![
                JvmInstruction::ALoad(0),
                JvmInstruction::InvokeStatic(method_ref("com/asgard/runtime/AsgardComposeRuntime", "findAsgardNativeSection", "([B)[B")),
                JvmInstruction::ALoad(1),
                JvmInstruction::InvokeVirtual(method_ref("java/io/File", "getAbsolutePath", "()Ljava/lang/String;")),
                JvmInstruction::InvokeStatic(method_ref("java/lang/System", "load", "(Ljava/lang/String;)V")),
                JvmInstruction::Return,
            ],
        }),
    });
    class
}

fn emit_compose_runtime() -> JvmClassFile {
    let mut class = JvmClassFile::new("com/asgard/runtime/AsgardComposeRuntime");
    class.access_flags = ACC_PUBLIC | ACC_SUPER;

    class.methods.push(JvmMethodSignature {
        name: "patch".into(),
        descriptor: JvmMethodDescriptor::new(
            vec![JvmTypeDescriptor::Object("java/lang/String".into()), JvmTypeDescriptor::Object("java/lang/String".into())],
            JvmTypeDescriptor::Void,
        ),
        access_flags: ACC_PUBLIC | ACC_STATIC,
        code: Some(JvmCodeBody { max_stack: 0, max_locals: 2, instructions: vec![JvmInstruction::Return] }),
    });

    class.methods.push(JvmMethodSignature {
        name: "findAsgardNativeSection".into(),
        descriptor: JvmMethodDescriptor::new(
            vec![JvmTypeDescriptor::Array(Box::new(JvmTypeDescriptor::Byte))],
            JvmTypeDescriptor::Array(Box::new(JvmTypeDescriptor::Byte)),
        ),
        access_flags: ACC_PUBLIC | ACC_STATIC,
        code: Some(JvmCodeBody { max_stack: 1, max_locals: 1, instructions: vec![JvmInstruction::AConstNull, JvmInstruction::AReturn] }),
    });

    class.methods.push(JvmMethodSignature {
        name: "loadFromProduct".into(),
        descriptor: JvmMethodDescriptor::new(
            vec![JvmTypeDescriptor::Array(Box::new(JvmTypeDescriptor::Byte)), JvmTypeDescriptor::Object("java/io/File".into())],
            JvmTypeDescriptor::Void,
        ),
        access_flags: ACC_PUBLIC | ACC_STATIC,
        code: Some(JvmCodeBody {
            max_stack: 2,
            max_locals: 2,
            instructions: vec![
                JvmInstruction::ALoad(0),
                JvmInstruction::ALoad(1),
                JvmInstruction::InvokeStatic(method_ref("com/asgard/runtime/AsgardHostBridge", "loadNativeFromProduct", "([BLjava/io/File;)V")),
                JvmInstruction::Return,
            ],
        }),
    });
    class
}

fn emit_main_activity() -> JvmClassFile {
    let mut class = JvmClassFile::new("com/asgard/runtime/MainActivity");
    class.super_name = "android/app/Activity".into();

    class.methods.push(JvmMethodSignature {
        name: "onCreate".into(),
        descriptor: JvmMethodDescriptor::new(vec![JvmTypeDescriptor::Object("android/os/Bundle".into())], JvmTypeDescriptor::Void),
        access_flags: ACC_PUBLIC,
        code: Some(JvmCodeBody {
            max_stack: 2,
            max_locals: 2,
            instructions: vec![
                JvmInstruction::ALoad0,
                JvmInstruction::ALoad(1),
                JvmInstruction::InvokeSpecial(method_ref("android/app/Activity", "onCreate", "(Landroid/os/Bundle;)V")),
                JvmInstruction::Return,
            ],
        }),
    });
    class
}
#[cfg(test)]
#[path = "../../tests/path_wired/codegen_android_shell.rs"]
mod codegen_android_shell_tests;
