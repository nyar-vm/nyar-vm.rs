# Nyar VM 維護指南

本指南面向 Nyar VM 項目的**內部維護者和核心開發團隊**，介紹項目架構、模塊職責、內部維護流程和系統級設計決策。

> **目標讀者**: 項目維護者、核心開發團隊成員、系統架構師
> **內容重點**: 內部架構、維護流程、系統設計、代碼組織

## 項目架構概覽

Nyar 採用 Rust Monorepo (Workspace) 架構，每個組件封裝在獨立的 crate 中，提供清晰的依賴關係、獨立的測試環境和高效的並行編譯。

```
nyar-vm/
├── Cargo.toml         # Workspace 根配置
└── projects/
    ├── nyar-types/    # 統一中間表示類型定義
    ├── nyar-aot/      # 基於 Chomsky 的 AOT 編譯器與優化器
    ├── nyar-vm/       # 字節碼解釋器與虛擬機執行核心
    ├── nyar-jit/      # 即時編譯器 (實驗性)
    └── nyar-tools/    # 命令行工具 (Nyar 工具鏈入口)
```

外部依賴:
- `ProjectChomsky`: 編譯器後端優化框架，位於 `../ProjectChomsky`

## 核心設計哲學

Nyar 的架構基於五大設計支柱：

### 1. 五階段流水線 (5-Stage Pipeline)

編譯器是對程序信息進行一系列保義變換的過程，建模為多個精心設計的中間表示串聯的流水線：

#### 階段 1: AST (Abstract Syntax Tree)
- **職責**: 忠實於源代碼的語法結構，保存完整的原始信息（如 SourceSpan）用於診斷。
- **當前實現**: 由前端提供（如 `oak-rust`）。
- **關鍵處理**:
  - **宏展開 (Macro Expansion)**: 執行宏和注記，生成新的 AST 節點。
- **優化**: 
  - 極早期的常數折疊（如字面量拼接）。
  - 語法脫糖。

#### 階段 2: HIR (High-level IR)
- **職責**: 全局語義圖，包含完整的類型信息、作用域和名稱解析結果。
- **關鍵處理**:
  - **符號解析 (Symbol Resolution)**: 
    - 處理導入，構建跨模塊的符號引用。
    - 建立命名空間層級結構，管理變量作用域（Scope）和影子變量（Shadowing）。
  - **類型檢查與推導 (Type Checking & Inference)**: 確保類型安全性。
  - **模式匹配解糖 (Pattern Matching Desugaring)**: 
    - 將複雜的 `match` 結構轉換為決策樹（Decision Tree）。
  - **代數效應脫糖 (Effect Handling Desugaring)**: 將 `try/handle` 結構映射到底層的控制流原語。

#### 階段 3: CFG (Control Flow Graph)
- **職責**: 以基本塊（BasicBlock）形式表示程序執行邏輯，控制流顯式化。
- **關鍵處理**:
  - **控制流線性化**: 將結構化的控制流（if/while/loop）轉換為 Goto 和 SwitchInt.
  - **解糖複雜的控制流**: 
    - 將模式匹配產生的決策樹展開為一系列的基本塊和條件跳轉。
    - 將代數效應的 `yield`/`resume` 映射到顯式的狀態保存和恢復。

#### 階段 4: SSA (Static Single Assignment)
- **職責**: 變量版本化，確保每個變量僅被賦值一次，通過 Phi 節點處理控制流匯合。
- **關鍵處理**:
  - **支配關係計算 (Dominance Analysis)**: 計算支配樹和支配前沿。
  - **Phi 節點插入**: 在支配前沿處插入 Phi 函數，處理變量合併。
- **優化 (透過 nyar-aot)**:
  - **全局數值編號 (GVN)**: 識別並消除計算上等價的冗餘表達式。
  - **稀疏條件常數傳播 (SCCP)**。
  - **循環不變代碼外提 (LICM)**。

#### 階段 5: LIR (Low-level IR)
- **職責**: 接近目標機器的線性指令集。
- **關鍵處理**:
  - **SSA 銷毀**: 通過映射到棧槽或局部變量來消除 Phi 節點。
  - **指令選擇 (Instruction Selection)**: 根據目標平台（如 Nyar 字節碼）選擇最優指令。
- **雙重角色**:
  - **編譯目標**: 作為生成 WebAssembly 等外部目標的中間表示。
  - **虛擬機目標**: `NyarVM` 作為一個參考解釋器，直接運行 LIR 指令集。
- **設計決策**:
  - **棧式計算機 (Stack Machine)**: Nyar LIR 採用棧式模型，以追求簡單、緊湊和易於生成。

## 核心模塊詳解

### nyar-types: 中間表示類型定義
**職責**: 集中管理各階段的中間表示類型定義。

### nyar-aot: AOT 編譯器與優化器
**職責**: 實現階段間的變換（Pass）與最終字節碼生成。

### nyar-vm: 虛擬機與解釋器
**職責**: 實現棧式字節碼解釋器與運行時。

### nyar-tools: 命令行工具
**職責**: 提供運行和編譯 Nyar 程序的命令行入口。

---

## 設計與實現專題

- [項目架構設計](project-architecture.md)
- [執行模型 (解釋與編譯)](execution-models.md)
- [對象下放 (Object Lowering)](object-lowering.md)
- [包管理與符號解析](package-management.md)
- [優化策略](optimization-strategies.md)
- [後端實現與考量](backends/index.md)

---
本維護指南將隨項目發展持續更新。
