# asgard

Asgard 编译器：AWSL 前端、打包与 Wasm 交付。

## 职责

- AWSL 源码解析、降低与渲染 IR
- 多平台打包（终端、桌面、移动端、SSG）
- 与 `legion-workspace` 协作完成 manifest 与构建计划

语义分析与 Nyar 后端在 `nyar-vm.rs` git 依赖中实现；本 crate 负责 Asgard 专用前端与交付链。

## 构建

```bash
cargo test -p asgard
```

npm 装配见 [`projects/packages/asgard`](../../packages/asgard/package.json)。
