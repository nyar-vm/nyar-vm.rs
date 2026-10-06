# lanes

Target lanes after artifact partitioning.

## Responsibilities

- Consume each partition from `ArtifactPartitionPlan`.
- Lower partition results to the backend input that route actually consumes.
- Maintain “each route is responsible only for itself.”

## Per-lane lowering

- **CPU / VM** lane may lower to `NyarIR` or an equivalent low-level form.
- **CLR** lane lowers to `ClrImage`, metadata, `MSIL`, `PE`, and related inputs.
- **JVM** lane lowers to `ClassFile`-style input.
- **WASM** lane lowers to structured control flow and section models.
- **GPU / Shader** lane lowers directly to target-specific models, not via a CPU-oriented compatibility shell.

## Forbidden

- No trait resolution.
- No row member selection.
- No nominal subtype decisions.
- No effect handler selection.
- Do not disguise open witness dispatch as ordinary static calls during lowering.
