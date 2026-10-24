#!/usr/bin/env pwsh
$name = "victory"
function Greet($who) {
  Write-Host $who
}
$x = 0
while ($x -lt 1) {
  $x = $x + 1
}
if ($x -eq 1) {
  for ($i = 0; $i -lt 1; $i = $i + 1) {
    Greet $name
  }
}
