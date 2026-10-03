# mir

这里承载 `MIR (SSA)`。

## 职责
- 以 `SSA` 形式表达目标无关的中层语义。
- 显式建模值、块参数、指令产值与 terminator。
- 为优化和后续 artifact 分区提供稳定输入。
- 控制流统一待办已并入工作区 `projects/readme.md`。

## 禁止
- 不退回语句列表式伪 `MIR`。
- 不塞入 `CLR / JVM / WASM / native` 专属指令。
- 不把 `MIR` 演化成新的全能总线对象。

## 分析入口与生产边界

- `compile_source_to_mir` 消费 `compile_source` 的正式 HIR 展开和调用验证结果，
  不单独解析源码，不绕过 HIR 合同。
- `lower_root_to_mir` 接受 AST，但同样先验证 HIR 语义合同，再降低和验证控制流。
- 分析接口只返回阶段数据，不证明依赖闭包、Canonical 校验、表示规划或产物成功。
  正式产物入口消费 Resolver 提供的有序源码组，由 Compiler 完成依赖链接和全部后续阶段。
- 单源码到 `CompiledProgram` 的辅助方法和 HIR 直连生产器仅用于单元测试；
  不存在按文件路径或预制依赖 HIR 导出直接生产 `CompiledProgram` 的生产方法。
