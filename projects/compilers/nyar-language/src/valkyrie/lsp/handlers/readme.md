# lsp handlers

LSP request handlers.

## Responsibilities

- Map protocol requests to compiler queries and document state.
- Keep each handler focused on a single capability.

## Forbidden

- Do not rebuild compile semantics inside handlers.
- Do not grow protocol routing into unmaintainable mega-entry functions.
