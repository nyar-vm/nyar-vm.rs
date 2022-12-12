# JVM 後端維護指南

JVM 後端負責將 Valkyrie 編譯為 Java 類文件格式。

## 編譯流水線

`Source -> AST -> HIR -> CFG -> JVM Bytecode`

## 設計考量

### 棧機架構
JVM 是一個基於棧的虛擬機。使用基於寄存器的 LIR 會引入冗餘的 `iload`/`istore` 指令。因此，JVM 後端跳過了 SSA 和 LIR 階段，直接從 **CFG** (控制流圖) 生成代碼。

### 線性化
CFG 的基本塊被線性化為扁平的指令流。跳轉指令（`Goto`, `SwitchInt`）通過在第二遍掃描（標籤修復）期間計算相對偏移量來處理。

### 類型系統
Valkyrie 類型映射到 JVM 描述符：
- `Int32` -> `I`
- `Int64` -> `J`
- `Float32` -> `F`
- `Float64` -> `D`
- `String` -> `Ljava/lang/String;`
- `Bool` -> `Z`
- `Unit` -> `V`
- `Struct/Enum` -> `Lpath/to/Class;`
- `Pointer` -> `J` (目前暫定映射為 64 位整數)

### 表達式生成
- **算術運算**：直接映射到 `iadd`, `ladd`, `fadd`, `dadd` 以及 `irem`, `lrem`, `frem`, `drem` 等指令。
- **比較運算**：
    - 對於 `Int32`：使用 `if_icmp<cond>` 跳轉並推送 0 或 1。
    - 對於 `Int64/Float/Double`：使用 `lcmp/fcmpl/dcmpl` 指令，隨後配合 `if<cond>` 指令生成布爾值。
    - 對於 `Object` 類型（Struct, Enum, String）：使用 `if_acmp<cond>` 實現引用相等性比較。
- **位運算與邏輯運算**：
    - `And`, `Or`, `Xor` 映射到 `iand/land`, `ior/lor`, `ixor/lxor`。
    - `Shl`, `Shr` 映射到 `ishl/lshl`, `ishr/lshr`。
- **一元運算**：
    - `Neg` 映射到 `ineg/lneg/fneg/dneg`。
    - `Not`：對於 `Bool` 和 `Int32` 使用 `iconst_1/iconst_m1` 與 `ixor` 實現；對於 `Int64` 使用常量池中的 `-1L` 與 `lxor` 實現。

### 方法與調用
- 每個 Valkyrie 函數都被發射為類中的一個 `static` 方法。
- **名稱重整 (Mangling)**：
    - 全局函數保持原名。
    - 結構體/枚舉方法：`ClassName$MethodName`。
    - Trait 方法：`TraitName$MethodName`。
    - Impl 方法：`[TraitName$]TargetName_MethodName`。
- 方法調用（`Call`）目前使用 `invokestatic`。後端會自動從 `CfgProgram` 中檢索被調用者的簽名以生成正確的描述符。
- 對於動態調用或函數指針，後端利用 `java/lang/invoke/MethodHandle` 的 `invoke` 方法實現。

### 泛型支持
- **類型擦除**：所有泛型類型在描述符中被擦除為 `Ljava/lang/Object;`。
- **Signature 屬性**：為了保留泛型信息，後端會為泛型函數和字段生成 JVM `Signature` 屬性。
- **函數類型**：Valkyrie 的函數類型映射為 `Ljava/lang/invoke/MethodHandle;`。

### 代數效應 (Algebraic Effects)
- Valkyrie 的 `raise` 被視為一種 Effect，受 AE 機制管控。
- **Raise 實現**：在非 resume 情況下等價於 Java 異常，映射為 `athrow` 指令。
- **處理器 (Handlers)**：通過 JVM `Code` 屬性中的 `exception_table` 實現 `PushHandler`/`PopHandler`。
- 進入 Handler 塊時，後端會自動將 `current_stack` 初始化為 1 以匹配 JVM 規範（Effect 對象會被推入棧頂）。
- **恢復 (Resume)**：目前僅支持非恢復路徑，完整 Continuation 支持計劃在未來通過棧幀捕獲或字節碼重寫實現。

### 控制流優化
- **跳轉指令**：
    - 默認使用 `goto_w` (4 字節偏移量) 以支持超大基本塊的跳轉。
    - `SwitchInt` 會根據跳轉範圍和稀疏程度自動選擇 `tableswitch` 或 `lookupswitch`，且均使用 4 字節偏移量。

## 當前進度

- [x] 基本 Class 文件結構生成
- [x] 常量池管理 (UTF8, Integer, Float, Long, Double, Class, String, FieldRef, MethodRef, NameAndType, MethodHandle, MethodType)
- [x] 基本類型映射 (Bool, Int32, Int64, Float32, Float64, String, Unit)
- [x] 算術運算 (Add, Sub, Mul, Div, Rem) 支持 Int32, Int64, Float32, Float64
- [x] 位運算與邏輯運算 (And, Or, Xor, Shl, Shr)
- [x] 一元運算 (Neg, Not)
- [x] 比較運算 (Eq, Ne, Lt, Le, Gt, Ge) 支持 Int32, Int64, Float32, Float64
- [x] 結構體與枚舉的基礎實例化與字段訪問 (限 1 層)
- [x] 數組基礎支持 (newarray, anewarray, iastore/iaload 等)
- [x] 靜態方法調用 (invokestatic)
- [x] 代數效應基礎 (Raise, Handlers)
- [x] 泛型 Signature 基礎支持

## 路線圖 (Roadmap)

### 短期目標 (Short-term)
1. **代碼重構**：重構 `emitter.rs` 以減少字節碼發射邏輯的冗餘。
2. **調試支持**：實現 `LineNumberTable` 屬性，支持源碼行號映射。
3. **字段訪問增強**：支持多層深度的字段/索引訪問。
4. **方法調用完善**：支持 `invokevirtual` 和 `invokeinterface`。

### 中期目標 (Mid-term)
1. **閉包支持**：利用 `invokedynamic` 實現 Lambda 和閉包。
2. **性能優化**：實現更智能的棧平衡優化，減少不必要的 `dup` 和 `pop`。
3. **反射與內省**：支持 Valkyrie 運行時反射。

### 長期目標 (Long-term)
1. **增量編譯**：支持基於類文件的增量編譯。
2. **原生互操作**：優化與 Java 標準庫的交互性能。
