//! Witness table layout and dispatch metadata for WASM linear memory.
//!
//! Trait objects are represented as `(data_ptr, witness_table_ptr)`, and dynamic
//! dispatch resolves the method slot through `call_indirect`.

/// A single method slot inside a witness table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessMethodSlot {
    /// Method index inside the trait witness table.
    pub method_index: u32,
    /// Function index inside the generated WASM module.
    pub function_index: u32,
}

/// Planned witness-table layout in linear memory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WitnessTableLayout {
    /// Concrete type name from `imply Target: Trait`.
    pub type_name: String,
    /// Trait name.
    pub trait_name: String,
    /// Ordered method slots.
    pub methods: Vec<WitnessMethodSlot>,
    /// Memory offset assigned by the backend.
    pub memory_offset: u32,
    /// Associated type binding count stored after the slots.
    pub associated_type_count: u32,
}

/// Trait fat pointer representation in WASM linear memory.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WasmTraitFatPointer {
    /// Pointer to the concrete object payload.
    pub data_ptr: u32,
    /// Pointer to the witness table.
    pub witness_ptr: u32,
}

/// Plan a witness-table layout for `imply Type: Trait`.
pub fn plan_witness_table_layout(type_name: &str, trait_name: &str, method_names: &[&str]) -> WitnessTableLayout {
    WitnessTableLayout {
        type_name: type_name.to_string(),
        trait_name: trait_name.to_string(),
        methods: method_names
            .iter()
            .enumerate()
            .map(|(index, _)| WitnessMethodSlot { method_index: index as u32, function_index: index as u32 })
            .collect(),
        memory_offset: 0,
        associated_type_count: 0,
    }
}

/// Resolve the function index for a witness call.
pub fn resolve_witness_call(layout: &WitnessTableLayout, method_index: u32) -> Option<u32> {
    layout.methods.iter().find(|slot| slot.method_index == method_index).map(|slot| slot.function_index)
}

/// Materialize witness-table bytes as little-endian `u32` slots.
pub fn materialize_witness_bytes(layout: &WitnessTableLayout) -> Vec<u8> {
    let mut bytes = Vec::new();
    for slot in &layout.methods {
        bytes.extend_from_slice(&slot.function_index.to_le_bytes());
    }
    bytes
}
