# compiler src

这里是编译主链源码。

## 职责
- 维护 `HIR -> Semantic MIR (SSA) -> CanonicalProgram -> RepresentationPlan`；目标私有计划与编码属于 emitter。
- 让语义在目标前闭合，避免后端兜底补语义。
- 为 `CLR` 主线与 `JVM / WASM / native` 冒烟护栏提供同一前端事实。
- 与 `tests/spec` 一起维护 `row / trait / class / sealed class / unite / effect` 的语义边界。

## 禁止
- 不引入统一跨端 `god ir`。
- 不把轻量 planner 长成语义总线。
- 不让目标无关层持有目标宿主实现细节。

## 唯一执行载荷边界

前端不再公开 `mir_function_to_executable` 或
`mir_functions_to_executable_map`。原有直接从语言 MIR 投影 executable 的
转换器和独立可达闭包实现已删除；它们绕过 canonical 校验与表示规划，
不能为 emitter 生成第二套成功载荷。

依赖旧 provider 注入的值存储测试随该入口移除。这不证明值存储、循环、
运行时或自举已通过；对应覆盖须从当前源码经过 `CompiledProgram` 和
目标私有准备边界重新建立。装配摘要与内部 callable 名称查找仍待收敛，
不能把删除旧公开入口称为唯一编译流完成。
