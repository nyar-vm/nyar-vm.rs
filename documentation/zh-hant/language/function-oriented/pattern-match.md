# 模式匹配 (Match)

Valkyrie 提供了強大的模式匹配功能，支持多種匹配模式和語法形式。

## 基本 Match 語法

### 標準 Match 語句

```valkyrie
# 基本模式匹配
match value {
    case 1: "one"
    case 2: "two"
    case 3: "three"
    else: "other"
}

# 範圍匹配
match score {
    case 90..=100: "A"
    case 80..=89: "B"
    case 70..=79: "C"
    case 60..=69: "D"
    else: "F"
}

# 多值匹配
match day {
    case "Saturday" | "Sunday": "Weekend"
    case "Monday"..="Friday": "Weekday"
    else: "Invalid day"
}
```

### 表達式 Match 語法

```valkyrie
# 表達式形式的 match
let result = match value {
    case 1: "one"
    case 2: "two"
    case 3: "three"
    else: "other"
}

# 鏈式調用
let processed = match input.transform() {
    case Fine(value): value * 2
    case Fail(error): 0
}
```

## 解構匹配

### 元組解構

```valkyrie
match point {
    case (0, 0): "Origin"
    case (x, 0): "On X-axis at ${x}"
    case (0, y): "On Y-axis at ${y}"
    case (x, y): "Point at (${x}, ${y})"
}

# 嵌套元組
match nested {
    case ((a, b), c): "Nested: ${a}, ${b}, ${c}"
    case (x, (y, z)): "Other nested: ${x}, ${y}, ${z}"
    else: "No match"
}
```

### 數組解構

```valkyrie
match array {
    case []: "Empty array"
    case [x]: "Single element: ${x}"
    case [first, second]: "Two elements: ${first}, ${second}"
    case [head, ..tail]: "Head: ${head}, Tail length: ${tail.len()}"
    case [.., last]: "Last element: ${last}"
    case [first, .. , last]: "First: ${first}, Last: ${last}"
}

# 固定長度匹配
match coordinates {
    case [x, y]: "2D point: (${x}, ${y})"
    case [x, y, z]: "3D point: (${x}, ${y}, ${z})"
    else: "Unsupported dimension"
}
```

### 對象解構

```valkyrie
match person {
    case { name: "Alice", age }: "Alice is ${age} years old"
    case { name, age: 18..=65 }: "${name} is working age"
    case { name, age, city: "Beijing" }: "${name} from Beijing, age ${age}"
}
```

## Guard 條件

可以使用 `if` 子句為模式匹配添加額外的過濾條件：

```valkyrie
match point {
    case (x, y) if x == y: "On diagonal"
    case (x, y) if x > y: "Below diagonal"
    case (x, y): "Above diagonal"
}
```

## 類型匹配

模式匹配也可以用於檢查和轉換類型：

```valkyrie
match shape {
    case s: Circle: "Circle with radius ${s.radius}"
    case s: Rectangle: "Rectangle ${s.width}x${s.height}"
    case _: "Unknown shape"
}
```
