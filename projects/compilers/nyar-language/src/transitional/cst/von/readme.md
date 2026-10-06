# von-parser src

这里是 `von-parser` 的源码目录。

## 职责

- `lib.rs` 定义 `VonValue`、`VonParseError`、`VonParser`。
- 保持 `VON` 文本模型和解析行为稳定。
- 维护和 `serde` 的值层互通。

## 架构

```text
source text
  → lexer::Lexer::tokenize()   # TokenKind + byte span
  → parser::VonParser::parse() # token 驱动递归下降
  → VonValue                   # serde 值模型（非 AST）
```

- `lexical.rs`：标识符规则与字符串解码，供 lexer 使用。
- `lexer/`：词法分析。
- `parser/`：语法分析，只消费 token 流。
- **文本格式化**（`to_string` 等）在 `nyar-language::text::von`，不在本 crate。

## 禁止

- 不在这里解析 `legion` 项目清单语义。
- 不引入与语言 `AST / HIR / MIR / LIR` 相关的结构。
