# Valkyrie 語言特性指南

## 概述

Valkyrie 是一門現代編程語言，旨在提供強大的表達能力和極致的性能。本指南詳細介紹 Valkyrie 語言的核心特性和高級功能，幫助開發者深入理解和使用這門語言。

## 編譯器架構

Valkyrie 的編譯器採用了現代化的設計，提供：
- 極致優化的增量編譯基礎設施
- 多層中間表示（AST → HIR → MIR → LIR）
- 可插拔的代碼生成後端
- 統一的錯誤處理框架
- 完善的語言服務器協議 (LSP) 支持

## 核心語言特性

### 1. 強大的類型系統

#### 基本類型

```valkyrie
# 原始類型
let integer: i32 = 42
let float: f64 = 3.14159
let boolean: bool = true
let character: char = 'A'
let text: String = "Hello, World!"

# 複合類型
let numbers: [i32; 5] = [1, 2, 3, 4, 5]
let point: (f64, f64) = (3.0, 4.0)
let maybe_value: Option<i32> = Some(42)
```

#### 泛型和類型參數

```valkyrie
# 泛型函數
micro identity<T>(value: T) -> T {
    value
}

# 泛型類型
type Container<T> = {
    value: T,
    metadata: String,
}

# 約束泛型
micro compare<T>(a: T, b: T) -> bool
where T: PartialEq
{
    a == b
}
```

#### 高階類型 (HKT)

```valkyrie
# 類型構造器
type Functor<F> = {
    map: micro<A, B>(F<A>, micro(A) -> B) -> F<B>
}

# 單子模式
type Monad<M> = {
    pure: micro<A>(A) -> M<A>,
    bind: micro<A, B>(M<A>, micro(A) -> M<B>) -> M<B>
}

# Option 單子實現
impl Monad<Option> {
    pure(value) { Some { value } }
    
    bind(opt, f) {
        match opt {
            with [monad_bind];
            case Some(value): f(value)
            case None: None
        }
    }
}
```

#### 類型函數 (mezzo)

```valkyrie
# 編譯時類型計算
mezzo Add<A, B>(a: A, b: B) -> Type {
    # 類型級加法
    match (a, b) {
        (Zero, n) => n,
        (Succ<m>, n) => Succ<Add<m, n>>
    }
}

# 條件類型選擇
mezzo If<Condition, Then, Else>(cond: Condition) -> Type {
    if cond {
        Then
    } else {
        Else
    }
}

# 類型驗證
mezzo IsNumeric<T>(t: T) -> bool {
    match t {
        i8 | i16 | i32 | i64 | i128 => true,
        u8 | u16 | u32 | u64 | u128 => true,
        f32 | f64 => true,
        _ => false
    }
}
```

### 2. 模式匹配系統

#### 基本模式匹配

```valkyrie
# 值匹配
match value {
    with [basic_match];
    case 0: "zero"
    case 1: "one"
    case 2: "two"
    else: "other"
}

# 範圍匹配
match score {
    with [grade_calculation];
    case 90..=100: "A"
    case 80..=89: "B"
    case 70..=79: "C"
    case 60..=69: "D"
    else: "F"
}

# 多值匹配
match day {
    with [weekend_check];
    case "Saturday" | "Sunday": "Weekend"
    case "Monday"..="Friday": "Weekday"
    else: "Invalid"
}
```

#### 解構匹配

```valkyrie
# 元組解構
match point {
    with [coordinate_analysis];
    case (0, 0): "Origin"
    case (x, 0): f"On X-axis at {x}"
    case (0, y): f"On Y-axis at {y}"
    case (x, y) if x == y: f"Diagonal at {x}"
    case (x, y): f"Point at ({x}, {y})"
}

# 數組解構
match array {
    with [array_pattern];
    case []: "Empty"
    case [x]: f"Single element: {x}"
    case [first, ..rest]: f"First: {first}, Rest: {rest.len()} items"
}

# 對象解構
match person {
    with [person_info];
    case { name: "Alice", age }: f"Alice is {age} years old"
    case { name, age: 18..=25 }: f"Young adult: ${name}"
    case { name, age }: f"{name} is {age} years old"
}
```

#### 聯合類型匹配

```valkyrie
# Result 類型匹配
match result {
    with [result_handling];
    case Fine { value }: f"Success: {value}"
    case Fail { error }: f"Error: {error}"
}

# 複雜聯合類型
union Expression {
    Literal { value: i32 },
    Variable { name: String },
    Binary {
        left: Expression,
        operator: String,
        right: Expression,
    },
}

match expr {
    with [expression_evaluation];
    case Literal(value): value
    case Variable(name): lookup_variable(name)
    case Binary { left, operator: "+", right }: {
        evaluate(left) + evaluate(right)
    }
    case Binary { left, operator: "*", right }: {
        evaluate(left) * evaluate(right)
    }
    else: 0
}
```

### 3. 函數式編程特性

#### 高階函數

