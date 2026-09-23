# bash src

`Bash` root-level guest package (alongside `valkyrie`, not inside it).

## Duties

- Reuse `std-data::text::bash` parse/AST models.
- Concrete `BashModule` / `BashSemanticBridge` (no shared `HostScript*` / language-crate trait layer).
- Provide `interpret` tree-walker as the language-package primary evaluator; `legacy-vm` only thin-adapts.

## Interpret subset (demo)

- Vars / `$var` / `${var}` / `$?` / `$1`… / `$#`
- `if` / `else` / `elif`, `while`, `for … in`
- Functions + `return`
- Builtins: `echo`, `printf` (`%s` / `%d` / `%i`), `test` / `[ ]`, `true` / `false`, `cd`, `exit`
- `&&` / `||`, pipelines (sequential stages), virtual redirects

## Current status

- Module + semantic bridge (exports top-level function names) + `interpret` for the legend demo subset.
- VM `evaluator/bash.rs` delegates here (same thin-adapter shape as C / Lua / PowerShell).

See [`../../../host-script-languages.md`](../../../host-script-languages.md).
