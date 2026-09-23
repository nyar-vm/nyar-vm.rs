# types src

这里放共享类型定义与编译期公共数据结构。

## 职责
- 维护 `SourceSpan`、错误类型、`HIR` 与 witness 相关基础结构。
- 作为 parser、compiler、interpreter 之间的共享契约层。

## 禁止
- 不在这里塞入具体编译流程。
- 不把共享类型层扩成包含行为和流程的上帝模块。
