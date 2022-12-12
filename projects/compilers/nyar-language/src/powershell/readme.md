# PowerShell language package

Concrete PowerShell guest over `std-data::text::powershell`.

## Duties

- Reuse `PowerShellScript` / `PsStmt` / `PsExpr` from `std-data` (parse only there).
- `PowerShellModule` / `PowerShellSemanticBridge` are concrete types (`language_id` / `source_path` / `exported_symbols`); no shared `HostScript*` layer.
- Provide `interpret` + thin `legacy-vm` adapter (bash pattern).

## Demo subset

- Variables, `if`/`else`, `while`, `for`
- `function` / `return`
- `Write-Output` / `Write-Host` / `echo`, `Get-Variable` / `Set-Variable`
- Arithmetic / comparison / logic ops and single-stage `|` pipeline

## Current status

- Module + semantic bridge + `interpret` for the legend demo subset.
- VM `evaluator/powershell.rs` delegates here.

See [`../../../host-script-languages.md`](../../../host-script-languages.md).
