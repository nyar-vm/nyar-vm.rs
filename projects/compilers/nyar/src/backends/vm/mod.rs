//! NyarVM backend suspend consumption strategy.

use serde::{Deserialize, Serialize};

use crate::planning::SuspendConsumptionModel;

/// NyarVM suspend consumption strategy (dual-path).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum VmSuspendStrategy {
    /// First-class continuation runtime (`SuspendRuntimePayload`).
    #[default]
    FirstClass,
    /// Explicit state-machine payload (`ControlFlowPayload`).
    StateMachine,
}

impl VmSuspendStrategy {
    /// Map to generic suspend consumption model.
    pub fn consumption_model(self) -> SuspendConsumptionModel {
        match self {
            Self::FirstClass => SuspendConsumptionModel::FirstClass,
            Self::StateMachine => SuspendConsumptionModel::StateMachine,
        }
    }
}
