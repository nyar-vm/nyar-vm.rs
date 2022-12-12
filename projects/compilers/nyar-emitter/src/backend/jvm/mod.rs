//! `JVM` 后端容器入口。
//!
//! 这里按 `class / jar` 两个输出边界收口，
//! 相关格式模型与编解码统一由 `std-data` 提供。

#![warn(missing_docs)]

use std::path::PathBuf;

use miette::{IntoDiagnostic, Result, WrapErr};
use nyar::{
    ControlFlowPayload,
    abstractions::{ArtifactFormat, BackendInputKind, BinaryArch, BinaryFlavor, BinaryTarget, TargetFamily},
    backends::{BackendDescriptor, CompilationOptions, TargetCodeGenBackend},
    packaging::{ArtifactDescriptor, ArtifactSet, TargetLane},
};

pub use std_data::binary::{
    class::{
        ConstantPoolBuilder, JvmClassError, JvmClassFile, JvmCodeBody, JvmFieldSignature, JvmInstruction, JvmMethodDescriptor, JvmMethodRef,
        JvmMethodSignature, JvmTypeDescriptor, decode_instructions, encode_instructions,
    },
    jar::{JvmJarEntry, JvmJarError, JvmJarPackage},
};

const JVM_ACC_PUBLIC: u16 = 0x0001;
const JVM_ACC_STATIC: u16 = 0x0008;

/// `JVM` 二进制后端输入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JvmBinaryBackendInput {
    /// 主类文件。
    pub class_file: JvmClassFile,
    /// 输出目录。
    pub output_dir: PathBuf,
    /// 是否同时落地裸 `class` 文件。
    pub emit_class_file: bool,
    /// Optional state-machine suspend payload (driver wire; binary backend ignores).
    pub control_flow: Option<ControlFlowPayload>,
    /// Singleton holder classes emitted alongside the fragment class.
    pub companion_classes: Vec<JvmClassFile>,
}

/// `JVM` 二进制后端。
pub struct JvmBinaryBackend {
    descriptor: BackendDescriptor,
}

impl JvmBinaryBackend {
    /// 创建一个新的 `JVM` 二进制后端。
    pub fn new() -> Self {
        Self {
            descriptor: BackendDescriptor {
                name: "jvm-jar".to_string(),
                input_kind: BackendInputKind::JvmClassFile,
                supported_targets: vec![BinaryTarget::new(TargetFamily::Jvm, BinaryArch::Any, BinaryFlavor::ManagedClr)],
            },
        }
    }
}

impl Default for JvmBinaryBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl TargetCodeGenBackend for JvmBinaryBackend {
    type Input = JvmBinaryBackendInput;

    fn descriptor(&self) -> &BackendDescriptor {
        &self.descriptor
    }

    fn validate(&self, input: &Self::Input) -> Result<()> {
        if input.class_file.internal_name.trim().is_empty() {
            return Err(miette::miette!("JVM 后端需要有效的主类内部名"));
        }
        if input.class_file.methods.is_empty() {
            return Err(miette::miette!("JVM 后端至少需要一个方法"));
        }
        Ok(())
    }

