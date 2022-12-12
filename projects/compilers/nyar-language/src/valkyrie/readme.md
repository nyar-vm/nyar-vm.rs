# compiler src

这里是编译主链源码。

## 职责
- 维护 `HIR -> MIR (SSA) -> optimize -> artifact partition -> target LIR`。
- 让语义在目标前闭合，避免后端兜底补语义。
- 为 `CLR` 主线与 `JVM / WASM / native` 冒烟护栏提供同一前端事实。
- 与 `tests/spec` 一起维护 `row / trait / class / sealed class / unite / effect` 的语义边界。

## 禁止
- 不引入统一跨端 `god ir`。
- 不把轻量 planner 长成语义总线。
- 不让目标无关层持有目标宿主实现细节。
