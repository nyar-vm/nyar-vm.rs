# Valkyrie LSP Diagnostic Codes — Naming (Lint)

Naming violations are **lint warnings**. They do not block parsing or compilation.

| Code | Severity | Message | Quick fix hint |
|------|----------|---------|----------------|
| `E0301` | Warning | `Name '{name}' should be snake_case` | Rename identifier to snake_case |
| `E0302` | Warning | `Name '{name}' should be snake_case` | Rename AWSL template binding (`:prop` / `@event`) |

## Lint scope

- **E0301**: `let` bindings, `micro` / `function` / `method` declarations, and parameters in `.v`, `.vx`, and AWSL `<script>` blocks.
- **E0302**: AWSL template bindings validated against the component ABI index.

## Authority

Lint rules are enforced in the LSP / IDE layer via `std_data::text::valkyrie::naming::validate_snake_case` and `std_data::text::awsl::is_snake_case`. The parser and formatter do not reject non-`snake_case` identifiers. IDEs and VSCode consume these lint diagnostics; do not duplicate regex rules client-side.
