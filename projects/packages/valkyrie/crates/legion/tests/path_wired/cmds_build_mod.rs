//! Migrated from `src/cmds/build/mod.rs` (`tests`).

use super::*;

use nyar::{BinaryArch, BinaryFlavor, BinaryTarget, HostProjectionBoundary, TargetFamily, TargetLane};
use nyar_emitter::{ArtifactPartition, DriverCompileReport};

fn partition_artifact_name(base_name: &str, partition: &ArtifactPartition, partition_count: usize) -> String {
    if partition_count <= 1 {
        return base_name.to_string();
    }

    let suffix = partition
        .name
        .rsplit("::")
        .next()
        .unwrap_or(partition.name.as_str())
        .chars()
        .map(|ch| if ch.is_ascii_alphanumeric() || ch == '-' || ch == '_' { ch } else { '_' })
        .collect::<String>();
    format!("{base_name}__{suffix}")
}

fn merge_partition_reports(reports: Vec<(String, DriverCompileReport)>, primary_partition_name: Option<&str>) -> DriverCompileReport {
    let mut merged = DriverCompileReport::default();
    let mut primary_contracts = Vec::new();
    let mut secondary_contracts = Vec::new();
    for (partition_name, report) in reports {
        merged.artifacts.artifacts.extend(report.artifacts.artifacts);
        if merged.entry_symbol.is_none() {
            merged.entry_symbol = report.entry_symbol.clone();
        }
        if primary_partition_name == Some(partition_name.as_str()) {
            primary_contracts.extend(report.run_contracts);
        }
        else {
            secondary_contracts.extend(report.run_contracts);
        }
    }
    merged.run_contracts = primary_contracts;
    merged.run_contracts.extend(secondary_contracts);
    merged
}

fn demo_partition(name: &str) -> ArtifactPartition {
    ArtifactPartition {
        fragment: rusty_valkyrie::Identifier::new("functions"),
        entry_operation: None,
        backend_name: "wasm-binary".to_string(),
        interpreter: rusty_valkyrie::Identifier::new("wasm.module"),
        name: name.to_string(),
        exported_operations: Vec::new(),
        lane: TargetLane::Wasm,
        binary_target: BinaryTarget::new(TargetFamily::Wasm, BinaryArch::Any, BinaryFlavor::Native),
        input_kind: None,
        clr_suspend_strategy: nyar::ClrSuspendStrategy::default(),
        host_boundary: HostProjectionBoundary::WasmJsGlue,
        reference_management: rusty_valkyrie::ReferenceManagement::HostGc,
        capabilities: Vec::new(),
        runtime_requirements: Vec::new(),
    }
}

#[test]
fn partition_artifact_name_aligns_with_export_partition_suffix() {
    let partition = demo_partition("demo::export__unity_editor");
    assert_eq!(partition_artifact_name("valkyrie.unity", &partition, 2), "valkyrie.unity__export__unity_editor");
}

#[test]
fn partition_artifact_name_adds_dimension_suffix_for_multi_partition_plan() {
    let partition = demo_partition("demo::host-interop");
    assert_eq!(partition_artifact_name("demo", &partition, 3), "demo__host-interop");
    assert_eq!(partition_artifact_name("demo", &partition, 1), "demo");
}

#[test]
fn merge_partition_reports_prefers_suspend_partition_run_contract_when_entry_is_suspend() {
    let mut suspend = DriverCompileReport::default();
    suspend.run_contracts = vec![DriverRunContract {
        logical_entry: "main".to_string(),
        physical_entry: "demo__suspend.jar".to_string(),
        invocation: "java".to_string(),
        validate: "java -jar demo__suspend.jar".to_string(),
    }];

    let mut functions = DriverCompileReport::default();
    functions.run_contracts = vec![DriverRunContract {
        logical_entry: "helper".to_string(),
        physical_entry: "demo__functions.jar".to_string(),
        invocation: "java".to_string(),
        validate: "java -jar demo__functions.jar".to_string(),
    }];

    let merged = merge_partition_reports(
        vec![("demo::functions".to_string(), functions), ("demo::suspend".to_string(), suspend)],
        Some("demo::suspend"),
    );

    assert_eq!(merged.run_contracts.first().expect("run contract").physical_entry, "demo__suspend.jar");
}

#[test]
fn merge_partition_reports_prefers_primary_partition_run_contract() {
    let mut primary = DriverCompileReport::default();
    primary.run_contracts = vec![DriverRunContract {
        logical_entry: "main".to_string(),
        physical_entry: "demo__functions.mjs".to_string(),
        invocation: "node".to_string(),
        validate: "node demo__functions.mjs".to_string(),
    }];

    let mut secondary = DriverCompileReport::default();
    secondary.run_contracts = vec![DriverRunContract {
        logical_entry: "_start".to_string(),
        physical_entry: "demo__host-interop.wasm".to_string(),
        invocation: "wasmtime".to_string(),
        validate: "wasmtime demo__host-interop.wasm".to_string(),
    }];

    let merged = merge_partition_reports(
        vec![("demo::host-interop".to_string(), secondary), ("demo::functions".to_string(), primary)],
        Some("demo::functions"),
    );

    assert_eq!(merged.run_contracts.first().expect("run contract").physical_entry, "demo__functions.mjs");
}
