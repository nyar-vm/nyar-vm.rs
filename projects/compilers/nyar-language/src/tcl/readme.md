# tcl src

`Tcl` root host-script guest (alongside `valkyrie`).

## Role

- Reuse the Oak Tcl parse model (parse only there); keep script semantics in this frontend adapter.
- Concrete `TclModule` / `TclSemanticBridge` + `interpret` (primary in this package).
- `legacy-vm` only thin-adapts (`evaluator/tcl.rs` → `interpret` here; bash pattern).
- `legend`: CLI / fixtures only.

## Subset

- `set` / `puts` / `expr` / `incr` / `return`
- `$` / `${}` / `[...]`
- `if` / `while` / `for` / `foreach` / `proc`
- `list` / `llength` / `lindex`

See [`../../../host-script-languages.md`](../../../host-script-languages.md).
