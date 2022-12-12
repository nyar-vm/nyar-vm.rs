# WASM 後端維護指南 (WASI Preview 2)

WASM 後端負責將 Valkyrie 編譯為 WebAssembly 組件 (Component) 格式，遵循 WASI Preview 2 標準。

## 1. 編譯流水線

`Source -> AST -> HIR -> CFG -> WASM Component`

目前的實現直接從 `CFG` 生成 `WASM` 指令，並封裝為 `WebAssembly Component`。

## 2. 設計考量

### 棧機架構與控制流
- **CFG 線性化**: 使用 `$dispatch` 循環和 `br_table` 實現非結構化控制流到 WASM 結構化控制流的映射。
  - **Relooper 結構**: 採用嵌套 `block` 結構。每個 CFG 基本塊對應一個 `block`。
  - **調度邏輯**: 使用一個局部變量 `$dispatch_local` 存儲下一個要執行的塊索引。
- **局部變量**: Cfg 中的 Local 映射為 WASM 的 `local`。Local 0 約定為返回值。

### 類型與運算
- **類型映射**: `CfgType` 映射為 WASM `ValType`。
  - 指針、引用、數組、結構體等在 WASM 線性內存中統一映射為 `i32` (wasm32) 或 `i64` (wasm64)。
- **算術運算**: 根據操作數類型自動選擇指令（如 `i64.add`, `f64.add`）。目前默認整數運算使用 `i64`。
- **常量池**: 字符串常量在編譯時收集，存儲在 WASM `DataSection` 中，運行時通過偏移量訪問。

### 內存佈局與對齊
- **線性內存**: 採用 `wasm-encoder` 構建。
- **結構體佈局**: 
  - 字段按照定義的順序排列。
  - **內存對齊**: 每個字段根據其類型的自然對齊要求進行對齊（如 `i64` 對齊到 8 字節）。
  - **總大小**: 結構體總大小對齊到其成員中的最大對齊值。
- **枚舉佈局**: 
  - 採用 `Tag + Payload` 模式。
  - Tag 為 `i32`，位於偏移量 0。
  - Payload 緊隨其後，並根據成員的最大對齊要求進行對齊。
  - **變體字段**: 支持帶有字段的枚舉變體，通過組件模型的 `variant` 類型進行映射。
- **內存管理**:
  - **堆分配**: 核心模塊實現了 `cabi_realloc` 函數，遵循 Canonical ABI 標準。
  - **分配算法**: 目前採用簡單的 Bump Allocation (指針碰撞) 算法，適用於短期運行或小型腳本。
  - **堆指針**: 使用 WASM 全局變量 (Global) 跟蹤當前堆頂，初始位置緊跟在 `DataSection`（常量池）之後。

### 組件模型 (Component Model / WASI P2)
- **多模塊架構**: 
  - `MockMemory`: 負責導出線性內存，作為組件內部的單一事實來源。
  - `Main`: 核心邏輯模塊，導入 `MockMemory` 導出的內存，並導出 `cabi_realloc` 用於內存管理。
- **接口對接 (Docking)**:
  - **wasi:cli/stdout**: 已實現接口導入，支持通過 `get-stdout` 獲取標準輸出句柄。
  - **wasi:io/streams**: 已實現 `write` 接口對接，支持向輸出流寫入字節序列。
  - **Canonical ABI**: 
    - 使用 `canon lower` 將組件級別的函數（如 `write`）降低為核心模塊可調用的函數。
    - 降低過程關聯了 `MockMemory` 提供的內存，以支持 `list<u8>` (string) 類型的傳遞。
- **實例化與鏈接**: 使用 `ComponentInstanceSection` 和 `ComponentAliasSection` 在組件內部完成模塊的實例化和鏈接。目前已支持多層級的別名映射，確保核心模塊能正確識別並調用降低後的 WASI 函數。

## 3. Valkyrie 特性處理

WASM 後端通過 `WasmConfig` 進行配置：

- **Variant**: 支持 `wasm32` 和 `wasm64`。
  - `wasm32`: 使用 32 位地址空間，這是目前的主流選擇。
  - `wasm64`: 使用 64 位地址空間，適用於需要大內存支持的場景。
- **Effect Lowering**:
  - `experimental_stack_switch`: 布爾值。若為 `true`，則嘗試使用 WASM 原生的 `stack-switching` 提案（方案 B）；若為 `false`，則回退到兼容性更好的 CPS 變換（方案 C）。

### Trait 與多態
- **實現方案**: 採用經典的 VTable (Virtual Method Table) 方案。
- **內存佈局**: 對象頭包含一個指向線性內存中 VTable 的偏移量。VTable 存儲函數索引 (Function Index)。
- **調用方式**: 使用 `call_indirect` 指令根據 VTable 中的索引動態調用函數。

