# mir

Home of **MIR (SSA)** in the Valkyrie compiler.

## Responsibilities

- Express target-neutral mid-level semantics in SSA form.
- Model values, block parameters, instruction results, and terminators explicitly.
- Provide stable input for optimization and downstream artifact partitioning.
- Control-flow unification work is tracked at the workspace level (see [`projects/readme.md`](../../../../readme.md)).

## Forbidden

- No statement-list pseudo-MIR rollback.
- No CLR / JVM / WASM / native-specific opcodes in this layer.
- Do not evolve MIR into another omnibus bus type.

## Analysis entry points vs production

- `compile_source_to_mir` consumes the formal HIR expansion and call validation from `compile_source`; it does not parse source on its own or bypass the HIR contract.
- `lower_root_to_mir` accepts AST input but still validates the HIR semantic contract before lowering and control-flow verification.
- Analysis APIs return stage data only; they do not prove dependency closure, canonical validation, representation planning, or artifact success. Production artifact entry consumes ordered source groups from the Resolver; the Compiler performs dependency linking and all later stages.
- Single-source→`CompiledProgram` helpers and HIR-direct producers exist for unit tests only. There is no production method that emits `CompiledProgram` from a file path or prebuilt dependency HIR export.
