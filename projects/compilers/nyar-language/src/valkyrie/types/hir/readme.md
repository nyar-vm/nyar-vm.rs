# hir

HIR structure definitions.

## Responsibilities

- Represent high-level program shape after basic semantic organization.
- Provide stable input for MIR (SSA) lowering and type-related analysis.

## Forbidden

- No target-specific encoding here.
- Do not turn HIR into an omnibus carrier across all compile phases.
