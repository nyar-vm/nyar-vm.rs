# parser src

这里是 Valkyrie 语法层源码。

## 职责

- `ast.rs` 定义强类型语法节点。
- `parser.rs` 负责把源码转成 `ValkyrieRoot`。
- 所有跨文件、源码映射相关节点都要带 `span` 或等价位置信息。
- `NamePath` 这类复用节点要保持轻量和明确。

## 禁止

- 不在这里做 `HIR` lowering。
- 不在这里塞入 project、workspace、build graph 等编排对象。
- 不在这里做目标相关 lowering。