```valkyrie
# 函數作為參數
micro apply_twice<T>(f: micro(T) -> T, value: T) -> T {
    f(f(value))
}

# 函數組合
micro compose<A, B, C>(f: micro(B) -> C, g: micro(A) -> B) -> micro(A) -> C {
    micro(x) { f(g(x)) }
}

# 柯里化
micro add(x: i32) -> micro(i32) -> i32 {
    micro(y) { x + y }
}

let add_five = add(5)
let result = add_five(10)  # 結果為 15
```

#### 閉包和 Lambda 表達式

```valkyrie
# 基本閉包
let square = micro(x) { x * x }
let add = micro(x, y) { x + y }

# 捕獲外部變量
let multiplier = 3
let multiply_by_three = micro(x) { x * multiplier }

# 複雜閉包
let process_data = micro(data) {
    let cleaned = data.filter { $item.is_valid() }
    let transformed = cleaned.map { $item.transform() }
    transformed.reduce { $acc + $item }
}

# 尾隨閉包語法
numbers.map { $x * $x }
    .filter { $x > 10 }
    .reduce { $acc + $x }
```

#### 遞歸和尾遞歸優化

```valkyrie
# 普通遞歸
micro factorial(n: i32) -> i32 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

# 尾遞歸優化
micro factorial_tail(n: i32, acc: i32 = 1) -> i32 {
    if n <= 1 {
        acc
    } else {
        factorial_tail(n - 1, n * acc)
    }
}

# 相互遞歸
micro is_even(n: i32) -> bool {
    if n == 0 {
        true
    } else {
        is_odd(n - 1)
    }
}

micro is_odd(n: i32) -> bool {
    if n == 0 {
        false
    } else {
        is_even(n - 1)
    }
}
```

### 4. 面向對象編程

#### 類定義和繼承

```valkyrie
# 基本類定義
class Animal {
    name: String
    age: i32
    
    new(name: String, age: i32) -> Self {
        Self { name, age }
    }
    
    speak(self) {
        print("${self.name} makes a sound")
    }
    
    get_info(self) -> String {
        "${self.name} is ${self.age} years old"
    }
}

# 繼承
class Dog extends Animal {
    breed: String
    
    new(name: String, age: i32, breed: String) -> Self {
        Self {
            name,
            age,
            breed
        }
    }
    
    speak(self) {
        print("${self.name} barks: Woof!")
    }
    
    fetch(self, item: String) {
        print("${self.name} fetches the ${item}")
    }
}
```

#### 特徵 (Traits) 和實現

```valkyrie
$ 特徵定義
trait Drawable {
    draw(self)
    get_area(self) -> f64
}

trait Comparable<T> {
    compare(self, other: T) -> i32
}

$ 特徵實現
class Circle {
    radius: f64
    
    new(radius: f64) -> Self {
        Self { radius }
    }
}

impl Drawable for Circle {
    draw(self) {
        print("Drawing circle with radius ${self.radius}")
    }
    
    get_area(self) -> f64 {
        3.14159 * self.radius * self.radius
    }
}

impl Comparable<Circle> for Circle {
    compare(self, other: Circle) -> i32 {
        if self.radius < other.radius {
            -1
        } else if self.radius > other.radius {
            1
        } else {
            0
        }
    }
}
```

### 5. 模塊系統

#### 命名空間組織

```valkyrie
# 基本命名空間
namespace math {
    let PI = 3.14159
    
    micro sin(x: f64) -> f64 {
        # 正弦函數實現
        x  # 簡化實現
    }
    
    micro cos(x: f64) -> f64 {
        # 餘弦函數實現
        1.0 - x * x / 2.0  # 簡化實現
    }
}

# 嵌套命名空間
namespace graphics {
    namespace shapes {
        class Rectangle {
            width: f64
            height: f64
            
            new(width: f64, height: f64) -> Self {
                Self { width, height }
            }
            
            area(self) -> f64 {
                self.width * self.height
            }
        }
    }
    
    namespace colors {
        class RGB {
            r: u8
            g: u8
            b: u8
        }
        
        let RED: RGB = class { r: 255, g: 0, b: 0 }
let GREEN: RGB = class { r: 0, g: 255, b: 0 }
let BLUE: RGB = class { r: 0, g: 0, b: 255 }
    }
}
```

#### 導入和使用

```valkyrie
# 完整導入
using math.*

micro calculate_circle_area(radius: f64) -> f64 {
    math.PI * radius * radius
}

# 選擇性導入
using math.{PI, sin, cos}
using graphics.shapes.Rectangle
using graphics.colors.{RED, GREEN, BLUE}

# 重命名導入
using graphics.shapes.Rectangle as Rect
using graphics.colors.RGB as Color

# 使用導入的內容
micro create_colored_rectangle() -> (Rect, Color) {
    let rect = Rect::new(10.0, 20.0)
    let color = RED
    (rect, color)
}
```

### 6. 控制流

#### 條件控制

```valkyrie
# 基本條件
if condition {
    # 執行代碼
} else if other_condition {
    # 其他條件
} else {
    # 默認情況
}

# 條件表達式
let result = if x > 0 {
    "positive"
} else if x < 0 {
    "negative"
} else {
    "zero"
}

# 守衛條件
if let Some { value } = optional_value {
    print(f"Got value: {value}")
}
```

