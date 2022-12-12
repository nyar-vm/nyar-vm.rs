# c src

`C` root-level guest package (alongside `valkyrie`, not inside it).

## Duties
- Reuse `std-data::text::c` parse/AST models.
- Concrete `CModule` / `CSemanticBridge`, plus `interpret` tree-walker (language-package primary evaluator).
- `legacy-vm` only thin-adapts (`evaluator/c.rs` → here); `legend` only assembles fixtures.
## Demo subset
- Types: `int` / `void` / `char` / `float` / `double`
- Function definitions/calls, `main` entry
- Locals/globals, assignment
- `if` / `else` / `while` / `for` / `break` / `continue` / `return`
- Arithmetic/comparison, `&&` / `||` / `!`
- `printf` (`%d` / `%i` / `%f` / `%s` / `%c` / `%%`)
- `#include` lines ignored
