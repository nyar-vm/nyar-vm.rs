# Valkyrie 语言特性指南

## 概述

Valkyrie 是一门现代编程语言，旨在提供强大的表达能力和极致的性能。本指南详细介绍 Valkyrie 语言的核心特性和高级功能，帮助开发者深入理解和使用这门语言。

## 编译器架构

Valkyrie 的编译器采用了现代化的设计，提供：
- 极致优化的增量编译基础设施
- 多层中间表示（AST → HIR → MIR → LIR）
- 可插拔的代码生成后端
- 统一的错误处理框架
- 完善的语言服务器协议 (LSP) 支持

## 核心语言特性

### 1. 强大的类型系统

#### 基本类型

```valkyrie
# 原始类型
let integer: i32 = 42
let float: f64 = 3.14159
let boolean: bool = true
let character: char = 'A'
let text: string = "Hello, World!"

# 复合类型
let numbers: [i32; 5] = [1, 2, 3, 4, 5]
let point: (f64, f64) = (3.0, 4.0)
let maybe_value: Option⟨i32⟩ = Some(42)
```

#### 泛型和类型参数

```valkyrie
# 泛型函数
micro identity⟨T⟩(value: T) -> T {
    value
}

# 泛型类型
type Container⟨T⟩ = {
    value: T,
    metadata: string,
}

# 约束泛型
micro compare⟨T⟩(a: T, b: T) -> bool
where T: PartialEq
{
    a == b
}
```

#### 高阶类型 (HKT)

```valkyrie
# 类型构造器
type Functor⟨F⟩ = {
    map: micro⟨A, B⟩(F⟨A⟩, micro(A) -> B) -> F⟨B⟩
}

# 单子模式
type Monad⟨M⟩ = {
    pure: micro⟨A⟩(A) -> M⟨A⟩,
    bind: micro⟨A, B⟩(M⟨A⟩, micro(A) -> M⟨B⟩) -> M⟨B⟩
}

# Option 单子实现
imply Monad⟨Option⟩ {
    pure(value) { Some(value) }
    
    bind(opt, f) {
        match opt {
            case Some(value): f(value)
            case None: None
        }
    }
}
```

#### 类型函数 (mezzo)

```valkyrie
# 编译时类型计算
mezzo Add⟨A, B⟩(a: A, b: B) -> type {
    # 类型级加法
    match (a, b) {
        (Zero, n) => n,
        (Succ⟨m⟩, n) => Succ⟨Add⟨m, n⟩⟩
    }
}

# 条件类型选择
mezzo If⟨Condition, Then, Else⟩(cond: Condition) -> type {
    if cond {
        Then
    } else {
        Else
    }
}

# 类型验证
mezzo IsNumeric⟨T⟩(t: T) -> bool {
    match t {
        i8 | i16 | i32 | i64 | i128 => true,
        u8 | u16 | u32 | u64 | u128 => true,
        f32 | f64 => true,
        _ => false
    }
}
```

### 2. 模式匹配系统

#### 基本模式匹配

```valkyrie
# 值匹配
match value {
    case 0: "zero"
    case 1: "one"
    case 2: "two"
    case _: "other"
}

# 范围匹配
match score {
    case 90..=100: "A"
    case 80..=89: "B"
    case 70..=79: "C"
    case 60..=69: "D"
    case _: "F"
}

# 多值匹配
match day {
    case "Saturday" | "Sunday": "Weekend"
    case "Monday"..="Friday": "Weekday"
    case _: "Invalid"
}
```

#### 解构匹配

```valkyrie
# 1. 元组解构
match point {
    case (x, 0): "On X-axis at {x}"
    case (0, y): "On Y-axis at {y}"
    case (x, y) if x == y: "Diagonal at {x}"
    case (x, y): "Point at ({x}, {y})"
}

# 2. 数组解构
match numbers {
    case []: "Empty list"
    case [x]: "Single element: {x}"
    case [first, ..rest]: "First: {first}, Rest: {rest.len()} items"
}

# 3. 对象解构
match user {
    case { name: "Alice", age }: "Alice is {age} years old"
    case { name, age: 18..=25 }: "Young adult: {name}"
    case { name, age }: "{name} is {age} years old"
}
```

#### 联合类型匹配

