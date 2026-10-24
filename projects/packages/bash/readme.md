# @nyar/bash

Bash 语言包。只声明语言身份并把宿主能力委托给 `@nyar/nyar`；解析与求值在 `nyar-language` 的 bash guest 中实现，本包不重复实现。

## 依赖顺序

```text
@nyar/bash ──► @nyar/nyar ──► @nyar/nyar-<platform>
```

`@nyar/nyar` 按当前 OS/arch 选择已安装的平台 collect（原生优先、Wasm 兜底），因此本包不直接依赖任何平台包。

## 导出

- `LANGUAGE`：语言 id 与源文件扩展名
- `guestArtifact()`：该语言 guest 模块的约定路径 `guest/bash.nyar`
- `createNyarHost()` / `resolveCollect`：来自 `@nyar/nyar` 的宿主入口

## License

MIT
