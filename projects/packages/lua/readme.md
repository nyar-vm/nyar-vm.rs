# @nyar/lua

Lua 模拟包。提供与原版 Lua 解释器同名、同调用形式的 `lua` 入口，执行由 `@nyar/nyar`（legend-vm 基板）承担；解析与求值在 `examples/rusty-lua` 中实现，本包不重复实现。

## 依赖顺序

```text
@nyar/lua ──► @nyar/nyar ──► @nyar/nyar-<platform>
```

`@nyar/nyar` 按当前 OS/arch 选择已安装的平台 collect（原生优先、Wasm 兜底），因此本包不直接依赖任何平台包。

## 用法

```bash
lua script.lua
lua -e 'print("hi")'
lua -
```

支持 `-e`、`-`（stdin）、`-v`、`--help`、`--version`；其他选项会以退出码 2 报“unsupported option”，不会静默忽略。没有脚本且 stdin 是 TTY 时明确报告不支持交互式解释器。

## 导出

- `LANGUAGE`：语言 id 与源文件扩展名
- `guestArtifact()`：该语言 guest 模块的约定路径 `guest/lua.nyar`
- `createNyarHost()` / `resolveCollect` / `runHostScript`：来自 `@nyar/nyar` 的宿主入口

## License

MIT