```valkyrie
# 4. Result 类型匹配
match result {
    case Fine(value): "Success: {value}"
    case Fail(error): "Error: {error}"
}

# 复杂联合类型
unity Expression {
    Literal(i32),
    Variable(string),
    Binary {
        left: Expression,
        operator: string,
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

### 3. 函数式编程特性

#### 高阶函数

```valkyrie
# 函数作为参数
micro apply_twice<T>(f: micro(T) -> T, value: T) -> T {
    f(f(value))
}

# 函数组合
micro compose<A, B, C>(f: micro(B) -> C, g: micro(A) -> B) -> micro(A) -> C {
    micro(x) { f(g(x)) }
}

# 柯里化
micro add(x: i32) -> micro(i32) -> i32 {
    micro(y) { x + y }
}

let add_five = add(5)
let result = add_five(10)  # 结果为 15
```

#### 闭包和 Lambda 表达式

```valkyrie
# 基本闭包
let square = micro(x) { x * x }
let add = micro(x, y) { x + y }

# 捕获外部变量
let multiplier = 3
let multiply_by_three = micro(x) { x * multiplier }

# 复杂闭包
let process_data = micro(data) {
    let cleaned = data.filter { $item.is_valid() }
    let transformed = cleaned.map { $item.transform() }
    transformed.reduce { $acc + $item }
}

# 尾随闭包语法
numbers.map { $x * $x }
    .filter { $x > 10 }
    .reduce { $acc + $x }
```

#### 递归和尾递归优化

```valkyrie
# 普通递归
micro factorial(n: i32) -> i32 {
    if n <= 1 {
        1
    } else {
        n * factorial(n - 1)
    }
}

# 尾递归优化
micro factorial_tail(n: i32, acc: i32 = 1) -> i32 {
    if n <= 1 {
        acc
    } else {
        factorial_tail(n - 1, n * acc)
    }
}

# 相互递归
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

### 4. 面向对象编程

#### 类定义和继承

```valkyrie
# 基本类定义
class Animal {
    name: String
    age: i32
    
    new(name: String, age: i32) -> Self {
        Self { name, age }
    }
    
    speak(self) {
        print("{self.name} makes a sound")
    }
    
    get_info(self) -> String {
        "{self.name} is {self.age} years old"
    }
}

# 继承
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
        print("{self.name} barks: Woof!")
    }
    
    fetch(self, item: String) {
        print("{self.name} fetches the {item}")
    }
}
```

#### 特征 (Traits) 和实现

```valkyrie
$ 特征定义
trait Drawable {
    draw(self)
    get_area(self) -> f64
}

trait Comparable<T> {
    compare(self, other: T) -> i32
}

$ 特征实现
class Circle {
    radius: f64
    
    new(radius: f64) -> Self {
        Self { radius }
    }
}

impl Drawable for Circle {
    draw(self) {
        print("Drawing circle with radius {self.radius}")
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

### 5. 模块系统

#### 命名空间组织

```valkyrie
# 基本命名空间
namespace math {
    let PI = 3.14159
    
    micro sin(x: f64) -> f64 {
        # 正弦函数实现
        x  # 简化实现
    }
    
    micro cos(x: f64) -> f64 {
        # 余弦函数实现
        1.0 - x * x / 2.0  # 简化实现
    }
}

# 嵌套命名空间
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

#### 导入和使用

```valkyrie
# 完整导入
using math.*

micro calculate_circle_area(radius: f64) -> f64 {
    math.PI * radius * radius
}

# 选择性导入
using math.{PI, sin, cos}
using graphics.shapes.Rectangle
using graphics.colors.{RED, GREEN, BLUE}

# 重命名导入
using graphics.shapes.Rectangle as Rect
using graphics.colors.RGB as Color

# 使用导入的内容
micro create_colored_rectangle() -> (Rect, Color) {
    let rect = Rect::new(10.0, 20.0)
    let color = RED
    (rect, color)
}
```

### 6. 控制流

#### 条件控制

```valkyrie
# 基本条件
if condition {
    # 执行代码
} else if other_condition {
    # 其他条件
} else {
    # 默认情况
}

# 条件表达式
let result = if x > 0 {
    "positive"
} else if x < 0 {
    "negative"
} else {
    "zero"
}

# 守卫条件
if let Some { value } = optional_value {
    print("Got value: {value}")
}
```

#### 循环控制

