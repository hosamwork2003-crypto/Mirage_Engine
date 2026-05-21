// ===================================================================
// mirage-renderer/src/residency_state.rs
// PURPOSE: Passive tracking of GPU residency metadata.
// ===================================================================

use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub enum ResidencyClassification {
    Evicted,
    PendingLoad,
    Resident,
    PendingEviction,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResidencySnapshot {
    pub tick: u64,
    pub classifications: BTreeMap<u64, ResidencyClassification>,
}

#[derive(Clone, Debug)]
pub struct RendererResidencyState {
    pub classifications: BTreeMap<u64, ResidencyClassification>,
    pub residency_epoch: u64,
}

impl Default for RendererResidencyState {
    fn default() -> Self {
        Self::new()
    }
}

impl RendererResidencyState {
    pub fn new() -> Self {
        Self {
            classifications: BTreeMap::new(),
            residency_epoch: 0,
        }
    }

    /// Passive update of residency classification metadata.
    pub fn update_metadata(&mut self, region_id: u64, class: ResidencyClassification) {
        self.classifications.insert(region_id, class);
    }

    /// Create a passive residency snapshot for visualization or stats.
    pub fn create_snapshot(&self, tick: u64) -> ResidencySnapshot {
        ResidencySnapshot {
            tick,
            classifications: self.classifications.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residency_state_determinism() {
        let mut state = RendererResidencyState::new();
        // Insert out of order
        state.update_metadata(10, ResidencyClassification::Resident);
        state.update_metadata(2, ResidencyClassification::Evicted);
        state.update_metadata(5, ResidencyClassification::PendingLoad);

        let snap = state.create_snapshot(42);
        assert_eq!(snap.tick, 42);

        // Verify keys are strictly sorted in BTreeMap (2, 5, 10)
        let keys: Vec<u64> = snap.classifications.keys().copied().collect();
        assert_eq!(keys, vec![2, 5, 10]);

        // Verify values
        assert_eq!(snap.classifications[&2], ResidencyClassification::Evicted);
        assert_eq!(snap.classifications[&5], ResidencyClassification::PendingLoad);
        assert_eq!(snap.classifications[&10], ResidencyClassification::Resident);
    }

    #[test]
    fn residency_snapshot_equivalence() {
        let mut state_a = RendererResidencyState::new();
        let mut state_b = RendererResidencyState::new();

        // Perform updates in different orders
        state_a.update_metadata(10, ResidencyClassification::Resident);
        state_a.update_metadata(5, ResidencyClassification::PendingEviction);

        state_b.update_metadata(5, ResidencyClassification::PendingEviction);
        state_b.update_metadata(10, ResidencyClassification::Resident);

        let snap_a = state_a.create_snapshot(100);
        let snap_b = state_b.create_snapshot(100);

        // Verify exact equivalence of snapshots
        assert_eq!(snap_a, snap_b);
    }
}
