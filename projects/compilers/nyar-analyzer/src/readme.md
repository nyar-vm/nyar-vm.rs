# nyar-analyzer (src)

Frontend-neutral analysis contracts for the Nyar platform (rustdoc source for the crate root).

## Responsibilities

- Consume program facts already closed by downstream frontends.
- Express entries, imports/exports, runtime requirements, and capability tags in a neutral shape.
- Feed unified input into `nyar` partitioning, lane selection, and backend planning.
- Provide the syntax-highlighting platform layer (`highlight`: `HighlightKind` / `Highlighter` trait), similar to a JetBrains-style platform split.

## Forbidden

- No dependency on concrete guest-language parser crates.
- No language-specific `AST / HIR / MIR / LIR` owned here.
- No concrete language highlighters (implemented as language plugins in `nyar-language`).
- No target container encoding or artifact packaging.
