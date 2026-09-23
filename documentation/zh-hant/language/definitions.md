# 定義

Valkyrie 提供了多種定義語法，用於聲明命名空間、變量、函數、類型和其他程式實體。

## 命名空間定義

Valkyrie 使用 `namespace` 或 `namespace!` 關鍵字聲明模組所屬的命名空間。

```valkyrie
# 顯式聲明命名空間
namespace! package.collection.option;

# 或者
namespace package.text;
```

## 變量定義

### 基本變量定義

```valkyrie
# 不可變變量
let name = "Alice"
let age = 30

# 可變變量
let mut counter = 0

# 顯式類型註解
let score: i32 = 95

# 延遲初始化
let result: i32
if condition {
    result = 42
} else {
    result = 0
}
```

## 函數定義 (micro)

Valkyrie 使用 `micro` 關鍵字定義函數。

### 基本函數定義

```valkyrie
# 無參數函數
micro greet() {
    print("Hello, World!")
}

# 帶參數函數
micro add(a: i32, b: i32) -> i32 {
    a + b
}
```

## 類型定義

Valkyrie 區分結構化數據（`class`）和代數數據類型（`unity`）。

### 類定義 (class)

```valkyrie
class Point {
    x: f64
    y: f64
}
```

### 聯合類型定義 (unity)

`unity` 用於定義和 Rust `enum` 類似的代數數據類型。

```valkyrie
unity Option⟨V⟩ {
    Some {
        value: V
    }
    None
}
```

## 實現定義 (imply)

Valkyrie 使用 `imply` 關鍵字為類型實現方法或 Trait。

```valkyrie
imply Option⸬Some {
    constructor(value: V) {
        this.value = value
    }
}

imply Unicode {
    # 實現方法
}
```

# 具名參數調用
let user = create_user(name: "Alice", active: false)
let result = sum(1, 2, 3, 4, 5)

# 引用參數
micro modify_array(arr: [i32]) {
    for i in 0..<arr.len() {
        arr[i] *= 2
    }
}

# 泛型參數
micro identity⟨T⟩(value: T) -> T {
    value
}

micro map⟨T, U⟩(items: [T], transform: micro(T) -> U) -> [U] {
    let mut result = []
    for item in items {
        result.push(transform(item))
    }
    result
}
```

### 高階函數

```valkyrie
# 函數作為參數
micro apply_operation(x: i32, y: i32, op: micro(i32, i32) -> i32) -> i32 {
    op(x, y)
}

# 返回函數
micro make_adder(n: i32) -> micro(i32) -> i32 {
    micro(x: i32) -> i32 {
        x + n
    }
}

# 閉包
let add_five = make_adder(5)
let result = add_five(10)  # 15

# 匿名函數
let numbers = [1, 2, 3, 4, 5]
let doubled = numbers.map(micro(x) { x * 2 })
let filtered = numbers.filter(micro(x) { x % 2 == 0 })
```
