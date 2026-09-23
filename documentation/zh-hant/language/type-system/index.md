# 型別系統

Valkyrie 提供了一個強大而靈活的型別系統，支援靜態型別檢查、型別推導和進階型別特性。

## 基本型別

### 原始型別

```valkyrie
# 整數型別
let a: i32 = 42
let b: u64 = 100
let c: isize = -1

# 浮點型別
let x: f32 = 3.14
let y: f64 = 2.718281828

# 布林型別
let flag: bool = true

# 字元與字串
let ch: char = 'A'
let text: string = "Hello, World!"
```

### 複合型別

```valkyrie
# 陣列型別
let numbers: [i32; 5] = [1, 2, 3, 4, 5]
let dynamic: [string] = ["a", "b", "c"]

# 元組型別
let point: (f64, f64) = (3.0, 4.0)
let mixed: (string, i32, bool) = ("test", 42, true)

# 可選型別
let maybe_value: i32? = 42
let empty: string? = None
```

## 複合型別定義

### 記錄型別

```valkyrie
# 基本記錄型別
type Point = {
    x: f64,
    y: f64,
}

# 泛型記錄型別
type Container⟨T⟩ = {
    value: T,
    metadata: string,
}

# 嵌套記錄型別
type Person = {
    name: string,
    age: i32,
    address: {
        street: string,
        city: string,
    },
}
```

### 聯合型別

```valkyrie
# 基本聯合型別
unity Result⟨T, E⟩ {
    Fine(T),
    Fail(E)
}

# 複雜聯合型別
unity Expression {
    Literal(i32),
    Variable(string),
    Binary {
        left: Expression,
        operator: string,
        right: Expression,
    }
}
```

## 特徵與實現

### 特徵定義

```valkyrie
# 基本特徵
trait Display {
    micro fmt(self) -> string
}
```