```valkyrie
# while 循环
while condition {
    # 循环体
    if should_break {
        break
    }
    if should_continue {
        continue
    }
}

# for 循环
for i in 0..10 {
    print(i)
}

for item in collection {
    process(item)
}

for (index, value) in collection.enumerate() {
    print("Index {index}: {value}")
}

# 无限循环
loop {
    let input = get_input()
    if input == "quit" {
        break
    }
    process(input)
}

# 带标签的循环
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

### 7. 错误处理

#### 异常系统

```valkyrie
# 抛出异常
micro validate_age(age: i32) {
    if age < 0 {
        raise "Age cannot be negative"
    }
    if age > 150 {
        raise "Age seems unrealistic"
    }
}

# 捕获异常
try {
    validate_age(-5)
    risky_operation()
}
.catch {
    case error: String:
        print("String error: {error}")
    case error: NetworkError:
        print("Network error: {error.message}")
        retry_connection()
    case error:
        print("Unknown error: {error}")
}
```

#### Result 类型

```valkyrie
# 使用 Result 类型
micro divide(a: f64, b: f64) -> Result<f64, String> {
    if b == 0.0 {
        Err { error: "Division by zero" }
    } else {
        Fine { value: a / b }
    }
}

# 链式错误处理
let result = divide(10.0, 2.0)
    .map { %1 * 2.0 }
    .and_then { value -> 
        if value > 100.0 {
            Fail { error: "Value too large" }
        } else {
            Fine { value: value }
        }
    }

match result {
    case Fine { value }: print("Result: {value}")
    case Fail { error }: print("Error: {error}")
}
```

### 8. 元编程

#### 宏系统

```valkyrie
# 简单宏定义
macro debug_print($expr) {
    @cfg(debug_assertions)
    print("DEBUG: {} = {}", stringify!($expr), $expr)
}

# 使用宏
debug_print!(x + y)
# 展开为: print("DEBUG: x + y = {}", x + y)

# 复杂宏
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

# 使用复杂宏
create_class!(Person, name: String, age: i32)
```

#### 编译时计算

```valkyrie
# 编译时常量
@const_eval
micro fibonacci_const(n: i32) -> i32 {
    if n <= 1 {
        n
    } else {
        fibonacci_const(n - 1) + fibonacci_const(n - 2)
    }
}

let fib_10 = fibonacci_const(10)  # 编译时计算

# 编译时类型生成
@derive(Debug, Clone, PartialEq)
class Point {
    x: f64
    y: f64
}
```

## 高级特性

### 1. 内存管理

```valkyrie
# 垃圾回收
let data = allocate_large_data()  # 自动管理内存
# 当 data 离开作用域时自动回收

# 引用计数
let shared_data = Rc::new(expensive_data())
let reference1 = shared_data.clone()
let reference2 = shared_data.clone()
# 当所有引用都离开作用域时自动释放
```

### 2. 并发和异步

```valkyrie
# 异步函数
async micro fetch_data(url: String) -> Result<String, Error> {
    let response = http_client.get(url).await?
    Fine { value: response.text().await? }
}

# 并发执行
async micro process_urls(urls: [String]) -> [Result<String, Error>] {
    let futures = urls.map { $url -> fetch_data($url) }
    futures.join_all().await
}
```

### 3. 性能优化

```valkyrie
# 内联优化
@inline
micro fast_add(a: i32, b: i32) -> i32 {
    a + b
}

# 特化优化
@specialize
micro generic_sort<T>(data: [T]) -> [T]
where T: Ord
{
    # 为每个具体类型生成优化版本
    data.sort()
}

# 零成本抽象
let result = numbers
    .iter()
    .map { $x * $x }
    .filter { $x > 100 }
    .collect()
# 编译后等价于手写循环
```

## 总结

Valkyrie 语言提供了丰富的特性集合：

1. **类型安全**: 强大的静态类型系统，编译时捕获错误
2. **表达能力**: 模式匹配、高阶类型、类型函数等高级特性
3. **函数式**: 高阶函数、闭包、不可变性等函数式编程特性
4. **面向对象**: 类、继承、特征等面向对象编程支持
5. **模块化**: 灵活的命名空间和导入系统
6. **元编程**: 宏系统和编译时计算
7. **性能**: 零成本抽象和编译时优化
8. **安全**: 内存安全和错误处理机制

这些特性使得 Valkyrie 能够适应各种编程场景，从系统编程到应用开发，从函数式编程到面向对象编程，都能提供优秀的开发体验和运行时性能。