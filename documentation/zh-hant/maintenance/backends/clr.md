# CLR (.NET) 後端評估與方案

CLR (Common Language Runtime) 是 .NET 平台的運行時環境。與 JVM 類似，CLR 使用基於棧的指令集（CIL - Common Intermediate Language）。

## 1. 編譯流程 (Pipeline)

由於 CLR 也是棧式虛擬機，其編譯流程建議參考 JVM 後端的實現，跳過寄存器分配階段。

- **推薦路徑**: `Source -> AST -> HIR -> CFG -> CIL (Common Intermediate Language)`
- **核心邏輯**:
    - **CFG 線性化**: 將 CFG 的塊按照順序排列。
    - **棧指令映射**: 直接從 CFG 表達式生成 `ldloc`, `stloc`, `add`, `call` 等 CIL 指令。
    - **元數據生成**: 使用 `System.Reflection.Emit` 風格的庫（或直接操作 PE 格式）生成程序集（Assembly）、類（Class）和方法（Method）的元數據。

## 2. 技術選型

### 方案 A: 靜態程序集生成 (AOT-like)
直接生成符合 ECMA-335 標準的 PE 格式二進制文件（.dll 或 .exe）。
- **優點**: 運行不需要編譯器存在，性能好。
- **工具**: 
    - [dnlib](https://github.com/0xd4d/dnlib) (C# 庫，可能需要 FFI)
    - [Kestrel](https://github.com/jbevain/cecil) (Cecil) 的 Rust 替代品或直接生成二進制流。

### 方案 B: 動態生成 (JIT-like)
在運行時使用反射發射指令。
- **優點**: 實現簡單，適合腳本化場景。
- **缺點**: 依賴 .NET 運行時。

## 3. 與 JVM 的差異

1. **值類型 (Struct)**: CLR 原生支持自定義值類型（ValueType），這比 JVM 目前的實現（Project Valhalla 尚在路上）更強大。Valkyrie 的 `struct` 可以直接映射為 CLR 的 `valuetype`。
2. **泛型 (Generics)**: CLR 的泛型是特化（Specialization）的，運行時保留類型信息。這允許 Valkyrie 實現更高效的泛型代碼。
3. **尾調用 (Tail Call)**: CIL 顯式支持 `tail.` 前綴，非常適合函數式編程語言的優化。

## 4. Valkyrie 特性處理

### Trait 與接口
- **映射方案**: Valkyrie 的 `trait` 可以完美映射到 CLR 的 `interface`。
- **默認實現**: CLR 現在支持接口的默認方法實現，這與 Valkyrie 的 trait 默認實現契合。

### 代數效應 (Algebraic Effects)
- **核心邏輯**: Valkyrie 的 `raise` 和 `yield` 受到 AE 機制管控。
- **異常映射**: 在非 `resume` 情況下，`raise` 等價於 CLR 的 `throw` 指令。需要為不同的效應生成對應的 `Exception` 子類。
- **延續支持 (Continuations)**: 由於 CLR 不支持限定延續，對於 `resume` 場景，需要將函數重寫為狀態機（類似於 C# 的 `async` 或 `yield return`），將局部變量提升為類字段，並通過狀態碼管理執行流的恢復。

### FFI 與外部導入 (`@import`)
- **CLR 專用標記**: 針對 `target: clr` 的導入標記，直接映射到 .NET 程序集的元數據。
- **調用方式**: 使用 `call` 指令調用完全限定的方法名（如 `[mscorlib]System.Console::WriteLine`）。
- **獨立性**: CLR 後端的 FFI 路徑與 WASM/WASI 完全獨立，不需要考慮 WASM 目標的調用約定或 Marshalling 墊片。

### 內存管理
- **託管堆**: 直接利用 CLR 的高效分代 GC。
- **析構函數**: Valkyrie 的析構邏輯可以映射到 `IDisposable` 模式。

## 4. 實施進度與計劃

### 當前狀態 (Current Status)
已實現基於文本的 CIL (Common Intermediate Language) 指令發射，並集成了 `ilasm` 自動化編譯流程。
- [x] 基礎類型映射 (`int32`, `int64`, `bool`, `string` 等)
- [x] 結構體 (`struct`) 映射到 `valuetype`
- [x] 命名空間支持 (Namespace mapping)
- [x] 代數數據類型 (ADT) 基礎映射
- [x] 尾遞歸優化 (Tail Call Optimization)
- [x] 複雜投影路徑加載與存儲 (`ldfld`, `stfld`, `ldelema` 等)
- [x] `ilasm` 自動化集成與 EXE 生成
- [x] `main` 函數作為程序入口點 (`.entrypoint`)
- [x] ADT 變體構造函數調用 (Parameterized constructor for ADT)
- [ ] 完整代數效應支持 (Full AE support with state machine)
- [ ] CLR 專用 FFI 導入 (Target-specific FFI for CLR)

### 短期計劃 (Short-term Plan)
1. **AE 異常映射**: 實現非 resume 情況下的效應到異常的自動轉換。
2. **CLR FFI 驗證**: 實現 `@import(target: clr, ...)` 的後端支持，能夠成功調用 .NET BCL 方法。
3. **測試框架**: 完善集成測試，支持自動運行生成的 `.exe` 並比對輸出。
4. **數組與切片**: 深度測試數組和切片操作的邊界情況。

### 長期計劃 (Long-term Plan)
1. **泛型支持**: 利用 CLR 原生泛型實現 Valkyrie 泛型。
2. **異常與代數效應**: 探索 `try-catch` 與 Valkyrie 效應系統的映射。
3. **二進制生成**: 考慮引入 PE 生成器以擺脫對 `ilasm` 的依賴。
