//! Public frontend-facing contract facade.

pub mod control_flow_payload;
pub mod executable;
pub mod gpu_fragment_planning;
pub mod nyar_type;
pub mod planning;

pub use control_flow_payload::{ProtocolDiagnostic, validate_future_protocol, witness_bindings_for_effect_with_diagnostics};

pub use executable::{mir_function_to_executable, mir_functions_to_executable_map};

pub use nyar_type::{
    ConcretizeError, concretize_mir_function_types, concretize_mir_function_types_lossy, concretize_type, concretize_type_lossy,
};

pub use planning::{
    FrontendNeutralPlan, NyarPlanningContract, hir_module_to_analysis_artifact, hir_module_to_frontend_neutral_plan,
    hir_module_to_object_algebraic_program, hir_module_to_program_facts,
};
