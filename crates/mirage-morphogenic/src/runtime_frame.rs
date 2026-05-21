use crate::continuity::ContinuitySnapshot;
use crate::emergence::StructuralEmergenceState;
use crate::resonance::EmergenceResonanceSnapshot;
use crate::convergence::StructuralConvergenceState;
use crate::persistence::StructuralPersistenceField;
use mirage_core::platform_drift::RuntimeDeterminismSeal;

#[derive(Clone, Debug, PartialEq)]
pub struct StructuralRuntimeFrame {
    pub frame_sequence: u64,
    pub originating_tick: u64,
    pub runtime_epoch: u64,
    pub topology_generation: u64,
    pub continuity_snapshot: ContinuitySnapshot,
    pub emergence_state: StructuralEmergenceState,
    pub resonance_snapshot: EmergenceResonanceSnapshot,
    pub convergence_state: StructuralConvergenceState,
    pub persistence_snapshot: StructuralPersistenceField,
    pub replay_identity: RuntimeFrameIdentity,
    pub determinism_seal: RuntimeDeterminismSeal,
    pub canonical_numeric_state: bool,
    pub replay_exactness_verified: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RuntimeFrameIdentity {
    pub frame_sequence: u64,
    pub originating_tick: u64,
    pub deterministic_hash_seed: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeFrameSequence {
    frames: Vec<StructuralRuntimeFrame>,
}

impl RuntimeFrameSequence {
    pub fn new(frames: Vec<StructuralRuntimeFrame>) -> Self {
        Self { frames }
    }

    pub fn stable_sort(&mut self) {
        self.frames.sort_by(|a, b| {
            a.frame_sequence
                .cmp(&b.frame_sequence)
                .then_with(|| a.originating_tick.cmp(&b.originating_tick))
        });
    }

    pub fn latest(&self) -> Option<&StructuralRuntimeFrame> {
        self.frames.last()
    }

    pub fn frame_at(&self, index: usize) -> Option<&StructuralRuntimeFrame> {
        self.frames.get(index)
    }

    pub fn len(&self) -> usize {
        self.frames.len()
    }

    pub fn is_empty(&self) -> bool {
        self.frames.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dummy_frame(frame_sequence: u64, originating_tick: u64) -> StructuralRuntimeFrame {
        use mirage_core::platform_drift::PlatformDeterminismSignature;
        StructuralRuntimeFrame {
            frame_sequence,
            originating_tick,
            runtime_epoch: 0,
            topology_generation: 0,
            continuity_snapshot: ContinuitySnapshot::new(0, vec![]),
            emergence_state: StructuralEmergenceState::new(0.0, 0.0, 0.0, 0.0),
            resonance_snapshot: EmergenceResonanceSnapshot { epoch: 0, resonance: vec![] },
            convergence_state: StructuralConvergenceState::compute_convergence(0.0, 0.0, 0.0, 0.0),
            persistence_snapshot: StructuralPersistenceField::new(0),
            replay_identity: RuntimeFrameIdentity {
                frame_sequence,
                originating_tick,
                deterministic_hash_seed: 0,
            },
            determinism_seal: RuntimeDeterminismSeal {
                runtime_hash: 0,
                frame_hash: 0,
                replay_hash: 0,
                policy_hash: 0,
                platform_signature: PlatformDeterminismSignature {
                    architecture: "unknown".to_string(),
                    compiler_version: "unknown".to_string(),
                    float_policy_hash: 0,
                    simd_policy_hash: 0,
                    runtime_policy_hash: 0,
                },
            },
            canonical_numeric_state: true,
            replay_exactness_verified: true,
        }
    }

    #[test]
    fn deterministic_frame_ordering() {
        let frame1 = dummy_frame(1, 100);
        let frame2 = dummy_frame(1, 200);
        let frame3 = dummy_frame(2, 50);

        let mut seq = RuntimeFrameSequence::new(vec![frame2.clone(), frame3.clone(), frame1.clone()]);
        seq.stable_sort();

        assert_eq!(seq.frame_at(0).unwrap().frame_sequence, 1);
        assert_eq!(seq.frame_at(0).unwrap().originating_tick, 100);

        assert_eq!(seq.frame_at(1).unwrap().frame_sequence, 1);
        assert_eq!(seq.frame_at(1).unwrap().originating_tick, 200);

        assert_eq!(seq.frame_at(2).unwrap().frame_sequence, 2);
        assert_eq!(seq.frame_at(2).unwrap().originating_tick, 50);
    }

    #[test]
    fn runtime_frame_equality() {
        let f1 = dummy_frame(1, 10);
        let f2 = dummy_frame(1, 10);
        assert_eq!(f1, f2);
    }

    #[test]
    fn stable_frame_sorting() {
        let f1 = dummy_frame(2, 10);
        let f2 = dummy_frame(1, 20);
        let mut seq = RuntimeFrameSequence::new(vec![f1.clone(), f2.clone()]);
        seq.stable_sort();
        assert_eq!(seq.frame_at(0).unwrap().frame_sequence, 1);
        assert_eq!(seq.frame_at(1).unwrap().frame_sequence, 2);
    }

    #[test]
    fn replay_equivalent_frames() {
        let f1 = dummy_frame(5, 50);
        let f2 = dummy_frame(5, 50);
        assert_eq!(f1, f2);
    }
}
