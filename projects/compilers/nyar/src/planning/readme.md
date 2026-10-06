# planning

Neutral orchestration plans owned by `nyar`.

## Responsibilities

- Accept only validated `CanonicalProgram`; standalone frontend facts are not a success entry.
- Combine targets, lanes, capabilities, and runtime requirements into `ArtifactPartitionPlan`.
- Emit closed `PartitionBackendRequirement` values from partition plans for the selection layer.
- Carry host boundaries and reference-object management strategy in partition plans so GC/RC policy does not leak into backend families or launcher details.
- Provide stable entry for backend selection, downstream lowering, and packaging.
- Partition entry, operation roots, and fragment views use `ItemInstanceId`, consuming Canonical fragment contracts directly.
- Equational optimization name views carry no entry, import, or call edges; name equality must not rebind callable roots.

## Forbidden

- No direct dependency on concrete frontend types.
- No reinterpretation of language-level semantics.
- No target container encoding responsibilities.
- Do not treat equational name lists as function closures; assembly must reject identities inconsistent with Canonical.
