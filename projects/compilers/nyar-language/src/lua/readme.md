# lua src

`Lua` root-level guest package (alongside `valkyrie`, not inside it).

## Duties

- Reuse `std-data::text::lua` parse/AST models.
- Concrete `LuaModule` / `LuaSemanticBridge` (no shared `HostScript*` / language-crate trait layer).
- Provide `interpret` tree-walker as the language-package primary evaluator; `legacy-vm` only thin-adapts.

## Demo subset

Richer than bash/ps/tcl host-script demos; still **not** full Lua 5.x. Prefer control-flow + tables over stdlib stubs.

- Locals / globals, multi-assignment, `t.field` / `t[key]` stores
- `if` / `elseif` / `else`, `while`, `repeat`/`until`, numeric `for`, `break`, `return`
- Named functions + calls (enclosing locals readable via env snapshot; not full upvalues)
- Tables: constructors, index/field, `#`, optional `table_insert(t, v)`
- Number/string ops, `..`, comparisons, short-circuit `and`/`or`, `not`
- `print`

## Status

- Module + semantic bridge + `interpret` for the legend mid-subset.
- VM `evaluator/lua.rs` delegates here (bash pattern).

See [`../../../host-script-languages.md`](../../../host-script-languages.md).
