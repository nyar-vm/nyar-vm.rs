# Valkyrie 语言快速入门

欢迎使用 Valkyrie 语言！Valkyrie 是一个现代化的函数式编程语言，提供强大的类型系统、灵活的模块系统和丰富的语言特性。

## 什么是 Valkyrie？

Valkyrie 是一个多范式编程语言，它提供：

- 🎯 **强大的类型系统**：支持泛型、高阶类型、类型推导等高级特性
- 🚀 **现代语法**：简洁直观的语法，支持模式匹配、闭包等现代特性
- 🔒 **内存安全**：垃圾回收器自动管理内存，避免内存泄漏
- ⚡ **高性能**：零成本抽象，编译时优化
- 🔧 **灵活的模块系统**：基于命名空间的模块组织方式

## 基本语法

### 变量定义

```valkyrie
# 不可变变量
let name = "Alice"
let age = 30
let is_active = true

# 可变变量
let mut counter = 0
let mut items = []

# 显式类型注解
let score: i32 = 95
let price: f64 = 29.99
let message: String = "Hello"
```

### 函数定义

```valkyrie
# 基本函数定义
micro greet() {
    print("Hello, World!")
}

# 带参数和返回值的函数
micro add(a: i32, b: i32) -> i32 {
    a + b
}

# 多参数函数
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

### 基本数据类型

```valkyrie
# 整数类型
let a: i32 = 42
let b: u64 = 100

# 浮点类型
let x: f32 = 3.14
let y: f64 = 2.718281828

# 布尔类型
let flag: bool = true

# 字符和字符串
let ch: char = 'A'
let text: String = "Hello, World!"

# 数组类型
let numbers: [i32; 5] = [1, 2, 3, 4, 5]
let dynamic: [String] = ["a", "b", "c"]

# 元组类型
let point: (f64, f64) = (3.0, 4.0)
let mixed: (String, i32, bool) = ("test", 42, true)
```

## 控制流

### 条件语句

```valkyrie
# if 语句
if x > 0 {
    print("正数")
} else {
    print("非正数")
}

# if 表达式
let result = if x > 0 { "positive" } else { "non-positive" }

# 多重条件
if score >= 90 {
    grade = "A"
} else if score >= 80 {
    grade = "B"
} else {
    grade = "F"
}
```

### 循环语句

```valkyrie
# while 循环
while counter < 10 {
    print(counter)
    counter = counter + 1
}

# for 循环
for i in 0..10 {
    print(i)
}

# 遍历数组
for item in items {
    print(item)
}

# 无限循环
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

# 范围匹配
match score {
    case 90..=100: "A"
    case 80..=89: "B"
    case 70..=79: "C"
    case _: "F"
}

# 元组解构
match point {
    case (0, 0): "Origin"
    case (x, 0): "On X-axis at {x}"
    case (0, y): "On Y-axis at {y}"
    case (x, y): "Point at ({x}, {y})"
}
```

## 类型定义

### 记录类型

```valkyrie
# 基本记录类型
type Point = {
    x: f64,
    y: f64,
}

# 泛型记录类型
type Container<T> = {
    value: T,
    metadata: String,
}
```

### 联合类型

```valkyrie
# 基本联合类型
union Result<T, E> {
    Fine { value: T },
    Fail { error: E }
}

# 使用联合类型
let result: Result<i32, String> = Fine { value: 42 }
match result {
    case Fine { value }: print("Success: {value}")
    case Fail { error }: print("Error: {error}")
}
```

### 类定义

```valkyrie
# 基本类定义
class Person {
    name: String
    age: i32
    
    new(name: String, age: i32) -> Self {
        Self { name, age }
    }
    
    speak(self) {
        print("Hello, I'm {self.name}")
    }
    
    get_info(self) -> String {
        "{self.name} is {self.age} years old"
    }
}

### 异常处理

