# 匿名函數與閉包

## 匿名函數

匿名函數是沒有名稱的函數，可以直接在表達式中定義和使用。

### 基本語法

```valkyrie
# 基本匿名函數
let add = micro(x, y) { x + y }

# 單參數匿名函數
let square = micro(x) { x * x }

# 無參數匿名函數
let get_random = micro() { random() }
```

## 閉包

閉包是一種特殊的匿名函數，可以捕獲其定義環境中的變量。

### 閉包語法
閉包使用花括號 `{}` 定義，參數使用 `$` 前綴：

```valkyrie
# 單參數閉包
let numbers = [1, 2, 3, 4, 5]
let doubled = numbers.map { $x * 2 }

# 多參數閉包
let pairs = [(1, 2), (3, 4), (5, 6)]
let sums = pairs.map { $a + $b }

# 無參數閉包
let lazy_value = { 42 }
```

### 參數自動推斷

閉包中的參數會按照首次出現的順序自動註冊到函數簽名中：

```valkyrie
# $x 是第一個參數，$y 是第二個參數
let operation = { $x + $y * 2 }

# 只使用一個參數
let increment = { $n + 1 }
```

## 尾隨閉包

當函數的最後一個參數是閉包時，可以使用尾隨閉包語法，省略括號：

```valkyrie
# 傳統調用方式
list.map(micro(x) { x * 2 })

# 尾隨閉包語法（完全等價）
list.map { $x * 2 }

# 多個參數時，只有最後一個可以使用尾隨語法
list.fold(0, micro(acc, item) { acc + item })
# 等價於
list.fold(0) { $acc + $item }
```

### 複雜示例

```valkyrie
# 鏈式調用與尾隨閉包
let result = numbers
    .filter { $x > 0 }
    .map { $x * $x }
    .fold(0) { $acc + $item }

# 嵌套閉包
let matrix = [[1, 2], [3, 4], [5, 6]]
let flattened = matrix
    .map { $row.map { $x * 2 } }
    .flatten()
```

## 閉包捕獲

閉包可以捕獲其定義環境中的變量：

```valkyrie
let multiplier = 10
let numbers = [1, 2, 3, 4, 5]

# 閉包捕獲外部變量 multiplier
let scaled = numbers.map { $x * multiplier }

# 捕獲可變變量
let mut counter = 0
let increment_counter = {
    counter += 1
    counter
}
```

## 高階函數示例

```valkyrie
# 自定義高階函數
micro apply_twice⟨T⟩(value: T, f: micro(T) -> T) -> T {
    f(f(value))
}

# 使用尾隨閉包
let result = apply_twice(5) { $x * 2 }  # 結果: 20

# 函數組合
micro compose⟨A, B, C⟩(f: micro(B) -> C, g: micro(A) -> B) -> micro(A) -> C {
    { f(g($x)) }
}

let add_one = micro(x) { x + 1 }
let double = micro(x) { x * 2 }
let add_one_then_double = compose(double, add_one)
```

## 最佳實踐

1. **簡潔性**: 對於簡單操作，優先使用閉包而不是命名函數
2. **可讀性**: 複雜邏輯應該使用命名函數以提高可讀性
3. **尾隨閉包**: 當閉包是最後一個參數時，使用尾隨語法提高代碼美觀度
4. **參數命名**: 在閉包中使用有意義的參數名（如 `$item`, `$element` 而不是 `$x`）

```valkyrie
# 好的實踐
users.filter { $user.is_active }
     .map { $user.name }
     .sort_by { $name.length }

# 避免過度嵌套
let process_data = micro(data) {
    data.filter { $item.is_valid }
        .transform { $item.normalize() }
        .group_by { $item.category }
}
```