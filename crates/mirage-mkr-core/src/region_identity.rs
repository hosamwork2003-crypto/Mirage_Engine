// ===================================================================
// mirage-mkr-core/src/region_identity.rs
// PURPOSE: Region runtime identity representation.
// ===================================================================

use serde::{Serialize, Deserialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct StructuralRegionId(pub u64);

impl StructuralRegionId {
    pub fn new(id: u64) -> Self {
        Self(id)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct StructuralRegionGeneration(pub u64);

impl StructuralRegionGeneration {
    pub fn new(generation: u64) -> Self {
        Self(generation)
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct RegionRuntimeIdentity {
    pub region_id: StructuralRegionId,
    pub generation: StructuralRegionGeneration,
    pub originating_tick: u64,
    pub continuity_epoch: u64,
    pub replay_sequence: u64,
}

impl RegionRuntimeIdentity {
    pub fn new(
        region_id: StructuralRegionId,
        generation: StructuralRegionGeneration,
        originating_tick: u64,
        continuity_epoch: u64,
        replay_sequence: u64,
    ) -> Self {
        Self {
            region_id,
            generation,
            originating_tick,
            continuity_epoch,
            replay_sequence,
        }
    }
}
