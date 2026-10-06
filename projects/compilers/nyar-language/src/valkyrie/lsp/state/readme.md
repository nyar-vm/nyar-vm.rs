# lsp state

LSP-side caches, document state, and query helper structures.

## Responsibilities

- Track open documents, version stamps, and incremental invalidation.
- Cache compiler snapshots reused by multiple handlers.

## Forbidden

- Do not store alternate semantic representations that diverge from the compiler main chain.
- Do not duplicate full pipeline state when a query snapshot suffices.
