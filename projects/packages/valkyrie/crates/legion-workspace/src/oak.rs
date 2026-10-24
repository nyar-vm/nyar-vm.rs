//! Oak 诊断边界辅助：在展示层之前保留 `source_offset`。

use std::{fmt::Display, ops::Range};

use miette::{LabeledSpan, Report};
use oak_core::OakError;

/// 将 Oak 单点偏移转为 miette 可用的字节区间。
pub fn source_point_span(error: &OakError) -> Option<Range<usize>> {
    error.source_offset().map(|start| start..start.saturating_add(1))
}

/// 构造带标签的源码标注。
pub fn labeled_span(label: impl Into<String>, span: &Range<usize>) -> LabeledSpan {
    LabeledSpan::new_with_span(Some(label.into()), (span.start, span.end.saturating_sub(span.start)))
}

/// 将 Oak 解析错误转为带可选源码标注的 miette 报告。
pub fn labeled_report(error: OakError, label: impl Into<String>) -> Report {
    let label = label.into();
    if let Some(span) = source_point_span(&error) {
        miette::miette!(labels = [labeled_span(label, &span)], "{error}")
    }
    else {
        miette::miette!("{error}")
    }
}

/// 将 Oak 解析错误包装为带上下文的 miette 报告。
pub fn labeled_report_with_context(error: OakError, context: impl Display, label: impl Into<String>) -> Report {
    let label = label.into();
    if let Some(span) = source_point_span(&error) {
        miette::miette!(labels = [labeled_span(label, &span)], "{context}: {error}")
    }
    else {
        miette::miette!("{context}: {error}")
    }
}
