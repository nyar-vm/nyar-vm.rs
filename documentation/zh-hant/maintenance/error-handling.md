# Valkyrie 錯誤處理與診斷指南

Valkyrie 採用了一種結構化、國際化優先的錯誤處理機制。每個錯誤不僅僅是一條文本消息，而是一個包含豐富元數據的數據結構，旨在提供 IDE 級別的診斷體驗。

## 1. 設計原則

- **結構化 (Structured)**: 錯誤應包含錯誤代碼、i18n 鍵和參數化數據。
- **國際化 (i18n)**: 錯誤消息不應硬編碼，而是通過 `key` 在不同語言的本地化文件中查找。
- **輕量級 (Lightweight)**: 使用 `Box<ErrorKind>` 模式，確保錯誤類型的大小保持最小，優化性能。
- **無外部宏 (No Magic Macros)**: 嚴禁使用 `thiserror` 或 `anyhow`。所有錯誤實現應保持顯式且易於追蹤。

## 2. 核心數據結構

### `ValkyrieError`

這是所有錯誤的頂層容器：

```rust
pub struct ValkyrieError {
    pub code: u32,                  // 唯一的錯誤代碼，如 0x001
    pub kind: Box<ValkyrieErrorKind>, // 具體的錯誤分類（包含 i18n key 和數據）
    pub labels: Vec<ValkyrieLabel>, // 源碼標註（支持 i18n）
    pub help: Option<ValkyrieHelp>, // 修復建議（支持 i18n）
}

pub struct ValkyrieLabel {
    pub span: SourceSpan,
    pub key: &'static str, // 標註內容的 i18n key
    pub data: BTreeMap<String, String>, // 標註內容的 i18n 參數
    pub primary: bool,
}

pub struct ValkyrieHelp {
    pub key: &'static str,          // 修復建議的 i18n key
    pub data: BTreeMap<String, String>, // 修復建議的 i18n 參數
}
```

### `ValkyrieErrorKind`

使用枚舉對錯誤進行分類，每個變體直接攜帶該錯誤所需的參數化數據。`key` 由變體類型隱式決定：

```rust
pub enum ValkyrieErrorKind {
    UnexpectedToken { expected: Vec<String>, found: String },
    UndefinedVariable { name: String },
    TypeMismatch { expected: String, found: String },
    // ...
}

impl ValkyrieErrorKind {
    pub fn key(&self) -> &'static str {
        match self {
            Self::UnexpectedToken { .. } => "error.syntax.unexpected_token",
            Self::UndefinedVariable { .. } => "error.semantic.undefined_variable",
            // ...
        }
    }
}
```

## 3. 實現示例

### 觸發一個語法錯誤

```rust
impl ValkyrieError {
    pub fn unexpected_token(span: SourceSpan, expected: Vec<String>, found: String) -> Self {
        Self {
            code: 1001,
            kind: Box::new(ValkyrieErrorKind::UnexpectedToken { expected, found }),
            labels: vec![ValkyrieLabel::primary(span, Some("unexpected.token"))],
            help: Some(ValkyrieHelp::new("check.syntax.manual")),
        }
    }
}
```

### 使用 Builder 模式快速構造

```rust
let error = ValkyrieError::new(1001, kind)
    .add_label(ValkyrieLabel::primary(span, Some("label.key")))
    .with_help(ValkyrieHelp::new("help.key").with_data("arg", "value"));
```

### 快速構造 IO 錯誤

```rust
impl ValkyrieError {
    pub fn io_error(error: std::io::Error, path: Option<Pathbuf>, ) -> Self {
        Self::new(
            0x0001,
            ValkyrieErrorKind::IoError {
                path,
                message: error.to_string(),
            },
        )
    }
}
```

## 4. 國際化 (i18n) 流水線

1. **觸發錯誤**: 編譯器在發現問題時構造 `ValkyrieError`。
2. **攜帶數據**: 錯誤對象僅攜帶 `key` 和 `data`（如 `{ "name": "foo" }`）。
3. **渲染診斷**: 
   - 診斷系統根據當前語言環境加載翻譯文件（如 `zh-CN.toml`）。
   - 使用 `key` 找到模板：`error.type.mismatch = "類型不匹配：期望 {expected}，但找到了 {found}"`。
   - 將 `data` 中的值填充進模板。

## 5. 最佳實踐

- **保持 ErrorKind 純淨**: 僅包含用於進一步分析或恢復的必要數據。
- **延遲渲染**: 不要在構造錯誤時格式化字符串。格式化應僅在最終輸出給用戶時進行。
- **使用 Error Code**: 每個面向用戶的錯誤都應分配一個永久的錯誤代碼，以便用戶查閱文檔。
- **禁止使用 `anyhow`**: 編譯器需要強類型的錯誤來決定是否可以繼續後續階段，`anyhow` 會丟失這些類型信息。
- **禁止使用 `thiserror`**: 我們需要自定義 `Diagnostic` 實現，手動實現 `Display` 和 `Error` trait 能提供更大的靈活性。
