// ===================================================================
// mirage-mkr-core/src/region_runtime.rs
// PURPOSE: Region runtime states and transition validation.
// ===================================================================

use crate::region_identity::RegionRuntimeIdentity;
use mirage_core::invariants::DeterministicInvariantViolation;
use serde::{Serialize, Deserialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum RegionActivationState {
    Inactive,
    Activating,
    Active,
    Deactivating,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum RegionResidencyState {
    Evicted,
    Loading,
    Resident,
    Stabilizing,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum RegionStreamingState {
    Idle,
    Streaming,
    Completed,
    Failed,
}

#[derive(Clone, Debug, Eq, PartialEq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct StructuralRegionRuntime {
    pub identity: RegionRuntimeIdentity,
    pub activation: RegionActivationState,
    pub residency: RegionResidencyState,
    pub streaming: RegionStreamingState,
}

impl StructuralRegionRuntime {
    pub fn new(identity: RegionRuntimeIdentity) -> Self {
        Self {
            identity,
            activation: RegionActivationState::Inactive,
            residency: RegionResidencyState::Evicted,
            streaming: RegionStreamingState::Idle,
        }
    }

    /// Transition activation state monotonically towards Active.
    pub fn activate_region(&mut self) -> Result<(), DeterministicInvariantViolation> {
        match self.activation {
            RegionActivationState::Inactive => {
                self.activation = RegionActivationState::Activating;
                Ok(())
            }
            RegionActivationState::Activating => {
                self.activation = RegionActivationState::Active;
                Ok(())
            }
            RegionActivationState::Active => {
                Err(DeterministicInvariantViolation {
                    invariant_name: "duplicate_activation_transition",
                    subsystem: "region_runtime",
                })
            }
            RegionActivationState::Deactivating => {
                Err(DeterministicInvariantViolation {
                    invariant_name: "activation_state_regression",
                    subsystem: "region_runtime",
                })
            }
        }
    }

    /// Transition activation state monotonically towards Inactive.
    pub fn deactivate_region(&mut self) -> Result<(), DeterministicInvariantViolation> {
        match self.activation {
            RegionActivationState::Active => {
                self.activation = RegionActivationState::Deactivating;
                Ok(())
            }
            RegionActivationState::Deactivating => {
                self.activation = RegionActivationState::Inactive;
                Ok(())
            }
            RegionActivationState::Inactive => {
                Err(DeterministicInvariantViolation {
                    invariant_name: "duplicate_deactivation_transition",
                    subsystem: "region_runtime",
                })
            }
            RegionActivationState::Activating => {
                Err(DeterministicInvariantViolation {
                    invariant_name: "deactivation_state_regression",
                    subsystem: "region_runtime",
                })
            }
        }
    }

    /// Transition residency state monotonically towards Stabilizing.
    pub fn stabilize_region(&mut self) -> Result<(), DeterministicInvariantViolation> {
        match self.residency {
            RegionResidencyState::Loading => {
                self.residency = RegionResidencyState::Resident;
                Ok(())
            }
            RegionResidencyState::Resident => {
                self.residency = RegionResidencyState::Stabilizing;
                Ok(())
            }
            RegionResidencyState::Stabilizing => {
                Err(DeterministicInvariantViolation {
                    invariant_name: "duplicate_stabilization",
                    subsystem: "region_runtime",
                })
            }
            RegionResidencyState::Evicted => {
                Err(DeterministicInvariantViolation {
                    invariant_name: "stabilize_on_evicted_region",
                    subsystem: "region_runtime",
                })
            }
        }
    }

    /// Get a canonicalized clone of the current runtime state.
    pub fn canonical_runtime_state(&self) -> Self {
        self.clone()
    }
}
