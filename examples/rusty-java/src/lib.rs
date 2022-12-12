#![warn(missing_docs)]
#![feature(new_range_api)]
//! Mini Java 语言前端

use nyar_types::{Id, NyarContext, NyarError, NyarFrontend, Vfs};
use oak_core::source::SourceText;
use oak_java::{JavaBuilder, JavaLanguage, JavaRoot};
use chomsky_types::Loc;
use chomsky_uir::{Analysis, IKun, egraph::HasDebugInfo};

pub mod codegen;
pub mod row_type;
pub mod tagless;
pub mod visitor;
pub mod runtime;

pub use crate::runtime::MiniJavaRuntime;

/// Mini Java 前端
pub struct MiniJavaFrontend<'a> {
    _language: &'a JavaLanguage,
    builder: JavaBuilder<'a>,
}

impl<'a> Default for MiniJavaFrontend<'a> {
    fn default() -> Self {
        // Use a leaked language for simplicity in this example to satisfy lifetimes
        let language = Box::leak(Box::new(JavaLanguage::default()));
        Self::new(language)
    }
}

impl<'a> MiniJavaFrontend<'a> {
    /// 创建新的前端实例
    pub fn new(language: &'a JavaLanguage) -> Self {
        Self {
            _language: language,
            builder: JavaBuilder::new(language),
        }
    }
}

impl<'a, A: Analysis<IKun> + 'static> NyarFrontend<A, JavaRoot> for MiniJavaFrontend<'a> 
where A::Data: HasDebugInfo
{
    type Language = JavaLanguage;

    /// 解析 Java 源代码
    fn parse(&self, source: &str) -> Result<JavaRoot, NyarError> {
        let mut session = oak_core::parser::ParseSession::<JavaLanguage>::default();
        let source_text = SourceText::new(source);
        let output = self.builder.build(&source_text, &[], &mut session);
        output
            .result
            .map_err(|e| NyarError::Compile(format!("{:?}", e)))
    }

    /// 统一的接入接口，支持 EGraph 优化流
    fn lower_unified<V: Vfs>(&self, ast: &JavaRoot, ctx: &mut NyarContext<'_, V, A>) -> Id {
        let mut converter = codegen::JavaUirConverter::new(ctx);
        converter.convert_root(ast).unwrap_or_else(|_| {
            let loc = Loc::default();
            ctx.builder().constant(0, loc)
        })
    }
}
