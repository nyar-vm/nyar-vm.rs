# packaging

Target lanes and artifact delivery protocol.

## Responsibilities

- Describe `OutputSpec`, `ArtifactSet`, and `TargetLane`.
- Unify orchestration protocol, not physical IR.

## Architecture

- Bridges `ArtifactPartitionPlan → target lane → backend input → artifact set`.
- Packaging answers “which route” and “what artifacts,” not “what does this call mean in the language.”
- Each lane must receive low-level input the target actually supports; invalid input must be rejected before backends run.

## Alignment with assembler docs

- Corresponds to the layer legacy docs called `Assembler`: select, validate, invoke backend, deliver artifacts.
- Does not perform trait resolve, row decisions, effect handler selection, or rewrite open witness into static functions.
- `OutputSpec` and `ArtifactSet` describe delivery results only; they are not carriers of language truth.
