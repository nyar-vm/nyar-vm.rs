# format

Frontend-neutral source formatting and printer contracts (same layering as `highlight`).

## Layers

| Layer | Location | Role |
|:---|:---|:---|
| Platform | `nyar-analyzer::format` | `FormatOptions`, `FormatError`, **`Document` layout engine**, **`syntax` CST**, `SourceMap`, `SourceFormatter`, `Printer`, Registry |
| Language plugin | `nyar-language` | `FormatSyntax` (CST → `Document`), `ToDocument` (model printer), per-language `SourceFormatter` |

## Two write paths (must stay separate)

| Contract | Input | Output | Use |
|:---|:---|:---|:---|
| **SourceFormatter** | Source text | `FormattedOutput` (text + `SourceMap`) | **Canonical formatting**; preserves trivia; `legion fmt` / LSP |
| **Printer** | Parsed **data model** | Text | Serialization / debug; **does not** guarantee comment or whitespace preservation |

## Canonical formatting pipeline

```text
source → lossless lexer → CST → FormatSyntax::format_document → Document::render_with_map
```

## Document engine

Wadler-style document algebra: `text`, `trivia`, `append`, `nest`, `line`, `softline`, `hardline`, `group`, `fill`.

Language plugins implement `FormatSyntax::format_document(&self, options)` (CST) and `ToDocument::to_document` (model printer; trait lives in `nyar-language` under orphan rules).

## Legacy

`FormatBuffer` is deprecated and kept only for V/AWSL transition; remove after migration.
