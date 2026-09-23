//! 可选：指向外部 Valkyrie 语言源码检出（非本仓库内容）。
use std::path::PathBuf;

/// `VALKYRIE_V_ROOT` 若存在且为目录则返回，否则 `None`（公开 CI 默认跳过相关测试）。
pub fn root() -> Option<PathBuf> {
    std::env::var_os("VALKYRIE_V_ROOT")
        .map(PathBuf::from)
        .filter(|path| path.is_dir())
}

/// `{VALKYRIE_V_ROOT}/projects`
pub fn projects() -> Option<PathBuf> {
    root().map(|root| root.join("projects")).filter(|path| path.is_dir())
}

/// `{VALKYRIE_V_ROOT}/examples/feature-matrix/test`
pub fn feature_matrix_test() -> Option<PathBuf> {
    root()
        .map(|root| root.join("examples/feature-matrix/test"))
        .filter(|path| path.is_dir())
}
