# selection

Backend selector.

## Responsibilities

- Choose a backend from `PartitionBackendRequirement` produced by `planning` and priority rules.
- Selection only; no compile-main-chain semantic judgments.

## Design constraints

- Do not route wrong inputs to a backend just because it “runs more often today.”
- The selector consumes finished backend requirement objects; it does not re-read optimizer state on its own.
- The selector does not replace `validate()`; route boundary checks remain inside backends.
