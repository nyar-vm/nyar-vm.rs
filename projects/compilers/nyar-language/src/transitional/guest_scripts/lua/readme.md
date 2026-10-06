# Lua text frontend (mid-subset)

Lexer, parser, and AST for legend / legacy-vm. **Not** full Lua 5.x — richer than shell demos, centered on control
flow + tables.

## Supported

- Locals / assignments (including multi-name and `t.field` / `t[key]` targets)
- `if` / `elseif` / `else`, `while`, `repeat`/`until`, numeric `for`, `break`, `return`
- Named `function` / `local function`, calls
- Tables: `{...}` constructors (array / `name=` / `[expr]=`), index & field access, `#` length
- Number / string ops, `..`, comparisons, `and` / `or` / `not`
- Literals: numbers, strings, `true` / `false` / `nil`
- Line comments `--`

## Out of scope

- Full Lua 5.x / metatables / coroutines / `require`
- Generic `for` (`pairs`/`ipairs` iterators as language syntax)
- Method calls (`obj:method`), varargs, closures as first-class values
- Bitwise ops, long strings `[[...]]`, goto / labels
