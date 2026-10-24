# @nyar/tcl

Tcl 模拟包。提供与原版 Tcl 解释器同名、同调用形式的 `tclsh` 入口，执行由 `@nyar/nyar`（legend-vm 基板）承担；解析与求值在 `examples/rusty-tcl` 中实现，本包不重复实现。

## 依赖顺序

```text
@nyar/tcl ──► @nyar/nyar ──► @nyar/nyar-<platform>
```

`@nyar/nyar` 按当前 OS/arch 选择已安装的平台 collect（原生优先、Wasm 兜底），因此本包不直接依赖任何平台包。

## 用法

```bash
tclsh script.tcl
tclsh - < script.tcl
```

支持 `-`（stdin）、`--help`、`--version`；其他选项会以退出码 2 报“unsupported option”，不会静默忽略。没有脚本且 stdin 是 TTY 时明确报告不支持交互式 shell。

## 导出

- `LANGUAGE`：语言 id 与源文件扩展名
- `guestArtifact()`：该语言 guest 模块的约定路径 `guest/tcl.nyar`
- `createNyarHost()` / `resolveCollect` / `runHostScript`：来自 `@nyar/nyar` 的宿主入口

## License

MIT
