# powershell text

Lexer / parser / AST for the legend / legacy-vm PowerShell demo subset.

## Structure

- `lexer/`: tokens (`$var`, cmdlets, `-eq`/`-and`, `|`, …)
- `parser/`: recursive-descent script → `PsStmt` / `PsExpr`
- `ast.rs`: statement / expression tree
- `mod.rs`: `PowerShellScript::parse`

## Supported subset

- Variables: `$name = expr`, `$true` / `$false` / `$null`
- Control: `if` / `else`, `while`, `for (init; cond; step)`
- Functions: `function Name($a, $b) { ... }`, `return`
- Output cmdlets: `Write-Output` / `Write-Host` / `echo`
- Helpers: `Get-Variable` / `Set-Variable` (evaluator)
- Expressions: arithmetic `+ - * / %`, comparisons `-eq -ne -lt -le -gt -ge`, logic `-and -or -xor -not`, `-like` /
  `-notlike`
- Pipeline: `expr | Cmdlet` (single-stage demo)
- Command calls: space-separated args and `(Name $a $b)`

Layering: [`../../../../host-script-languages.md`](../../../../host-script-languages.md).
