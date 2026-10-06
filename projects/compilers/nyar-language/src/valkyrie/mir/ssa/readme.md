# mir ssa

Finer-grained home of **MIR (SSA)** structures and algorithms.

## Responsibilities

- Host SSA-specific structures and algorithms.
- Keep block parameters, value references, and control-flow conventions defined in one place.

## Forbidden

- No target-specific `BackendPrivatePlan` or runtime logic mixed in here.
- Do not let the `ssa` subtree become a second compile mega-entry.

## Call fact ownership

Call contracts must flow from resolved declarations, not be re-parsed by MIR or backends:

```text
source closure → declaration resolve & instantiation → typed HIR → Semantic MIR → verify → Canonical → representation plan → target-private plan
```

- Declaration resolution owns callable declaration, owner, generic substitution, and full signature.
- Overload selection must keep the chosen declaration and its instance identity; candidate names and instantiated signatures cannot replace declaration identity.
- Static vs instance methods are determined only by the declaration's `self` parameter; no `self` means static.
- Semantic MIR ordinary calls, operators, and collection methods reference resolved instances uniformly—no path truncation, owner guessing, or receiver patching.
- Function-value calls reference SSA values and their resolved function types; they must not masquerade as named static functions.
- Dependency closure walks instance identity only; missing declaration, type substitution, body, or formal import contract fails explicitly.
- Canonical, partitioning, and target-private plans consume the same identity; public ABI names are used only at interop boundaries.
- When a target capability is unimplemented, reject—do not restore Symbol dispatch, type-default fallbacks, or host-side compile substitution.

## Current gaps (evidence, not allowed contracts)

These are unfinished items in current sources:

1. **`hir/overload.rs`** — overload selection returns full candidates; call lowering no longer re-finds candidates by name, and singleton/extractor matching keeps declaration facts. `HirResolvedCall` still describes results by name/signature without full declaration + substitution identity; that gap is not closed.
2. **`expr_helpers.rs`** — `lower_callee_operand` no longer truncates resolved operator paths, but still synthesizes static `Symbol`s from expression spelling when no resolution exists; that must fail at the call-resolution boundary.
3. **`compile_pipeline/link.rs`** — builds dependency pools by function symbol; `mod.rs` `callable_identity_table` sorts/deduplicates/numbers names, then `resolve_callable_operands` rewrites calls. Late numbering is not declaration identity carried from the front.
4. **`compile_pipeline/canonical.rs`** — generates declarations from instance numbers, type-checks, then uses fixed monomorphic substitution. That does not prove generic call instances and evidence environments are wired end-to-end.

Remediation is one vertical feature: establish a single front-end declaration/instance table, migrate HIR, dependency closure, MIR, Canonical, and all consumers, then delete name-binding APIs. Do not keep old resolution branches, dual success types, or transition flags for a new table. Migrators may only mechanically rewrite files and references—they must not invent semantics.

Acceptance must cover same name different owner, same declaration different generic instances, static/instance calls, operators, function values, collection calls, and formal imports. Changing ABI display names must not change internal binding; missing identity, ambiguous declarations, wrong arity, and uninstantiated types must fail at semantic boundaries. Source-driven tests must use formal `Compiler` entry; hand-built MIR fixtures prove local contracts only, not full compile-flow success.
