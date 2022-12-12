# NyarVM 維護指南

NyarVM 是 Nyar 的內部參考解釋器。

## 編譯流水線

`Source -> AST -> HIR -> CFG -> SSA -> LIR -> VM Execution`

### 1. AST -> HIR
- 負責：消除語法糖，解析作用域，處理陰影（Shadowing）。
- 實現：由前端提供（如 `oak-rust`）。

### 2. HIR -> CFG
- 負責：將結構化控制流（if, while, loop）轉換為基本塊和跳轉。
- 實現：`nyar-aot`。

### 3. CFG -> SSA
- 負責：構造靜態單賦值形式，插入 Phi 節點。
- 實現：`nyar-aot`。

### 4. SSA -> LIR
- 負責：Phi 消除，以及向下放到棧式指令。
- 實現：[compiler.rs](file:///e:/%E6%99%AE%E9%81%8D%E4%BC%98%E5%8C%96/nyar-vm/projects/nyar-vm/src/bytecode/compiler.rs)。
- 當前狀態：
    - [x] 基礎 Phi 消除（通過在前驅塊末尾插入 Move）。
    - [x] 基礎指令降級為棧式字節碼。
    - [ ] 優化棧的使用（減少冗餘的 Push/Pop）。
    - [x] 局部變量槽位分配。
    - [x] 棧幀深度計算。

### 5. VM Execution
- 負責：執行 LIR 指令，管理運行棧、堆和 Effect 處理器。
- 實現：[interpreter.rs](file:///e:/%E6%99%AE%E9%81%8D%E4%BC%98%E5%8C%96/nyar-vm/projects/nyar-vm/src/vm/interpreter.rs)。
- 當前狀態：
    - [x] 基礎算術與邏輯指令。
    - [x] 跳轉與分支（Jmp, JmpIf）。
    - [x] 函數調用與返回（支持 Native 與 FFI）。
    - [x] 對象與數組操作（Alloc, Load, Store）。
    - [x] 結構化錯誤處理（Effect, Continuation）。
    - [x] 代碼覆蓋率統計。

### 6. FFI 機制
- **設計**：FFI 標記通過 Annotation (如 `@import`) 掛載於 `micro` 函數聲明之上。
- **優勢**：
    - **類型安全**：利用函數的 `parameters` 和 `returns` 定義，編譯器能準確生成參數封送（Marshaling）代碼。
    - **多後端適配**：通過 `target` 參數（如 `wasm`, `jvm`, `clr`, `dll`）由不同後端解釋執行。
- **示例**：
    ```nyar
    @import(target: wasm, "wasi:random/insecure", "get-insecure-random-u64")
    micro get_random_u64() -> u64
    ```

### 7. Deep JIT Optimization
- **觸發層級**：JIT 編譯器工作在 **SSA / HIR** 層面，而非 LIR。
- **核心邏輯**：
    - **語義保留**：在 SSA 層面保留了完整的類型信息、對象邊界和顯式的效應（Effects）流向。
    - **優化能力**：
        - **跨函數內聯**：在 SSA 層面根據熱點進行深度內聯。
        - **類型特化**：利用運行時反饋，在 SSA 層面生成特定類型的快速路徑。
        - **內存優化**：進行更精確的逃逸分析，將對象標量化。

### 棧式計算機
NyarVM 的 LIR 是一個棧式指令集，優化了生成難度，並且能更自然地與現代運行時對接。

### Phi 消除
通過消除 Phi 節點並將 SSA 變量映射到棧槽或局部變量，將 SSA 降低為 LIR。
