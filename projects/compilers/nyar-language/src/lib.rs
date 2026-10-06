#![doc = include_str!("../readme.md")]
#![warn(missing_docs)]

#[path = "valkyrie/formatter/mod.rs"]
pub mod formatter;
#[path = "valkyrie/printer/mod.rs"]
pub mod printer;
#[path = "valkyrie/text/mod.rs"]
pub mod text;

pub mod awsl;
pub mod bash;
pub mod c;
pub mod javascript;
pub mod lua;
pub mod msil;
#[path = "valkyrie/optimizer/mod.rs"]
pub mod optimizer;
pub mod pe;
pub mod powershell;
pub mod python;
pub mod tcl;
pub mod valkyrie;
pub mod von;
pub mod wat;
pub mod wit;

pub use bash::{BashModule, BashSemanticBridge, BashValue, evaluate_bash_script, evaluate_bash_source};
pub use c::{CModule, CSemanticBridge, CValue, evaluate_c_script, evaluate_c_source};
pub use javascript::JavascriptModule;
pub use lua::{LuaModule, LuaSemanticBridge, LuaValue, evaluate_lua_script, evaluate_lua_source, specialize_lua_into, specialize_lua_script};
pub use pe::ResidualSink;
pub use powershell::{PowerShellModule, PowerShellSemanticBridge, PowerShellValue, evaluate_powershell_script, evaluate_powershell_source};
pub use python::PythonModule;
pub use tcl::{TclModule, TclSemanticBridge, TclValue, evaluate_tcl_script, evaluate_tcl_source};

pub use nyar::{
    self, ArtifactKind, ArtifactPartitionPlan, ArtifactPolicy, ArtifactSet, CanonicalAbi, CanonicalArch, CanonicalSpecification,
    CanonicalTarget, CanonicalTargetParseError, CanonicalVendor, CompilationOptions, EntryPolicy, HostProjectionBoundary, ProgramFacts,
    PublishFormat, ReferenceManagement, RunnerFamily, RunnerSelector, TargetHostKind, TargetMode, TargetProfile, WrapStrategy,
};
pub use valkyrie::{
    compile_pipeline::compile_source_groups_to_artifacts,
    derive,
    frontend_contract::{
        ConcretizeError, concretize_mir_function_types, concretize_mir_function_types_lossy, concretize_type, concretize_type_lossy,
    },
    hir::{AstToHir, CaptureAnalyzer, CompilerSourceGroup, ValkyrieCompiler},
    mir,
    mir::{
        AggregateLayout, AggregateLayoutPlan, FieldLayout, FlagsLayout, LayoutId, MirBlock, MirBlockRef, MirConstant, MirDiagnostic,
        MirEffectKind, MirFunction, MirInstruction, MirLowerer, MirModule, MirOperand, MirOperation, MirStorageKind, MirTerminator, MirValue,
        MirValueOrigin, MirValueRef, SingletonInstancePlan, SumTypeLayout, SumVariantLayout, collect_singleton_instance_plans,
        compute_aggregate_layout_plan, layout_id_for_type, layout_key_for_nyar_type, layout_key_for_type, storage_kind_for_type,
    },
    module, type_checker, types,
    types::{Identifier, NamePath, QualifiedName, SourceID, SourceSpan},
    typing, validation,
    validation::ControlFlowScheduler,
};
pub(crate) use valkyrie::{frontend_contract, hir, symbols};
