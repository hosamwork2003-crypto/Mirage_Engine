use crate::state::{StructuralProvenance, StructuralState};
use crate::continuity::{ContinuitySnapshot, ContinuityEpoch};

/// Stable lane identity (allocator-independent).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct MorphogenicLaneId(pub u64);

/// Deterministic lane metadata (non-authoritative).
#[derive(Debug, Clone, PartialEq)]
pub struct MorphogenicLane {
    pub lane_id: MorphogenicLaneId,
    pub deterministic_sequence_index: u64,
    pub source_node: usize,
    pub target_node: usize,
    pub reinforcement_weight: f32,
}

impl MorphogenicLane {
    pub fn new(lane_id: MorphogenicLaneId, sequence_index: u64, source_node: usize, target_node: usize, reinforcement_weight: f32) -> Self {
        Self {
            lane_id,
            deterministic_sequence_index: sequence_index,
            source_node,
            target_node,
            reinforcement_weight: reinforcement_weight.clamp(0.0, 1.0),
        }
    }
}

/// Immutable descriptor that carries provenance and deterministic order.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralPropagationDescriptor {
    pub lane_id: MorphogenicLaneId,
    pub deterministic_sequence_index: u64,
    pub provenance: StructuralProvenance,
    pub target_node: usize,
    pub reinforcement_weight: f32,
}

/// Immutable, replay-safe propagation sequence.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralPropagationSequence {
    pub descriptors: Vec<StructuralPropagationDescriptor>,
}

impl StructuralPropagationSequence {
    pub fn new() -> Self {
        Self { descriptors: Vec::new() }
    }

    pub fn from_descriptors(mut desc: Vec<StructuralPropagationDescriptor>) -> Self {
        // Ensure deterministic ordering by sorting on sequence_index then lane_id.
        desc.sort_by(|a, b| {
            a.deterministic_sequence_index
                .cmp(&b.deterministic_sequence_index)
                .then_with(|| a.lane_id.0.cmp(&b.lane_id.0))
        });
        Self { descriptors: desc }
    }

    pub fn len(&self) -> usize { self.descriptors.len() }

    pub fn is_empty(&self) -> bool { self.descriptors.is_empty() }
}

/// Realization frame — ties a snapshot to when it was realized.
#[derive(Debug, Clone, PartialEq)]
pub struct StructuralRealizationFrame {
    pub tick: u64,
    pub epoch: ContinuityEpoch,
    pub snapshot: ContinuitySnapshot,
}

/// Non-authoritative realization sequence of frames.
#[derive(Debug, Clone, PartialEq)]
pub struct MorphogenicRealizationSequence {
    pub frames: Vec<StructuralRealizationFrame>,
}

impl MorphogenicRealizationSequence {
    pub fn new() -> Self { Self { frames: Vec::new() } }
}

/// Stateless deterministic applier: immutable input -> immutable output.
#[derive(Debug, Clone, Copy)]
pub struct MorphogenicRealizer;

impl MorphogenicRealizer {
    pub fn new() -> Self { Self }

    /// Realize the given immutable propagation sequence against a base snapshot and structural state.
    /// Returns a new ContinuitySnapshot (epoch = base_snapshot.epoch + 1).
    /// Deterministic ordering: descriptors are applied in ascending deterministic_sequence_index,
    /// tie-broken by lane_id.
    pub fn realize_sequence(
        _tick: u64,
        base_snapshot: &ContinuitySnapshot,
        sequence: &StructuralPropagationSequence,
        state: &StructuralState,
    ) -> ContinuitySnapshot {
        let n = base_snapshot.continuity.len();
        if n == 0 {
            return ContinuitySnapshot::new(base_snapshot.epoch + 1, Vec::new());
        }

        // Prepare a sorted copy of descriptors (stable, deterministic).
        let mut descs = sequence.descriptors.clone();
        descs.sort_by(|a, b| {
            a.deterministic_sequence_index
                .cmp(&b.deterministic_sequence_index)
                .then_with(|| a.lane_id.0.cmp(&b.lane_id.0))
        });

        // Accumulate per-target deterministically.
        let mut accum = vec![0.0f32; n];
        for d in descs.iter() {
            if d.target_node >= n { continue; }
            accum[d.target_node] = (accum[d.target_node] + d.reinforcement_weight).min(1.0);
        }

        // Apply accumulation to base_snapshot (immutable -> new vector).
        let mut out = base_snapshot.continuity.clone();
        for i in 0..n {
            out[i] = (out[i] + accum[i]).clamp(0.0, 1.0);
        }

        // Deterministic decay from structural state
        for v in &mut out {
            *v = (*v * state.continuity_factor).clamp(0.0, 1.0);
        }

        // Smoothing stabilization pass (index-ordered)
        let mut smoothed = out.clone();
        for i in 0..n {
            let mut sum = out[i];
            let mut count = 1.0f32;
            if i > 0 { sum += out[i - 1]; count += 1.0; }
            if i + 1 < n { sum += out[i + 1]; count += 1.0; }
            smoothed[i] = (sum / count).clamp(0.0, 1.0);
        }

        ContinuitySnapshot::new(base_snapshot.epoch + 1, smoothed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::continuity::ContinuitySnapshot;
    use crate::state::StructuralState;

    #[test]
    fn deterministic_structural_propagation_equivalence() {
        // base snapshot
        let base = ContinuitySnapshot::new(100, vec![0.0, 0.0, 0.0, 0.0]);

        // descriptors in two different insertion orders but same sequence indices
        let d1 = StructuralPropagationDescriptor {
            lane_id: MorphogenicLaneId(0x0001_0002),
            deterministic_sequence_index: 1,
            provenance: StructuralProvenance::new(10, 1, 1, 100),
            target_node: 2,
            reinforcement_weight: 0.4,
        };
        let d2 = StructuralPropagationDescriptor {
            lane_id: MorphogenicLaneId(0x0001_0003),
            deterministic_sequence_index: 0,
            provenance: StructuralProvenance::new(10, 1, 0, 100),
            target_node: 2,
            reinforcement_weight: 0.6,
        };

        let seq_a = StructuralPropagationSequence::from_descriptors(vec![d1.clone(), d2.clone()]);
        let seq_b = StructuralPropagationSequence::from_descriptors(vec![d2, d1]);

        let s = StructuralState::new(1.0, 0.0, 1.0);
        let out_a = MorphogenicRealizer::realize_sequence(0, &base, &seq_a, &s);
        let out_b = MorphogenicRealizer::realize_sequence(0, &base, &seq_b, &s);

        assert_eq!(out_a, out_b);
    }

    #[test]
    fn snapshot_epoch_increments() {
        let base = ContinuitySnapshot::new(5, vec![0.0; 3]);
        let seq = StructuralPropagationSequence::new();
        let s = StructuralState::default();
        let out = MorphogenicRealizer::realize_sequence(0, &base, &seq, &s);
        assert_eq!(out.epoch, 6);
    }
}