#### 循環控制

```valkyrie
# while 循環
while condition {
    # 循環體
    if should_break {
        break
    }
    if should_continue {
        continue
    }
}

# for 循環
for i in 0..10 {
    print(i)
}

for item in collection {
    process(item)
}

for (index, value) in collection.enumerate() {
    print(f"Index {index}: {value}")
}

# 無限循環
loop {
    let input = get_input()
    if input == "quit" {
        break
    }
    process(input)
}

# 帶標籤的循環
'outer: loop {
    'inner: for i in 0..10 {
        if should_break_outer {
            break 'outer
        }
        if should_continue_inner {
            continue 'inner
        }
    }
}
```

### 7. 錯誤處理

#### 異常系統

```valkyrie
# 拋出異常
micro validate_age(age: i32) {
    if age < 0 {
        raise "Age cannot be negative"
    }
    if age > 150 {
        raise "Age seems unrealistic"
    }
}

# 捕獲異常
try {
    validate_age(-5)
    risky_operation()
}
.catch {
    case error: String:
        print(f"String error: {error}")
    case error: NetworkError:
        print(f"Network error: {error.message}")
        retry_connection()
    case error:
        print(f"Unknown error: {error}")
}
```

#### Result 類型

```valkyrie
# 使用 Result 類型
micro divide(a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 {
        Err { error: "Division by zero" }
    } else {
        Fine { value: a / b }
    }
}

# 鏈式錯誤處理
let result = divide(10.0, 2.0)
    .map { $value * 2.0 }
    .and_then { $value -> 
        if $value > 100.0 {
            Err { error: "Value too large" }
        } else {
            Ok { value: $value }
        }
    }

match result {
    with [error_handling];
    case Fine { value }: print(f"Result: {value}")
    case Fail { error }: print(f"Error: {error}")
}
```

### 8. 元編程

#### 宏系統

```valkyrie
# 簡單宏定義
macro debug_print($expr) {
    @cfg(debug_assertions)
    print("DEBUG: {} = {}", stringify!($expr), $expr)
}

# 使用宏
debug_print!(x + y)
# 展開為: print("DEBUG: x + y = {}", x + y)

# 複雜宏
macro create_class($name, $($field:$type),*) {
    class $name {
        $($field: $type,)*
        
        new($($field: $type),*) -> Self {
            Self {
                $($field,)*
            }
        }
    }
}

# 使用複雜宏
create_class!(Person, name: String, age: i32)
```

#### 編譯時計算

```valkyrie
# 編譯時常量
@const_eval
micro fibonacci_const(n: i32) -> i32 {
    if n <= 1 {
        n
    } else {
        fibonacci_const(n - 1) + fibonacci_const(n - 2)
    }
}

let fib_10 = fibonacci_const(10)  # 編譯時計算

# 編譯時類型生成
@derive(Debug, Clone, PartialEq)
class Point {
    x: f64
    y: f64
}
```

## 高級特性

### 1. 內存管理

```valkyrie
# 垃圾回收
let data = allocate_large_data()  # 自動管理內存
# 當 data 離開作用域時自動回收

# 引用計數
let shared_data = Rc::new(expensive_data())
let reference1 = shared_data.clone()
let reference2 = shared_data.clone()
# 當所有引用都離開作用域時自動釋放
```

### 2. 並發和異步

```valkyrie
# 異步函數
async micro fetch_data(url: String) -> Result<String, Error> {
    let response = http_client.get(url).await?
    Fine { value: response.text().await? }
}

# 並發執行
async micro process_urls(urls: [String]) -> [Result<String, Error>] {
    let futures = urls.map { $url -> fetch_data($url) }
    futures.join_all().await
}
```

### 3. 性能優化

```valkyrie
# 內聯優化
@inline
micro fast_add(a: i32, b: i32) -> i32 {
    a + b
}

# 特化優化
@specialize
micro generic_sort<T>(data: [T]) -> [T]
where T: Ord
{
    # 為每個具體類型生成優化版本
    data.sort()
}

# 零成本抽象
let result = numbers
    .iter()
    .map { $x * $x }
    .filter { $x > 100 }
    .collect()
# 編譯後等價於手寫循環
```

## 總結

Valkyrie 語言提供了豐富的特性集合：

1. **類型安全**: 強大的靜態類型系統，編譯時捕獲錯誤
2. **表達能力**: 模式匹配、高階類型、類型函數等高級特性
3. **函數式**: 高階函數、閉包、不可變性等函數式編程特性
4. **面向對象**: 類、繼承、特徵等面向對象編程支持
5. **模塊化**: 靈活的命名空間和導入系統
6. **元編程**: 宏系統和編譯時計算
7. **性能**: 零成本抽象和編譯時優化
8. **安全**: 內存安全和錯誤處理機制

這些特性使得 Valkyrie 能夠適應各種編程場景，從系統編程到應用開發，從函數式編程到面向對象編程，都能提供優秀的開發體驗和運行時性能。
