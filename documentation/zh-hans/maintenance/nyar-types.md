# Nyar Value 类型系统

`Value` 是 Nyar VM 中的核心数据交换格式，采用了高效的 **NaN-Boxing** 编码方案，将所有运行时类型压缩在 64 位（8 字节）的字宽内。

## 内存布局 (NaN-Boxing)

Nyar 使用 IEEE 754 标准中的非数 (NaN) 空间来编码非浮点类型。

### 1. 浮点数 (f64)
如果 64 位值不是 NaN（即指数部分不全是 1），则该值直接解释为标准 IEEE 754 双精度浮点数。
- **判断条件**: `(bits & 0x7FF8_0000_0000_0000) != 0x7FF8_0000_0000_0000`

### 2. 标记值 (Tagged Values)
如果该值是一个特定的 NaN 格式，则剩余的 51 位被划分为 **Tag (类型标记)** 和 **Payload (负载)**。

- **NaN Base**: `0x7FF8_0000_0000_0000`
- **Tag (4 bits)**: 位于第 47-50 位。
- **Payload (47 bits)**: 位于低 47 位，足以存储 128TB 的寻址空间（在现代 64 位系统上通常只使用 48 位地址空间，且用户空间地址通常在高位为 0，因此 47 位通常足够）。

| 位范围 | 含义 | 说明 |
| :--- | :--- | :--- |
| 63 | 符号位 | 固定为 0 (NaN 空间) |
| 62-51 | 指数位 | 固定为 `0x7FF` (NaN 空间) |
| 50-47 | **Tag** | 定义 `ValueTag` 类型 |
| 46-0 | **Payload** | 存储整数值、布尔值或 GC 指针 |

## 类型定义 (ValueTag)

Nyar 支持丰富的内置类型，通过 `ValueTag` 进行区分：

| Tag | 类型名称 | 描述 | 负载内容 |
| :--- | :--- | :--- | :--- |
| 0 | `Int` | 47 位有符号整数 | 补码数值 |
| 1 | `Bool` | 布尔值 | 0 或 1 |
| 2 | `Null` | 空值 | 0 |
| 3 | `String` | 字符串 | `GcBox<String>` 指针 |
| 4 | `Array` | 固定长度数组 | `GcBox<Array>` 指针 |
| 5 | `BigInt` | 任意精度整数 | `GcBox<BigInt>` 指针 |
| 6 | `Object` | 静态对象/结构体 | `GcBox<Object>` 指针 |
| 7 | `Closure` | 闭包 | `GcBox<Closure>` 指针 |
| 8 | `DynObject` | 动态对象/映射 | `GcBox<DynObject>` 指针 |
| 9 | `List` | 动态列表 (Vector) | `GcBox<List>` 指针 |
| 10 | `Tuple` | 元组 | `GcBox<Tuple>` 指针 |
| 11 | `Continuation`| 延续 (协程状态) | `GcBox<Continuation>` 指针 |
| 12 | `Effect` | 效应实例 | `GcBox<Effect>` 指针 |
| 13 | `Code` | 代码对象 | 内部指针/索引 |
| 14 | `WitnessTable`| 类型见证表 (VTable) | 指针 |
| 15 | `Float` | 浮点数 (逻辑标记) | 仅用于统一接口 |
| 16 | `Function` | 函数指针 | 原始指针/索引 |
| 17 | `TraitObject` | 特征对象 | `(Pointer, WitnessTable)` |

## 垃圾回收 (GC) 集成

所有堆分配类型（String, Array, Object 等）在 Payload 中存储的都是指向 `GcBox<T>` 的指针。

- **追踪 (Trace)**: GC 扫描器通过检查 `ValueTag` 来确定 Payload 是否为指针。
- **标记 (Mark)**: 如果是堆对象，则递归调用 `GcHeader::mark`。
- **不变性**: `Value` 本身是 `Copy` 的，仅作为引用的持有者。

## 设计优势

1. **缓存友好**: 8 字节字宽可以直接放入寄存器，在栈上传递开销极小。
2. **无需分支的浮点运算**: 浮点数处理路径极短，适合科学计算场景。
3. **单态化优化**: JIT 可以根据 `ValueTag` 快速生成特化代码（Guard Elimination）。
4. **内存紧凑**: 相比于 `enum` 带来的内存对齐开销（通常需要 16 字节），NaN-Boxing 节省了 50% 的内存空间。
