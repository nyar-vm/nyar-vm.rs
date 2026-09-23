use std_data::binary::tvm::TvmHeaderError;

use crate::engine::{EngineLoader, Match};

/// TextVM 运行时入口。提供极简的静态方法存在判定 / 查找 / 替换。
pub fn is_match(engine_bytes: &[u8], input: &[u8]) -> Result<bool, TvmHeaderError> {
    Ok(EngineLoader::load(engine_bytes)?.is_match(input))
}

/// 查找第一个匹配区间。
pub fn find(engine_bytes: &[u8], input: &[u8]) -> Result<Option<Match>, TvmHeaderError> {
    Ok(EngineLoader::load(engine_bytes)?.find_first(input))
}

/// 查找所有匹配区间。
pub fn find_all(engine_bytes: &[u8], input: &[u8]) -> Result<Vec<Match>, TvmHeaderError> {
    Ok(EngineLoader::load(engine_bytes)?.find_all(input))
}

/// 执行替换，返回替换后的新字节序列。
pub fn replace(engine_bytes: &[u8], input: &[u8], replacement: &[u8]) -> Result<Vec<u8>, TvmHeaderError> {
    Ok(EngineLoader::load(engine_bytes)?.replace(input, replacement))
}
