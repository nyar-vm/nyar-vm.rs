# `legion bootstrap`

`legion bootstrap` 在已有可运行编译器二进制上执行多代构建验证（`seed → v1 → v2`），支持 `clr`、`jvm`、`wasm` 等目标。

## 用法

```bash
legion bootstrap --project <project-dir> --target clr
legion bootstrap --project <project-dir> --target jvm
legion bootstrap --project <project-dir> --target wasm
```

- `--project`：含 `legion.von` 与源码树的工程目录。
- 各代产物须由编译器读取真实源文件生成。

## 调试

```bash
legion spy jvm <artifact.jar> --list
legion spy jvm <artifact.jar> --func demo/Main --method main
```

## 测试

```bash
cargo test -p legion --test cmds_bootstrap
```
