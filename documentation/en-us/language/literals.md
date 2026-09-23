# Literals

Valkyrie supports various literal types for representing constant values in programs.

## Numeric Literals

### Integer Literals

```valkyrie
# Decimal integer
42
-17
0

# Hexadecimal integer
0xFF
0x1A2B

# Binary integer
0b1010
0b11110000

# Octal integer
0o755
0o644

# Integers with underscore separators (improves readability)
1_000_000
0xFF_FF_FF
0b1010_1010
```

### Floating-Point Literals

```valkyrie
# Standard floating-point
3.14
-2.5
0.0

# Scientific notation
1.23e4
-5.67E-3
2.0e+10

# With underscore separators
3.141_592_653
1.234_567e-8
```

## String Literals

### Ordinary Strings

```valkyrie
# Double-quoted strings
"Hello, World!"
"This is a string"

# Single-quoted strings
'Hello, World!'
'Single-quoted string'

# Empty strings
""
''
```

### Escape Sequences

```valkyrie
# Common escape sequences
"Newline: \n"
"Tab: \t"
"Carriage return: \r"
"Backslash: \\"
"Double quote: \""
"Single quote: \'"

# Unicode escape
"\u{1F600}"  # 😀 emoji
"\u{4E2D}"   # Chinese character "中"
```

### Raw Strings

```valkyrie
# Raw strings (escape sequences are not processed)
r"C:\Users\Name\Documents"
r'This is a raw string \n will not be a newline'

# Multi-line raw strings
r"""
This is a
multi-line raw string
\n escapes are not processed
"""
```

### Multi-line Strings

```valkyrie
# Multi-line double-quoted strings
"""
This is a
multi-line string
supports \n escapes
"""

# Multi-line single-quoted strings
'''
Multi-line
single-quoted
string
'''
```

### String Interpolation

```valkyrie
let name = "Alice"
let age = 30

# Variable interpolation
"My name is ${name}"

# Expression interpolation
"Age next year: ${age + 1}"

# Conditional expression interpolation
let temperature = 25
let weather = "It's ${ if temperature > 20 { "warm" } else { "cool" } } today"
```

## Character Literals

```valkyrie
# Single character
'a'
'中'
'1'

# Escape characters
'\n'
'\t'
'\\'
'\"'

# Unicode characters
'\u{1F600}'  # 😀
'\u{4E2D}'   # 中
```

## Boolean Literals

```valkyrie
# Boolean values
true
false
```

## Null Literal

```valkyrie
# Null value
null
```

## Array Literals

```valkyrie
# Empty array
[]

# Integer array
[1, 2, 3, 4, 5]

# String array
["apple", "banana", "cherry"]

# Mixed type array
[1, "hello", true, null]

# Nested array
[[1, 2], [3, 4], [5, 6]]

# Multi-line array
[
    "first",
    "second",
    "third"
]
```

### Map Literals

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
