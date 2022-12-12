# Valkyrie 編譯器：降級指南 (Lowering Guide)

## 1. 降級哲學：漸進式精煉

Valkyrie 編譯器的核心架構建立在**漸進式降級 (Progressive Lowering)** 之上。編譯過程不是一次性轉換，而是一場從高度抽象到極端具體的旅程。

```mermaid
graph TD
    subgraph Frontend
        A[源代碼] -->|解析| B(<b>oak-valkyrie</b><br><i>語法結構</i>);
    end

    subgraph Mid-end (The Lowering Pipeline)
        B -->|<b>HIR Lowering</b>| C(<b>HIR</b><br><i>類型, 作用域, Traits</i>);
        C -->|<b>CFG Lowering</b>| D(<b>CFG</b><br><i>基本塊, 控制流</i>);
        D -->|<b>SSA Transform</b>| E(<b>SSA</b><br><i>數據流優化</i>);
        E -->|<b>LIR Lowering</b>| F(<b>LIR</b><br><i>虛擬指令, 寄存器</i>);
    end

    subgraph Backends (Multi-Export)
        C -.->|Source Export| G[<b>C Backend</b>];
        D -.->|Structural Export| H[<b>WASM Backend</b>];
        E -.->|Flow Export| I[<b>LLVM Backend</b>];
        F -->|Execution| J[<b>Valkyrie VM</b>];
    end
```

## 2. 多層級後端轉換 (Backend Lowering)

為了兼顧執行效率和多平台兼容性，Valkyrie 允許後端從不同的 IR 層級接入。

### 2.1 每一層 IR 的轉換優勢

| 接入層級 | 轉換目標 | 核心優勢 |
| :--- | :--- | :--- |
| **HIR** | C / TypeScript | 保留了完整的高級語法語義，適合源碼級導出和跨語言互操作。 |
| **CFG** | WebAssembly | WASM 需要結構化的控制流。在 CFG 層級可以利用 Relooper 算法更容易地生成 `loop` 和 `if` 結構。 |
| **SSA** | LLVM IR | LLVM 是基於 SSA 的優化器。在此層級接入可以無縫對接 LLVM 的全局優化（GVN, SCCP 等）。 |
| **LIR** | VM / Native | 針對 Valkyrie 運行時優化的線性指令集，適合解釋執行、JIT 或生成高度定制的 AOT 機器碼。 |

### 2.2 LIR 轉換到 WASM/LLVM 的難點

雖然可以從 LIR 轉換到 WASM 或 LLVM，但存在以下顯著困難：

1.  **控制流丟失**: LIR 是基於跳轉（Jumps/Branches）的扁平指令流。WASM 要求嚴格的結構化嵌套，從 LIR 恢復控制流結構非常昂貴且複雜。
2.  **寄存器 vs SSA**: LIR 使用物理風格的虛擬寄存器。轉換到 LLVM SSA 需要重新進行“寄存器提升”（Mem2Reg），這相當於逆轉了 LIR 的生成過程。
3.  **代數效應的底層實現**: LIR 中的 `Yield/Raise` 是基於運行時 Frame 快照實現的。WASM 或 LLVM 並不直接支持這種細粒度的狀態保存，需要通過特殊的變換（如 Asyncify 或 Continuation Passing Style）來實現，性能損耗較大。

## 3. 特性降級示例：模式匹配 (Pattern Matching)

模式匹配是 Valkyrie 的核心特性。我們將追蹤它如何從 AST 降級到最終的控制流。

### 3.1 AST 階段
在 AST 中，`match` 是一個直接反映語法的節點：
- `ast::Match { scrutinee, arms }`
- 每個 arm 包含 `pattern`, `guard`, 和 `body`。

### 3.2 HIR 階段：模式解糖與決策樹
在 HIR 中，`match` 表達式經歷以下關鍵變換：
- **模式解糖 (Pattern Desugaring)**: 
  - 將複雜的嵌套模式（如 `Some(Point { x: 1, .. })`）分解為簡單的基元測試：
    1. 測試是否為 `Some`。
    2. 綁定內部值到臨時變數。
    3. 測試該值的 `x` 字段是否等於 `1`。
- **決策樹生成 (Decision Tree Construction)**: 
  - 編譯器構建一個邏輯上的決策樹，優化匹配順序以減少冗餘測試。
  - **窮盡性檢查 (Exhaustiveness Check)**: 使用空間覆蓋算法確保所有可能的輸入都被覆蓋，否則產生編譯錯誤。

### 3.3 CFG 階段：控制流顯式化
在 CFG 中，決策樹被轉化為具體的基本塊結構：
- **分支展開**: 每個決策節點轉化為一個 `SwitchInt` 或 `If` 終結符。
- **綁定提升**: 模式匹配中綁定的變數（如 `let Some(x) = ...` 中的 `x`）在進入對應分支塊時被賦值。
- **錯誤塊**: 對於未覆蓋的情況（如果編譯器允許非窮盡匹配），會跳轉到一個顯式的 `Panic` 塊。

## 4. 特性降級示例：代數效應 (Algebraic Effects)

Valkyrie 的代數效應通過**延續 (Continuations)** 和**處理器棧 (Handler Stack)** 實現。

### 4.1 HIR 階段
- **效應類型檢查**: 確保 `try` 塊中觸發的所有效應在 `handle` 中都有定義，或者在函數簽名中聲明了。
- **閉包捕獲**: 將 `handle` 中的每個分支轉化為一個隱式的閉包，用於接收效應參數和恢復點（resume）。

### 4.2 CFG 階段：顯式棧操作
代數效應在 CFG 層面引入了特殊的控制流指令：
- **`PushHandler`**: 在進入 `try` block 前，將當前處理器的引用壓入虛擬機的處理器棧。
- **`PopHandler`**: 在退出 `try` block 時，彈出對應的處理器。
- **`EffectCall`**: 觸發一個效應，它本質上是一個非局部的跳轉，查找處理器棧並跳轉到匹配的 `handle` 塊。

### 4.3 LIR 階段：虛擬機原語
最終，這些操作映射到 `ValkyrieVM` 的核心指令：
- `Raise { effect, resume_target }`: 暫停當前執行流，尋找處理器。
- `Yield { value, resume_target }`: 用於實現生成器或異步操作。
- `PushHandler` / `PopHandler`: 操縱 `ValkyrieVM` 結構中的 `handler_stack`。
