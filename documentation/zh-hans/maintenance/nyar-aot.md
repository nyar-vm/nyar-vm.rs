# nyar-aot

`nyar-aot` 是 Nyar 项目的静态编译驱动器，支持将源码或字节码预先编译为高效的目标平台构件。

## 核心职责

- **多后端适配**: 支持 Native (LLVM/Gaia), WebAssembly (WASI), JVM 等多种后端。
- **全程序优化 (WPO)**: 在编译阶段进行跨模块的内联、死代码消除等优化。
- **构件封装**: 自动生成符合目标平台规范的二进制文件或库。

## 编译流程详解

`nyar-aot` 的核心是一个驱动循环，它协调前端解析器与后端生成器：

1.  **Intent Collection**: 收集前端生成的所有 `IKun` 意图。
2.  **Global Optimization**: 驱动 `ProjectChomsky` 在 E-Graph 中进行大规模变换。
3.  **Cost-based Extraction**: 根据目标后端的 `CostModel` 提取出成本最低（执行最快）的代码树 `IKunTree`。
4.  **Artifact Generation**:
    -   **Native**: 生成 ELF/Mach-O/PE 可执行文件。
    -   **WASM**: 生成符合 WASI 标准的 `.wasm` 组件。
    -   **JVM/CLR**: 生成 `.class` 或 `.dll` 文件。

## 后端开发接口

要为一个新平台开发 AOT 支持，需要实现 `chomsky_extract::Backend` trait：

```rust
impl Backend for MyNewBackend {
    fn get_model(&self) -> &dyn CostModel {
        &self.custom_cost_model
    }

    fn generate(&self, tree: &IKunTree) -> Result<BackendArtifact, String> {
        // 将 IKunTree 映射到目标平台指令
        let binary = self.emit_code(tree);
        Ok(BackendArtifact::Binary(binary))
    }
}
```

## 使用示例

通过 `nyar-tools` 调用 AOT 编译：

```bash
# 编译为原生二进制
nyarc compile main.java --target native --output main.exe

# 编译为 WASM
nyarc compile main.kt --target wasm --output main.wasm
```
