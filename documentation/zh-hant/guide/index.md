# Valkyrie 語言快速入門

歡迎使用 Valkyrie 語言！Valkyrie 是一個現代化的函數式編程語言，提供強大的類型系統、靈活的模塊系統和豐富的語言特性。

## 什麼是 Valkyrie？

Valkyrie 是一個多範式編程語言，它提供：

- 🎯 **強大的類型系統**：支持泛型、高階類型、類型推導等高級特性
- 🚀 **現代語法**：簡潔直觀的語法，支持模式匹配、閉包等現代特性
- 🔒 **內存安全**：垃圾回收器自動管理內存，避免內存泄漏
- ⚡ **高性能**：零成本抽象，編譯時優化
- 🔧 **靈活的模塊系統**：基於命名空間的模塊組織方式

## 基本語法

### 變量定義

```valkyrie
# 不可變變量
let name = "Alice"
let age = 30
let is_active = true

# 可變變量
let mut counter = 0
let mut items = []

# 顯式類型註解
let score: i32 = 95
let price: f64 = 29.99
let message: String = "Hello"
```

### 函數定義

```valkyrie
# 基本函數定義
micro greet() {
    print("Hello, World!")
}

# 帶參數和返回值的函數
micro add(a: i32, b: i32) -> i32 {
    a + b
}

# 多參數函數
micro calculate(x: f64, y: f64, operation: String) -> f64 {
    if operation == "add" {
        x + y
    } else if operation == "multiply" {
        x * y
    } else {
        0.0
    }
}
```

### 基本數據類型

```valkyrie
# 整數類型
let a: i32 = 42
let b: u64 = 100

# 浮點類型
let x: f32 = 3.14
let y: f64 = 2.718281828

# 布爾類型
let flag: bool = true

# 字符和字符串
let ch: char = 'A'
let text: String = "Hello, World!"

# 數組類型
let numbers: [i32; 5] = [1, 2, 3, 4, 5]
let dynamic: [String] = ["a", "b", "c"]

# 元組類型
let point: (f64, f64) = (3.0, 4.0)
let mixed: (String, i32, bool) = ("test", 42, true)
```

## 控制流

### 條件語句

```valkyrie
# if 語句
if x > 0 {
    print("正數")
} else {
    print("非正數")
}

# if 表達式
let result = if x > 0 { "positive" } else { "non-positive" }

# 多重條件
if score >= 90 {
    grade = "A"
} else if score >= 80 {
    grade = "B"
} else {
    grade = "F"
}
```

### 循環語句

```valkyrie
# while 循環
while counter < 10 {
    print(counter)
    counter = counter + 1
}

# for 循環
for i in 0..10 {
    print(i)
}

# 遍歷數組
for item in items {
    print(item)
}

# 無限循環
loop {
    if should_break {
        break
    }
}
```

## 模式匹配

```valkyrie
# 基本模式匹配
match value {
    case 1: "one"
    case 2: "two"
    case 3: "three"
    case _: "other"
}

# 範圍匹配
match score {
    case 90..=100: "A"
    case 80..=89: "B"
    case 70..=79: "C"
    case _: "F"
}

# 元組解構
match point {
    case (0, 0): "Origin"
    case (x, 0): "On X-axis at ${ x }"
    case (0, y): "On Y-axis at ${ y }"
    case (x, y): "Point at (${ x }, ${ y })"
}
```

## 類型定義

### 記錄類型

```valkyrie
# 基本記錄類型
type Point = {
    x: f64,
    y: f64,
}

# 泛型記錄類型
type Container<T> = {
    value: T,
    metadata: String,
}
```

### 聯合類型

```valkyrie
# 基本聯合類型
union Result<T, E> {
    Fine { value: T },
    Fail { error: E }
}

# 使用聯合類型
let result: Result<i32, String> = Fine { value: 42 }
match result {
    case Fine { value }: print("Success: ${ value }")
    case Fail { error }: print("Error: ${ error }")
}
```
