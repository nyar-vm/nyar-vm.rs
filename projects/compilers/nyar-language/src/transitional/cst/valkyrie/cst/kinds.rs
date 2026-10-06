//! Valkyrie CST 节点种类。

use std::ops::Range;

use crate::transitional::cst::valkyrie::ast::RootStatement;

/// CST 节点种类标签。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum ValSyntaxKind {
    /// 文件根。
    Root = 1,
    /// 顶层语句。
    Statement = 2,
    /// trivia（注释 / 空白 / 模板指令）。
    Trivia = 3,
    /// 错误恢复占位。
    Error = 4,
}

/// CST 顶层元素。
#[derive(Debug, Clone, PartialEq)]
pub enum ValCstElement {
    /// 独立 trivia（文件头 / 语句间）。
    Trivia {
        /// 原文切片。
        text: String,
        /// 字节范围。
        span: Range<usize>,
    },
    /// 已解析语句（含附着 trivia）。
    Statement {
        /// 语句前 trivia。
        leading: String,
        /// 语句 AST。
        ast: RootStatement,
        /// 语句后 trivia（至下一语句前）。
        trailing: String,
    },
    /// 错误恢复区域。
    Error {
        /// 错误消息。
        message: String,
        /// 原文切片。
        text: String,
        /// 字节范围。
        span: Range<usize>,
    },
}

impl ValSyntaxKind {
    /// 转为平台 `SyntaxNode::kind` 标签。
    pub fn as_u16(self) -> u16 {
        self as u16
    }
}
