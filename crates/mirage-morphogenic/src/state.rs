use crate::continuity::{StructuralContinuityField, ContinuitySnapshot};

/// Replay-grade structural provenance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StructuralProvenance {
    pub originating_tick: u64,
    pub topology_generation: u64,
    pub lane_sequence_index: u64,
    pub continuity_epoch: u64,
}

impl StructuralProvenance {
    pub fn new(originating_tick: u64, topology_generation: u64, lane_sequence_index: u64, continuity_epoch: u64) -> Self {
        Self { originating_tick, topology_generation, lane_sequence_index, continuity_epoch }
    }
}

/// Deterministic structural metadata (non-authoritative).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct StructuralState {
    pub stability_score: f32,
    pub propagation_pressure: f32,
    pub continuity_factor: f32,
}

impl StructuralState {
    pub fn new(stability_score: f32, propagation_pressure: f32, continuity_factor: f32) -> Self {
        let mut s = Self { stability_score, propagation_pressure, continuity_factor };
        s.normalize();
        s
    }

    /// Clamp to [0.0, 1.0]
    pub fn normalize(&mut self) {
        self.stability_score = self.stability_score.clamp(0.0, 1.0);
        self.propagation_pressure = self.propagation_pressure.clamp(0.0, 1.0);
        self.continuity_factor = self.continuity_factor.clamp(0.0, 1.0);
    }

    /// Apply deterministic decay to a runtime field (index-ordered).
    pub fn apply_decay_to_field(&self, field: &mut StructuralContinuityField) {
        field.apply_decay(self.continuity_factor);
    }

    /// Apply deterministic decay to an immutable snapshot (produces mutated snapshot).
    pub fn apply_decay_to_snapshot(&self, mut snapshot: ContinuitySnapshot) -> ContinuitySnapshot {
        for v in &mut snapshot.continuity {
            *v = (*v * self.continuity_factor).clamp(0.0, 1.0);
        }
        snapshot
    }
}

impl Default for StructuralState {
    fn default() -> Self {
        Self { stability_score: 1.0, propagation_pressure: 0.0, continuity_factor: 1.0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::continuity::StructuralContinuityField;

    #[test]
    fn decay_snapshot() {
        let mut f = StructuralContinuityField::new(2);
        f.set(0, 1.0);
        let snap = f.snapshot(0);
        let s = StructuralState::new(1.0, 0.0, 0.5);
        let out = s.apply_decay_to_snapshot(snap);
        assert!((out.continuity[0] - 0.5).abs() < 1e-6);
    }
}