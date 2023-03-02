//! 已删除路由 — 不得把 IntrinsicOpcode 恢复为 Semantic MIR / Call 权威。
//!
//! 数组 / std 操作经一等 InstructionKind（ArrayGet/Set/Length/…）或
//! Invoke → std adaptor → BackendPrivatePlan 降低。本模块仅使历史
//! `use …::intrinsic_opcode` 路径在类型层失败关闭。

#![allow(dead_code)]
