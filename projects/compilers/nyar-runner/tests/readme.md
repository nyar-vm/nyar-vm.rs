# interpreter tests

这里仅保留运行时门面测试。

## 职责
- 覆盖 `RuntimeFamily`、运行模板和命令展开这类纯 runtime 语义。
- 保证 `nyar-runner` 只消费编译产物，不感知任何前端语言源码。
- 真实源码 fixture 与集成冒烟测试已迁到 `legion/tests`。
