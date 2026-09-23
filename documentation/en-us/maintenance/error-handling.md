# Valkyrie Error Handling and Diagnostics Guide

Valkyrie adopts a structured, internationalization-first error handling mechanism. Each error is not just a text message, but a data structure containing rich metadata, designed to provide an IDE-level diagnostic experience.

## 1. Design Principles

- **Structured**: Errors should contain an error code, an i18n key, and parameterized data.
- **Internationalization (i18n)**: Error messages should not be hardcoded but looked up via a `key` in localization files for different languages.
- **Lightweight**: Use the `Box<ErrorKind>` pattern to ensure the error type size remains minimal, optimizing performance.
- **No Magic Macros**: Strictly avoid using `thiserror` or `anyhow`. All error implementations should remain explicit and easy to trace.

## 2. Core Data Structures

### `ValkyrieError`

This is the top-level container for all errors:

```rust
pub struct ValkyrieError {
    pub code: u32,                  // Unique error code, e.g., 0x001
    pub kind: Box<ValkyrieErrorKind>, // Specific error category (contains i18n key and data)
    pub labels: Vec<ValkyrieLabel>, // Source code labels (supports i18n)
    pub help: Option<ValkyrieHelp>, // Repair suggestions (supports i18n)
}

pub struct ValkyrieLabel {
    pub span: SourceSpan,
    pub key: &'static str, // i18n key for the label content
    pub data: BTreeMap<String, String>, // i18n parameters for the label content
    pub primary: bool,
}

pub struct ValkyrieHelp {
    pub key: &'static str,          // i18n key for the repair suggestion
    pub data: BTreeMap<String, String>, // i18n parameters for the repair suggestion
}
```

### `ValkyrieErrorKind`

Errors are categorized using an enum, with each variant carrying the parameterized data required for that error. The `key` is implicitly determined by the variant type:

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

## 3. Implementation Example

### Triggering a Syntax Error

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

### Rapid Construction Using the Builder Pattern

```rust
let error = ValkyrieError::new(1001, kind)
    .add_label(ValkyrieLabel::primary(span, Some("label.key")))
    .with_help(ValkyrieHelp::new("help.key").with_data("arg", "value"));
```

### Rapid Construction of an IO Error

```rust
impl ValkyrieError {
    pub fn io_error(error: std::io::Error, path: Option<PathBuf>) -> Self {
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

## 4. Internationalization (i18n) Pipeline

1. **Triggering the Error**: The compiler constructs a `ValkyrieError` when it detects an issue.
2. **Carrying Data**: The error object carries only the `key` and `data` (e.g., `{ "name": "foo" }`).
3. **Rendering Diagnostics**: 
   - The diagnostic system loads translation files (e.g., `en-US.toml`) based on the current locale.
   - It finds the template using the `key`: `error.type.mismatch = "Type mismatch: expected {expected}, but found {found}"`.
   - It populates the template with values from `data`.

## 5. Best Practices

- **Keep ErrorKind Pure**: Include only the data necessary for further analysis or recovery.
- **Deferred Rendering**: Do not format strings when constructing errors. Formatting should only occur during final output to the user.
- **Use Error Codes**: Assign a permanent error code to each user-facing error so users can consult the documentation.
- **No `anyhow`**: The compiler needs strongly-typed errors to decide whether to proceed to subsequent stages; `anyhow` loses this type information.
- **No `thiserror`**: We need custom `Diagnostic` implementations; manually implementing the `Display` and `Error` traits provides greater flexibility.
