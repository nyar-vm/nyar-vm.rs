use std::fmt::{Display, Formatter};

use std_data::binary::tvm::{TvmFlags, TvmHeader, TvmOperation};
use text_vm::{
    encoding::TextEncoding,
    engine::{CompiledUnit, EngineLoader},
};

use crate::{
    parser::{self, ParseError},
    static_analyzer::{self, Complexity},
};

/// 编译错误。
#[derive(Debug)]
pub enum CompileError {
    /// 模式解析失败。
    Parse(ParseError),
    /// 不支持的复杂度等级。
    UnsupportedComplexity(Complexity),
    /// `.tvm` 头部编码失败。
    Tvm(std_data::binary::tvm::TvmHeaderError),
}

impl Display for CompileError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Parse(err) => write!(f, "模式解析失败: {err}"),
            Self::UnsupportedComplexity(complexity) => {
                write!(f, "暂不支持的复杂度等级: {complexity:?}")
            }
            Self::Tvm(err) => write!(f, "TVM 编码失败: {err}"),
        }
    }
}

impl std::error::Error for CompileError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Parse(err) => Some(err),
            Self::Tvm(err) => Some(err),
            Self::UnsupportedComplexity(_) => None,
        }
    }
}

impl From<ParseError> for CompileError {
    fn from(value: ParseError) -> Self {
        Self::Parse(value)
    }
}

/// 编译结果：执行单元与对应的 `.tvm` 字节码。
pub struct CompiledQuery {
    /// 可直接调用的执行单元。
    pub unit: Box<dyn CompiledUnit>,
    /// 完整 `.tvm` 文件字节。
    pub tvm_bytes: Vec<u8>,
}

impl CompiledQuery {
    /// 判断输入中是否存在匹配。
    pub fn is_match(&self, input: &[u8]) -> bool {
        self.unit.is_match(input)
    }
}

/// 编译正则模式为静态查询执行单元。
pub fn compile(pattern: &str) -> Result<CompiledQuery, CompileError> {
    compile_with_encoding(pattern, TextEncoding::Utf8)
}

/// 使用指定编码编译模式。
pub fn compile_with_encoding(pattern: &str, encoding: TextEncoding) -> Result<CompiledQuery, CompileError> {
    let parsed = parser::parse(pattern)?;
    let complexity = static_analyzer::analyze(&parsed);

    match complexity {
        Complexity::Literal => {
            let literal_bytes = parsed.literal.as_bytes().to_vec();
            let tvm_bytes = assemble_literal_tvm(&literal_bytes, encoding)?;
            let unit = EngineLoader::load(&tvm_bytes).map_err(CompileError::Tvm)?;
            Ok(CompiledQuery { unit, tvm_bytes })
        }
        Complexity::Regular | Complexity::ContextFree => Err(CompileError::UnsupportedComplexity(complexity)),
    }
}

fn assemble_literal_tvm(pattern: &[u8], encoding: TextEncoding) -> Result<Vec<u8>, CompileError> {
    let header_size = TvmHeader::HEADER_SIZE;
    let total_size = header_size + pattern.len();

    let header = TvmHeader {
        version: 1,
        encoding: encoding as u8,
        operation: TvmOperation::Find as u8,
        flags: TvmFlags::IS_LITERAL | TvmFlags::HAS_PREFIX,
        min_match_len: pattern.len() as u32,
        literal_prefix_offset: header_size as u32,
        dfa_table_offset: 0,
        dfa_state_count: 0,
        total_size: total_size as u32,
    };

    let mut buffer = vec![0u8; total_size];
    header.encode(&mut buffer).map_err(CompileError::Tvm)?;
    buffer[header_size..].copy_from_slice(pattern);
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use text_vm::tvm;

    use super::*;

    #[test]
    fn compile_hello_is_match() {
        let query = compile("hello").expect("compile");
        assert!(query.is_match(b"say hello world"));
        assert!(tvm::is_match(&query.tvm_bytes, b"say hello world").expect("tvm is_match"));
    }
}
