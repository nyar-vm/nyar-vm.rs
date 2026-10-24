# Valkyrie text snapshot fixtures

`legion` 持有的 Valkyrie 源文件与文本 sidecar 基线。`std-data` / `nyar-language` 只消费、不持有 fixture。

## 文件形态

每个源文件配多个文本 sidecar，同级存放：

```
lexer/angle_less.v
lexer/angle_less.v.lex
lexer/angle_less.v.parse
lexer/angle_less.v.highlight
```

## 输出协议

- `*.lex`：lossless token 流，每行 `Token { kind, span, text }`
- `*.parse`：分层 parser tree dump——CST 壳（Trivia / Statement / Error）+ AST / Xg / Tg 子树展开；每节点含稳定 `kind` 与
  `span`
- `*.highlight`：merged highlight spans（lexical + semantic），每行 `Span { kind, span, text, modifier }`；语义层为空时等价于
  lexical merged

禁止使用 Rust `Debug` 直接落盘；printer 实现在 [
`std-data/src/text/valkyrie/parse_dump/`](../../../../std-data/src/text/valkyrie/parse_dump/mod.rs)。

## 重生成

```powershell
$env:NYAR_TEST_REGENERATE = "1"
cargo test -p std-data text_fixture_lex_and_parse_regression
cargo test -p nyar-language text_fixture_highlight_regression
```

也支持 `VALKYRIE_TEST_REGENERATE` 与 `LEGION_TEST_REGENERATE`（宽语义：`1` / `true` / `yes` / `on` / `regenerate`）。

首次运行缺少 sidecar 时会自动创建，无需显式 regenerate。

## 目录约定

| 子目录     | 覆盖范围                                                   |
|------------|------------------------------------------------------------|
| `lexer/`   | lossless token、trivia、XML token、字符串字面量            |
| `core/`    | 核心语法：参数、类型表达式、construct                      |
| `surface/` | 声明级 surface form：widget、flags、union/enums            |
| `vx/`      | `.vx` widget markup、T-Grammar meta fixup（走 `parse_vx`） |

## 消费方

| Crate           | Runner                                   | Sidecar           |
|-----------------|------------------------------------------|-------------------|
| `std-data`      | `tests/text/valkyrie/fixtures/runner.rs` | `.lex` / `.parse` |
| `nyar-language` | `tests/valkyrie/highlight/fixtures.rs`   | `.highlight`      |

路径解析（两 crate 相同）：`CARGO_MANIFEST_DIR/../legion/tests/fixtures/text/valkyrie`
