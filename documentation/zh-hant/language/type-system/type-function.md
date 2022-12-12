# 類型函數 (Type Function)

類型函數是 Valkyrie 語言中用於在類型層面進行計算的強大特性。透過 `mezzo` 關鍵字定義，類型函數允許在編譯時對類型進行操作和變換。

## 基本語法

```valkyrie
mezzo FunctionName(param: Type) -> Type {
    $ 類型函數體
}
```

## 示例

### 判斷偶數類型

```valkyrie
mezzo IsEven(z: Type) -> bool {
    $ 檢查類型 z 是否表示偶數
    match z {
        i32 if z % 2 == 0 => true,
        _ => false
    }
}
```

### 類型映射

```valkyrie
mezzo MapType⟨T⟩(input: T) -> Type {
    $ 對輸入類型進行映射變換
    match input {
        i32 => i64,
        f32 => f64,
        _ => input
    }
}
```

### 條件類型選擇

```valkyrie
mezzo ConditionalType⟨T, U⟩(condition: bool) -> Type {
    $ 根據條件選擇類型
    if condition {
        T
    } else {
        U
    }
}
```

## 特性

- **編譯時執行**: 類型函數在編譯時執行，不會產生執行時期開銷
- **類型安全**: 所有類型操作都經過編譯器驗證
- **遞迴支持**: 支持遞迴類型函數定義
- **模式匹配**: 可以對類型進行模式匹配

## 使用場景

1. **類型驗證**: 在編譯時驗證類型是否滿足特定條件
2. **類型轉換**: 自動推導和轉換相關類型
3. **泛型約束**: 為泛型參數添加複雜的類型約束
4. **元編程**: 實現高級的編譯時代碼生成

## 注意事項

- 類型函數必須是純函數，不能有副作用
- 所有分支都必須返回有效的類型
- 遞迴深度有限制，防止無限遞迴
