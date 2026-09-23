# tcl src

`Tcl` root host-script guest (alongside `valkyrie`).

## Role

- Reuse `std-data` `TclScript` (parse only there).
- Concrete `TclModule` / `TclSemanticBridge` + `interpret` (primary in this package).
- `legacy-vm` only thin-adapts (`evaluator/tcl.rs` → `interpret` here; bash pattern).
- `legend`: CLI / fixtures only.

## Subset

- `set` / `puts` / `expr` / `incr` / `return`
- `$` / `${}` / `[...]`
- `if` / `while` / `for` / `foreach` / `proc`
- `list` / `llength` / `lindex`

See [`../../../host-script-languages.md`](../../../host-script-languages.md).