```valkyrie
micro divide(a: i32, b: i32) -> Result⟨i32, String⟩ {
    if b == 0 {
        Fail("Division by zero")
    } else {
        Fine(a / b)
    }
}

match divide(10, 2) {
    case Fine(res): print("Result: {res}")
    case Fail(err): print("Error: {err}")
}
```

# 使用类
let person = Person::new("Alice", 30)
person.speak()
let info = person.get_info()
```

## 模块系统

### 命名空间声明

```valkyrie
# 声明命名空间
namespace math.geometry {
    class Point {
        x: f64
        y: f64
    }
    
    micro distance(p1: Point, p2: Point) -> f64 {
        let dx = p1.x - p2.x
        let dy = p1.y - p2.y
        (dx * dx + dy * dy).sqrt()
    }
}
```

### 导入系统

```valkyrie
# 导入整个命名空间
using math.geometry.*

# 选择性导入
using math.geometry.{Point, distance}

# 重命名导入
using math.geometry.Point as GeomPoint

# 使用导入的内容
micro main() {
    let p1 = Point { x: 0.0, y: 0.0 }
    let p2 = Point { x: 3.0, y: 4.0 }
    let dist = distance(p1, p2)
    print("Distance: {dist}")
}
```

## 字面量

### 数值字面量

```valkyrie
# 整数字面量
42
0xFF        # 十六进制
0b1010      # 二进制
0o755       # 八进制
1_000_000   # 带分隔符

# 浮点数字面量
3.14
1.23e4      # 科学计数法
3.141_592_653  # 带分隔符
```

### 字符串字面量

```valkyrie
# 普通字符串
"Hello, World!"
'单引号字符串'

# 转义序列
"换行符：\n"
"制表符：\t"
"Unicode：\u{1F600}"  # 😀 表情符号

# 原始字符串
r"C:\Users\Name\Documents"
r"""多行原始字符串
不处理转义序列"""

# 字符串插值
let name = "Alice"
let age = 30
let message = "Hello, {name}! You are {age} years old."
```

### 其他字面量

```valkyrie
# 数组字面量
[1, 2, 3, 4, 5]
["a", "b", "c"]

# 对象字面量
{
    name: "Alice",
    age: 30,
    active: true
}

# 元组字面量
(1, 2, 3)
("name", 30, true)

# 范围字面量
0..=100     # 包含范围
1..<10      # 排除范围

# 正则表达式字面量
re"hello"
re"\d+"
re"[a-zA-Z]+"
```

## 闭包和高阶函数

```valkyrie
# 基本闭包语法
let square = { $x * $x }
let add = { $x + $y }

# 显式参数类型
let multiply = { $x: i32, $y: i32 -> $x * $y }

# 多语句闭包
let complex = {
    let result = $x * 2
    result + 1
}

# 高阶函数使用
let numbers = [1, 2, 3, 4, 5]
let squares = numbers.map { $x * $x }
let evens = numbers.filter { $x % 2 == 0 }
let sum = numbers.reduce { $acc + $x }
```

## 类型函数 (mezzo)

```valkyrie
# 类型函数定义
mezzo IsEven(z: Type) -> bool {
    # 检查类型 z 是否表示偶数
    match z {
        i32 if z % 2 == 0 => true,
        _ => false
    }
}

# 类型映射
mezzo MapType<T>(input: T) -> T {
    # 对输入类型进行映射变换
    match input {
        i32 => i64,
        f32 => f64,
        _ => input
    }
}
```

## 下一步

现在你已经了解了 Valkyrie 语言的基本语法和特性，可以：

1. **深入学习**：查看 [语言特性详细指南](./features.md)
2. **类型系统**：了解 [类型系统](../language/type-system/index.md) 的高级特性
3. **模式匹配**：掌握 [模式匹配](../language/pattern-match.md) 的强大功能
4. **模块系统**：学习 [模块系统](../language/modules.md) 的组织方式
5. **元编程**：探索 [元编程](../language/meta-programming/index.md) 的高级用法

开始你的 Valkyrie 编程之旅吧！