    fn compile(&self, input: Self::Input, options: &CompilationOptions) -> Result<ArtifactSet> {
        std::fs::create_dir_all(&input.output_dir)
            .into_diagnostic()
            .wrap_err_with(|| format!("创建输出目录失败：{}", input.output_dir.display()))?;

        let mut class_file = input.class_file;
        ensure_java_launcher_entry(&mut class_file);

        let class_bytes = class_file.to_bytes().map_err(|error| miette::miette!("JVM class 编码失败：{error}"))?;

        if input.emit_class_file {
            let relative = format!("{}.class", class_file.internal_name.trim_end_matches(".class"));
            let class_path = input.output_dir.join(&relative);
            if let Some(parent) = class_path.parent() {
                std::fs::create_dir_all(parent).into_diagnostic().wrap_err_with(|| format!("创建 class 输出目录失败：{}", parent.display()))?;
            }
            std::fs::write(&class_path, &class_bytes)
                .into_diagnostic()
                .wrap_err_with(|| format!("写入 class 文件失败：{}", class_path.display()))?;
            for companion in &input.companion_classes {
                let companion_relative = format!("{}.class", companion.internal_name.trim_end_matches(".class"));
                let companion_path = input.output_dir.join(&companion_relative);
                if let Some(parent) = companion_path.parent() {
                    std::fs::create_dir_all(parent)
                        .into_diagnostic()
                        .wrap_err_with(|| format!("创建 companion class 输出目录失败：{}", parent.display()))?;
                }
                let companion_bytes = companion.to_bytes().map_err(|error| miette::miette!("JVM companion class 编码失败：{error}"))?;
                std::fs::write(&companion_path, companion_bytes)
                    .into_diagnostic()
                    .wrap_err_with(|| format!("写入 companion class 失败：{}", companion_path.display()))?;
            }
        }

        let jar_path = input.output_dir.join(format!("{}.jar", options.artifact_name));
        let mut package = JvmJarPackage::new(format!("{}.jar", options.artifact_name));
        package.main_class = Some(internal_name_to_binary_name(&class_file.internal_name));
        package.push_class(&class_file).map_err(|error| miette::miette!("JAR 打包失败：{error}"))?;
        for companion in &input.companion_classes {
            package.push_class(companion).map_err(|error| miette::miette!("JAR 打包 singleton 类失败：{error}"))?;
        }
        let jar_bytes = package.to_bytes().map_err(|error| miette::miette!("JAR 编码失败：{error}"))?;
        std::fs::write(&jar_path, jar_bytes).into_diagnostic().wrap_err_with(|| format!("写入 JAR 文件失败：{}", jar_path.display()))?;

        let mut artifacts = ArtifactSet::default();
        artifacts.push(ArtifactDescriptor {
            name: options.artifact_name.clone(),
            kind: nyar::ArtifactKind::Executable,
            format: ArtifactFormat::RawBinary,
            target: options.target.clone(),
            lane: TargetLane::Jvm,
        });
        if input.emit_class_file {
            artifacts.push(ArtifactDescriptor {
                name: format!("{}.class", class_file.internal_name.replace('/', ".")),
                kind: nyar::ArtifactKind::AssemblyListing,
                format: ArtifactFormat::RawBinary,
                target: options.target.clone(),
                lane: TargetLane::Jvm,
            });
        }
        Ok(artifacts)
    }
}

/// 将 `JVM` 内部类名转换为 Java 二进制类名。
pub fn internal_name_to_binary_name(internal_name: &str) -> String {
    internal_name.trim_end_matches(".class").replace('/', ".")
}

fn ensure_java_launcher_entry(class_file: &mut JvmClassFile) {
    let launcher_descriptor = JvmMethodDescriptor::new(
        vec![JvmTypeDescriptor::array(JvmTypeDescriptor::Object("java/lang/String".to_string()))],
        JvmTypeDescriptor::Void,
    );
    if class_file.methods.iter().any(|method| method.name == "main" && method.descriptor == launcher_descriptor) {
        return;
    }

    let Some(entry_method) =
        class_file.methods.iter().find(|method| method.name == "main" && method.descriptor.parameter_types.is_empty()).cloned()
    else {
        return;
    };

    let mut instructions = vec![JvmInstruction::InvokeStatic(JvmMethodRef {
        owner: class_file.internal_name.clone(),
        name: entry_method.name.clone(),
        descriptor: entry_method.descriptor.clone(),
    })];
    if entry_method.descriptor.return_type != JvmTypeDescriptor::Void {
        instructions.push(JvmInstruction::Pop);
    }
    instructions.push(JvmInstruction::Return);

    class_file.methods.push(JvmMethodSignature {
        name: "main".to_string(),
        descriptor: launcher_descriptor,
        access_flags: JVM_ACC_PUBLIC | JVM_ACC_STATIC,
        code: Some(JvmCodeBody { max_stack: 1, max_locals: 1, instructions }),
    });
}
