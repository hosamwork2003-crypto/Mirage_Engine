// ===================================================================
// mirage-mkr-core/src/streaming_pipeline.rs
// PURPOSE: Structural streaming activation and transitions.
// ===================================================================

use crate::region_identity::StructuralRegionId;
use crate::region_runtime::{StructuralRegionRuntime, RegionResidencyState, RegionStreamingState};
use mirage_core::invariants::DeterministicInvariantViolation;
use std::collections::BTreeMap;
use serde::{Serialize, Deserialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum StreamingPhase {
    RegionActivation = 1,
    ResidencyPreparation = 2,
    StructuralStreaming = 3,
    ContinuityPropagation = 4,
    Stabilization = 5,
    SnapshotSealing = 6,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct StructuralStreamingDescriptor {
    pub region_id: StructuralRegionId,
    pub phase: StreamingPhase,
    pub sequence_index: u64,
    pub target_residency: RegionResidencyState,
}

impl Eq for StructuralStreamingDescriptor {}

impl Ord for StructuralStreamingDescriptor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.sequence_index.cmp(&other.sequence_index)
            .then_with(|| self.region_id.cmp(&other.region_id))
            .then_with(|| self.phase.cmp(&other.phase))
            .then_with(|| self.target_residency.cmp(&other.target_residency))
    }
}

impl PartialOrd for StructuralStreamingDescriptor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructuralStreamingSequence {
    pub descriptors: Vec<StructuralStreamingDescriptor>,
}

impl StructuralStreamingSequence {
    pub fn new(descriptors: Vec<StructuralStreamingDescriptor>) -> Self {
        Self { descriptors }
    }

    /// Sort descriptors stable-deterministically by sequence_index.
    pub fn stable_sort(&mut self) {
        self.descriptors.sort();
    }

    /// Canonicalize the streaming sequence.
    pub fn canonicalize(&mut self) {
        self.stable_sort();
    }

    /// Apply streaming transitions sequentially without dynamic reordering or async.
    pub fn apply_streaming_sequence(
        &self,
        runtimes: &mut BTreeMap<StructuralRegionId, StructuralRegionRuntime>,
    ) -> Result<(), DeterministicInvariantViolation> {
        for desc in &self.descriptors {
            let runtime = runtimes.get_mut(&desc.region_id).ok_or(
                DeterministicInvariantViolation {
                    invariant_name: "missing_streaming_region",
                    subsystem: "streaming_pipeline",
                }
            )?;

            match desc.phase {
                StreamingPhase::RegionActivation => {
                    runtime.activate_region()?;
                }
                StreamingPhase::ResidencyPreparation => {
                    if runtime.residency == RegionResidencyState::Evicted {
                        runtime.residency = RegionResidencyState::Loading;
                    }
                }
                StreamingPhase::StructuralStreaming => {
                    runtime.streaming = RegionStreamingState::Streaming;
                    runtime.residency = desc.target_residency;
                }
                StreamingPhase::ContinuityPropagation => {
                    runtime.streaming = RegionStreamingState::Completed;
                }
                StreamingPhase::Stabilization => {
                    runtime.stabilize_region()?;
                }
                StreamingPhase::SnapshotSealing => {
                    // Marker phase: snapshot sealing is handled during world snapshot finalization.
                }
            }
        }
        Ok(())
    }
}
