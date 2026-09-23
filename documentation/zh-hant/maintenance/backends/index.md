# 後端維護指南

本文檔介紹了 Nyar 編譯器支持的各種後端的編譯流程和設計考量。

## 概覽

Nyar 支持多個後端，每個後端都有其自身的特性和優化策略。通用流水線為：
`Source -> AST -> HIR -> CFG -> SSA -> LIR -> Target`

## 後端實現

- [JVM 後端](jvm.md): 針對 Java 虛擬機，採用基於棧的架構。
- [WASI 後端](wasi.md): 針對 WebAssembly，採用基於棧的架構。
- [CLR 後端](clr.md): 針對 .NET 運行時，採用基於棧的架構。
- [Native 後端](native.md): 針對原生指令集（x86, x64, arm64, riscv）的評估方案。
- [NyarVM](nyar-vm.md): 參考解釋器，採用基於棧的架構。

## 設計決策

### 棧式計算機優先

Nyar 的 LIR 和多數目標後端（如 JVM, WASM）都採用棧式架構。這簡化了編譯器的後端設計，並能更自然地與現代運行時對接。