### 代數效應 (Algebraic Effects)
Valkyrie 的核心特性之一是代數效應，在 WASM 中的實現具有挑戰性。隨著瀏覽器對新提案的支持，目前規劃如下：
- **方案 A (Asyncify)**: 利用 Binaryen 的 `asyncify` 工具在用戶態保存和恢復調用棧。（不再作為首選方案）
- **方案 B (Stack Switching)**: 利用 WASM 原生的 `stack-switching` 提案。這是實現可恢復（resumable）效應的最優路徑。可通過 `experimental_stack_switch = true` 開啟。
- **方案 C (CPS 變換)**: 在編譯階段將帶有效應的代碼轉換為續體傳遞風格 (Continuation Passing Style)。這是默認方案 (`experimental_stack_switch = false`)。

**當前狀態與評估**: 
- `Raise`: 
    - **非恢復路徑 (Non-resumable)**: 鑒於 WASM `exception-handling` 提案已在瀏覽器實裝，我們將優先使用 `throw` 指令實現 `Raise`。這使得效應在不恢復時表現為標準異常。
    - **可恢復路徑**: 依賴 `stack-switching` 或 `CPS` 變換。
- `PushHandler` / `PopHandler`: 需要結合 `try-catch` 或 `try_table` 指令實現。

### 內存管理 (Memory Management)
- **線性內存模型**: 目前實現了一個極簡的 Bump Allocator (`cabi_realloc`)。由於缺乏 `free`，僅適用於短期任務。
- **WASM GC 模型**: 鑒於 WASM GC 提案已實裝，目前已引入基於 GC 對象的類型表達支持。
    - **當前進度**: 
        - 結構體和數組已支持映射為 WASM 的 `struct` 和 `array` 類型。
        - 已實現基於 `struct.new` / `array.new_fixed` 的對象分配。
        - 已實現基於 `struct.get` / `struct.set` 和 `array.get` / `array.set` 的字段與索引訪問。
    - **優勢**: 消除內存洩漏，增強安全性，並簡化 AE 續體中的對象生命週期管理。
- **建議**: 並行保留線性內存模型（用於底層 FFI）和 GC 模型（用於 Valkyrie 原生類型）。可通過 `experimental_gc` 配置項開啟。

### 複雜類型支持
- **聚合類型 (Struct/Array)**:
    - 基礎分配已實現，但 `emit_load` / `emit_store` 尚不支持聚合類型的按值拷貝（Memcpy）。
    - 數組字面量在 `AST -> HIR` 階段存在降級丟失問題。
- **字符串 (String)**: 將對接 WASI 的組件模型字符串表達，直接在 WASM 二進制中編碼相關的類型定義。

### 組件模型與工具鏈 (Component Model)
- **直接構建**: 我們不依賴 `wit-component` 等外部工具。WASI Preview 2 所需的組件包裝、類型聲明（如 `WIT` 對應的部分）均通過直接寫入 WASM 二進制（Component Section）的形式實現。
- **斷裂點**: `AST -> HIR` 和 `HIR -> CFG` 階段對某些複雜表達式（如數組、閉包）的處理存在缺失。
- **驗證**: 持續通過 `wasi_test.rs` 跟蹤修復進度。目前 `arithmetic` 通過，`struct/array/control_flow` 仍受限於上述缺失特性。

## 4. 剩餘缺失特性與待辦事項 (Missing Features & Roadmap)

### 核心功能
- [ ] **代數效應 (AE)**:
    - [ ] 實現 `Raise` 的非恢復路徑（映射至 WASM `throw` 指令）。
    - [ ] 實現 `PushHandler` / `PopHandler`（映射至 WASM `try-catch` 或 `try_table`）。
    - [ ] 研究並實現 `stack-switching` 提案下的續體恢復邏輯。
- [ ] **Trait 與多態**:
    - [ ] 設計並實現線性內存中的 VTable 佈局。
    - [ ] 實現基於 `call_indirect` 的動態分發。
- [ ] **枚舉 (Enum)**:
    - [ ] 實現 `Tag + Payload` 的線性內存佈局。
    - [ ] 支持 GC 模式下的變體表達（可能映射至 WASM `struct` 的子類或聯合）。

### 優化與增強
- [ ] **GC 模式完善**:
    - [ ] 支持 GC 數組的動態長度分配 (`array.new`)。
    - [ ] 支持 GC 字符串 (`string.new_utf8` 等)。
    - [ ] 實現 GC 對象與線性內存 FFI 的橋接層。
- [ ] **後端架構**:
    - [ ] 增強 `relooper` 邏輯以支持更複雜的控制流（如帶標籤的 `break`）。
    - [ ] 實現 `memcpy` 優化，用於聚合類型的線性內存按值拷貝。

### 工具與驗證
- [ ] **測試覆蓋**:
    - [ ] 修復 `AST -> HIR` 階段對數組、結構體構造函數的處理缺失。
    - [ ] 增加更多針對 GC 模式和 AE 機制的單元測試。
