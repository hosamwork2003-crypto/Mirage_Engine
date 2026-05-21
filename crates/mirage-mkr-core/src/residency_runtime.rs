// ===================================================================
// mirage-mkr-core/src/residency_runtime.rs
// PURPOSE: Regional residency transitions and stabilization.
// ===================================================================

use crate::region_identity::StructuralRegionId;
use crate::region_runtime::{StructuralRegionRuntime, RegionResidencyState};
use mirage_core::invariants::DeterministicInvariantViolation;
use std::collections::BTreeMap;
use serde::{Serialize, Deserialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum ResidencyStabilizationState {
    Unstable,
    Stabilizing,
    Stable,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructuralResidencyDescriptor {
    pub region_id: StructuralRegionId,
    pub target_state: RegionResidencyState,
    pub stabilization: ResidencyStabilizationState,
    pub sequence_index: u64,
}

impl Eq for StructuralResidencyDescriptor {}

impl Ord for StructuralResidencyDescriptor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sequence_index.cmp(&other.sequence_index)
            .then_with(|| self.region_id.cmp(&other.region_id))
            .then_with(|| self.target_state.cmp(&other.target_state))
            .then_with(|| self.stabilization.cmp(&other.stabilization))
    }
}

impl PartialOrd for StructuralResidencyDescriptor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructuralResidencySequence {
    pub descriptors: Vec<StructuralResidencyDescriptor>,
}

impl StructuralResidencySequence {
    pub fn new(descriptors: Vec<StructuralResidencyDescriptor>) -> Self {
        Self { descriptors }
    }

    /// Stable-sort the residency sequence deterministically.
    pub fn stable_sort(&mut self) {
        self.descriptors.sort();
    }

    /// Canonicalize the residency sequence.
    pub fn canonicalize(&mut self) {
        self.stable_sort();
    }

    /// Apply residency transitions while strictly checking monotonicity.
    pub fn apply_residency_sequence(
        &self,
        runtimes: &mut BTreeMap<StructuralRegionId, StructuralRegionRuntime>,
    ) -> Result<(), DeterministicInvariantViolation> {
        for desc in &self.descriptors {
            let runtime = runtimes.get_mut(&desc.region_id).ok_or(
                DeterministicInvariantViolation {
                    invariant_name: "missing_residency_region",
                    subsystem: "residency_runtime",
                }
            )?;

            // Monotonicity check: cannot transition from Resident back to Loading or Evicted
            // unless sequence index allows eviction.
            if runtime.residency == RegionResidencyState::Resident
                && (desc.target_state == RegionResidencyState::Loading || desc.target_state == RegionResidencyState::Evicted)
            {
                // In strict world execution, we reject arbitrary residency regressions.
                return Err(DeterministicInvariantViolation {
                    invariant_name: "residency_regression_violation",
                    subsystem: "residency_runtime",
                });
            }

            runtime.residency = desc.target_state;
            if desc.stabilization == ResidencyStabilizationState::Stable {
                // If requested state is Stable, ensure we stabilize.
                if runtime.residency == RegionResidencyState::Resident {
                    runtime.residency = RegionResidencyState::Stabilizing;
                }
            }
        }
        Ok(())
    }
}
