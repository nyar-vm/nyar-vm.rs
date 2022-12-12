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
