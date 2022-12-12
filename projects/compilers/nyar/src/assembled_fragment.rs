//! Driver-agnostic assembled fragment payload shared by frontends and the driver.

use std::collections::{BTreeMap, BTreeSet};

use nyar_optimizer::TheoryBundle;
use nyar_types::{
    AggregateLayoutPlan, CapabilityTag, ExecutableFunction, ExternalCallEdge, ExternalImportLink, FlagsLayout, Identifier, InternalCallEdge,
    QualifiedName, SingletonInstancePlan, SumTypeLayout, WitnessCallEdge, WitnessSubmission,
};

use crate::planning::{ControlFlowPayload, SuspendRuntimePayload};

/// Fragment payload produced by a language frontend after MIR lowering.
///
/// The driver wraps this into a backend [`FragmentSubmission`]-equivalent with an
/// executable provider. This type is intentionally free of driver crate types.
#[derive(Debug, Clone)]
pub struct AssembledFragment {
    /// Logical module name.
    pub module_name: String,
    /// Semantic fragment identifier.
    pub fragment_id: Identifier,
    /// Exported stable operations.
    pub exported_operations: Vec<QualifiedName>,
    /// Required capability constraints.
    pub required_capabilities: Vec<CapabilityTag>,
    /// Theory bundle for this fragment.
    pub theory_bundle: TheoryBundle,
    /// Interpretive entry operation, if any.
    pub entry_operation: Option<QualifiedName>,
    /// Stable operation → external import link map.
    pub external_import_links: BTreeMap<QualifiedName, ExternalImportLink>,
    /// Resolved external call edges.
    pub external_call_edges: Vec<ExternalCallEdge>,
    /// Resolved internal call edges.
    pub internal_call_edges: Vec<InternalCallEdge>,
    /// Operations that return string literals only.
    pub operation_literal_returns: BTreeMap<QualifiedName, String>,
    /// Operations that return `unit`.
    pub operation_void_returns: BTreeSet<QualifiedName>,
    /// Named trait witness table payloads.
    pub witness_tables: Vec<WitnessSubmission>,
    /// Entry witness dynamic call edges.
    pub witness_calls: Vec<WitnessCallEdge>,
    /// Suspend control-flow rewrite payload (state-machine backends).
    pub control_flow: Option<ControlFlowPayload>,
    /// First-class suspend runtime payload.
    pub suspend_runtime: Option<SuspendRuntimePayload>,
    /// Aggregate memory layout plan.
    pub aggregate_layouts: AggregateLayoutPlan,
    /// Sum type discriminant layouts.
    pub sum_types: Vec<SumTypeLayout>,
    /// Flags bitmask layouts.
    pub flags_types: Vec<FlagsLayout>,
    // DELETED (ADR 0010/0011): intrinsics / nullable_intrinsics / nullable_try_calls / nullable_bool_profiles.
    /// Reachable executable functions for this partition.
    pub executable_functions: BTreeMap<QualifiedName, ExecutableFunction>,
    /// Singleton global instance initialization plans.
    pub singleton_instances: Vec<SingletonInstancePlan>,
}
