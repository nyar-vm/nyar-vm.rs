# 控制流

Valkyrie 提供了豐富的控制流語句，用於控制程式的執行流程。

## 條件語句

### if 語句

```valkyrie
# 基本 if 語句
if condition {
    # 執行程式碼
}

# if-else 語句
if x > 0 {
    print("正數")
}
else {
    print("非正數")
}

# if-else if-else 鏈
if score >= 90 {
    grade = "A"
}
else if score >= 80 {
    grade = "B"
}
else if score >= 70 {
    grade = "C"
}
else {
    grade = "F"
}

# if 表達式（返回值）
let result = if x > 0 { "positive" } else { "non-positive" }

# 多行 if 表達式
let message = if user.is_admin {
    "管理員用戶"
}
else if user.is_premium {
    "高級用戶"
}
else {
    "普通用戶"
}
```

### 條件表達式

```valkyrie
# 三元運算子風格
let max = if a > b { a } else { b }

# 鏈式條件
let status = if online { "在線" } else if busy { "忙碌" } else { "離線" }
```

## 循環語句

### loop 語句（無限循環）

```valkyrie
# 基本無限循環
loop {
    # 無限執行的程式碼
    if should_break {
        break
    }
}

# 帶標籤的循環
'outer: loop {
    'inner: loop {
        if condition1 {
            break 'outer  # 跳出外層循環
        }
        if condition2 {
            break 'inner  # 跳出內層循環
        }
    }
}

# break 返回值
let found = loop {
    let item = get_next_item()
    if item.is_target() {
        break Some { value: item }  # 返回找到的項
    }
    if no_more_items() {
        break None  # 返回空值
    }
}
```

### while 語句

```valkyrie
# 基本 while 循環
while condition {
    # 當條件為真時執行
    update_condition()
}

# 複雜條件
while x > 0 && y < 100 {
    x -= 1
    y += 2
}

# while let 模式匹配
while let Some { value: item } = iterator.next() {
    process(item)
}

# 帶標籤的 while 循環
'search: while has_more_data() {
    let data = get_next_data()
    if data.is_target() {
        break 'search
    }
}
```

### until 語句

```valkyrie
# until 循環（當條件為假時執行）
until condition {
    # 當條件為假時執行
    update_condition()
}

# 等價於 while !condition
until x <= 0 {
    x -= 1
}

# until let 模式匹配
until let None = optional_value {
    process(optional_value.unwrap())
    optional_value = get_next_optional()
}
```

### for 語句

```valkyrie
# 範圍循環
for i in 0..<10 {
    print(i)  # 輸出 0 到 9
}

# 包含結束值的範圍
for i in 0..=10 {
    print(i)  # 輸出 0 到 10
}

# 陣列迭代
let numbers = [1, 2, 3, 4, 5]
for num in numbers {
    print(num)
}

# 帶索引的迭代
for (index, value) in numbers.enumerate() {
    print(f"索引 ${index}: 值 ${value}")
}

# 字串迭代
for char in "hello".chars() {
    print(char)
}

# 物件屬性迭代
for (key, value) in object.entries() {
    print(f"${key}: ${value}")
}

# 帶條件的 for 循環
for item in collection where item.is_valid() {
    process(item)
}

# 嵌套循環
for i in 0..<3 {
    for j in 0..<3 {
        print(f"(${i}, ${j})")
    }
}
```

## 模式匹配

### match 語句

Valkyrie 的 `match` 語句提供了強大的結構化模式匹配能力。

```valkyrie
match value {
    # 字面量匹配
    case 1: print("One")
    case "hello": print("Greeting")
    
    # 變量綁定
    case x: print("Got ${x}")
    
    # 類型匹配
    case is i32: print("It's an integer")
    
    # 結構化解構
    case Point { x, y }: print("Point at ${x}, ${y}")
    
    # 列表匹配
    case [first, ..rest]: print("First: ${first}, Rest: ${rest}")
    
    # 守衛條件
    case x if x > 100: print("Large number: ${x}")
    
    # 通配符
    case _: print("Something else")
}
```

## 跳轉語句

- `break`: 立即跳出當前循環。可以帶標籤或返回值。
- `continue`: 跳過當前循環的剩餘部分，進入下一次迭代。可以帶標籤。
- `return`: 從函數中返回。可以帶返回值。

## 異常傳播

```valkyrie
# 使用 ? 操作符傳播異常
micro process_file(path: string) -> Result⟨string, IOError⟩ {
    let content = read_file(path)?  # 如果失敗則提前返回錯誤
    let processed = transform(content)?
    Fine { value: processed }
}

# 手動拋出異常
micro validate_age(age: i32) -> Result⟨unit, ValidationError⟩ {
    if age < 0 {
        throw ValidationError("年齡不能為負數")
    }
    if age > 150 {
        throw ValidationError("年齡不能超過150")
    }
    Fine { value: unit }
}
```
