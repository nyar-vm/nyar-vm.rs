# 字面量

Valkyrie 支持多種字面量類型，用於表示程式中的常量值。

## 數值字面量

### 整數字面量

```valkyrie
# 十進制整數
42
-17
0

# 十六進制整數
0xFF
0x1A2B

# 二進制整數
0b1010
0b11110000

# 八進制整數
0o755
0o644

# 帶下劃線分隔符的整數（提高可讀性）
1_000_000
0xFF_FF_FF
0b1010_1010
```

### 浮點數字面量

```valkyrie
# 標準浮點數
3.14
-2.5
0.0

# 科學計數法
1.23e4
-5.67E-3
2.0e+10

# 帶下劃線分隔符
3.141_592_653
1.234_567e-8
```

## 字符串字面量

### 普通字符串

```valkyrie
# 雙引號字符串
"Hello, World!"
"這是一個中文字符串"

# 單引號字符串
'Hello, World!'
'單引號字符串'

# 空字符串
""
''
```

### 轉義序列

```valkyrie
# 常見轉義序列
"換行符：\n"
"製表符：\t"
"回車符：\r"
"反斜杠：\\"
"雙引號：\""
"單引號：\'"

# Unicode 轉義
"\u{1F600}"  # 😀 表情符號
"\u{4E2D}"   # 中文字符 "中"
```

### 原始字符串

```valkyrie
# 原始字符串（不處理轉義序列）
r"C:\Users\Name\Documents"
r'這是原始字符串 \n 不會換行'

# 多行原始字符串
r"""
這是一個
多行原始字符串
不處理 \n 轉義
"""
```

### 多行字符串

```valkyrie
# 多行雙引號字符串
"""
這是一個
多行字符串
支持 \n 轉義
"""

# 多行單引號字符串
'''
多行
單引號
字符串
'''
```

### 字符串插值

```valkyrie
let name = "Alice"
let age = 30

# 變量插值
"My name is ${name}"

# 表達式插值
"Age next year: ${age + 1}"

# 條件表達式插值
let temperature = 25
let weather = "It's ${ if temperature > 20 { "warm" } else { "cool" } } today"
```

## 字符字面量

```valkyrie
# 單個字符
'a'
'中'
'1'

# 轉義字符
'\n'
'\t'
'\\'
'\"'

# Unicode 字符
'\u{1F600}'  # 😀
'\u{4E2D}'   # 中
```

## 布爾字面量

```valkyrie
# 布爾值
true
false
```

## 空值字面量

```valkyrie
# 空值
null
```

## 數組字面量

```valkyrie
# 空數組
[]

# 整數數組
[1, 2, 3, 4, 5]

# 字串數組
["apple", "banana", "cherry"]

# 混合類型數組
[1, "hello", true, null]

# 嵌套數組
[[1, 2], [3, 4], [5, 6]]

# 多行數組
[
    "first",
    "second",
    "third"
]
```

### 映射字面量 (Map)

```valkyrie
{
    "name": "Alice",
    "age": 30,
    "is_admin": false
}

{
    1: "one",
    2: "two",
    3: "three"
}
```
