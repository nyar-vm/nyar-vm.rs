# WebAssembly (WASM) 后端

将已验证的 Wasm TargetExecutionPlan 映射到 WASM 指令集。前端语义 IR 和中间 lowering 由具体语言 pipeline 选择，不由 Wasm backend 反向推断。

## 特点

- **WASI 支持**: 兼容 WebAssembly 系统接口，支持文件 IO 与网络。
- **内存模型**: 线性内存管理。
- **部署**: 适用于浏览器环境或 Edge Computing。
