# Witness Table

该模块定义 Witness Table 的核心数据结构。

## 术语区分

| 术语 | 范围 | 说明 |
| :--- | :--- | :--- |
| **witness table** | 本模块 / `trait`·`imply` | Valkyrie 动态派发；胖指针 `(data, witness_table)` |
| **COM vtable** | Windows FFI | `[com]` 互操作专用，不属于本模块 |
| **传统 OOP vtable** | 外部对比 | 文档对比用，非 Valkyrie 实现 |

## 概述

Witness Table 是 trait 实现的运行时表示，
用于动态方法分发。每个 `impl Trait for Type` 
都会生成一个 Witness Table。

## 数据结构

```text
struct WitnessTable {
    trait_id: Identifier,
    type_id: Identifier,
    methods: Vec<WitnessMethod>,
    associated_types: Vec<AssociatedType>,
}
```

## 方法分发

Witness Table 支持以下分发方式：

- **静态分发**: 编译时已知具体类型
- **动态分发**: 通过 witness table 运行时查找
