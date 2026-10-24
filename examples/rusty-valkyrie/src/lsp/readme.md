# lsp src

Language Server Protocol implementation for Valkyrie.

## Responsibilities

- Organize protocol entry, handlers, state cache, and diagnostic conversion.
- Reuse compiler facts to power IDE queries.

## Forbidden

- Do not promote LSP-only temporary structures into compiler main representations.
- Do not duplicate the full compile pipeline inside LSP handlers.
