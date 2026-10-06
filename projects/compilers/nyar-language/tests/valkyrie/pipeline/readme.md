# compiler tests pipeline

Validates parser output flowing into `HIR / MIR / LIR` without inventing a unified pseudo-IR.

## Focus

- `row` must close to member calls before entering MIR, not remain open evidence.
- Open `trait` / `effect` dispatch must not be disguised as static calls too early.
- After `ArtifactPartitionPlan`, inputs must be target-specific, not a cross-target compatibility shell.
