# legion src

这里放 `legion` 的源码入口与轻量编排层。

## 职责

- `main.rs` 负责命令行入口。
- `manifest.rs`、`von.rs` 负责清单与配置读取。
- `planner.rs` 负责构建计划，不承载语言语义。

## 禁止

- 不在这里解析 Valkyrie 语法。
- 不在这里保存 `HIR / MIR / LIR` 结构。
- 不把 planner 演化成新的伪 `IR`。
