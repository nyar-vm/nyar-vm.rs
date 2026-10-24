# Bash text frontend (minimal subset)

Lexer, parser, and AST for legend / legacy-vm demo scripts.

## Supported (demo subset)

- Assignments / `export NAME[=value]`
- `if` / `then` / `elif` / `else` / `fi`
- `while` / `for … in` / `do` / `done`
- Functions: `name() { … }` / `function name { … }`
- Simple commands, pipelines `|`, `&&` / `||`
- Redirections `>`, `>>`, `<` (text model only)
- Words / quoted strings; `$var` / `${var}` / `$?` left as text for the interpreter

Not a full shell grammar — enough to drive host-script fixtures.
