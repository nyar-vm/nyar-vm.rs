//! 报告数据模型。

use serde::{Deserialize, Serialize};

/// 单个测试结果条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestResultEntry {
    /// 测试名称（workspace 模式下可带 `project::` 前缀）。
    pub name: String,
    /// 状态：`pass` / `fail` / `skip` / `compile_error`。
    pub status: String,
    /// 错误信息。
    pub error: Option<String>,
    /// 目标平台标签。
    pub target: String,
}

/// 覆盖率特性条目。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageFeatureEntry {
    /// 特性标识。
    pub feature: String,
    /// 显示名称。
    pub display: String,
    /// 是否已覆盖。
    pub covered: bool,
    /// 覆盖该特性的项目名。
    pub projects: Vec<String>,
}

/// 覆盖率汇总报告。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CoverageReport {
    /// 已覆盖数量。
    pub covered: usize,
    /// 特性总数。
    pub total: usize,
    /// 覆盖百分比。
    pub percentage: f64,
    /// 特性列表。
    pub features: Vec<CoverageFeatureEntry>,
}

/// 单条基准结果。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchResultRow {
    /// 项目名。
    pub project: String,
    /// 基准函数名。
    pub test: String,
    /// 目标平台。
    pub target: String,
    /// 平均编译耗时（毫秒）。
    pub compile_ms: f64,
    /// 平均运行耗时（毫秒）。
    pub runtime_ms: f64,
}

/// 基准报告。
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BenchReport {
    /// 运行次数。
    pub runs: usize,
    /// 结果列表。
    pub rows: Vec<BenchResultRow>,
}
