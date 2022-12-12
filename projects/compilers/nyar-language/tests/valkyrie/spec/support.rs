#![allow(dead_code)]

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecLayer {
    Row,
    Trait,
    Nominal,
    Effect,
    Pipeline,
    BackendBoundary,
    Diagnostics,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpecStatus {
    Planned,
    KnownGap,
    Implemented,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SpecCase {
    pub name: &'static str,
    pub layer: SpecLayer,
    pub status: SpecStatus,
    pub rule: &'static str,
}

impl SpecCase {
    pub const fn planned(name: &'static str, layer: SpecLayer, rule: &'static str) -> Self {
        Self { name, layer, status: SpecStatus::Planned, rule }
    }

    pub const fn known_gap(name: &'static str, layer: SpecLayer, rule: &'static str) -> Self {
        Self { name, layer, status: SpecStatus::KnownGap, rule }
    }

    pub const fn implemented(name: &'static str, layer: SpecLayer, rule: &'static str) -> Self {
        Self { name, layer, status: SpecStatus::Implemented, rule }
    }
}
