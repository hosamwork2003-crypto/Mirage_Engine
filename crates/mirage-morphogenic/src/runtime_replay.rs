use crate::runtime_frame::{StructuralRuntimeFrame, RuntimeFrameIdentity};

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeReplayBuffer {
    frames: Vec<StructuralRuntimeFrame>,
    pub replay_epoch: u64,
    pub canonical_frame_count: u64,
}

impl RuntimeReplayBuffer {
    pub fn new(replay_epoch: u64) -> Self {
        Self {
            frames: Vec::new(),
            replay_epoch,
            canonical_frame_count: 0,
        }
    }

    pub fn push_frame(&mut self, frame: StructuralRuntimeFrame) {
        self.frames.push(frame);
        self.canonical_frame_count += 1;
    }

    pub fn latest(&self) -> Option<&StructuralRuntimeFrame> {
        self.frames.last()
    }

    pub fn frame_at(&self, index: usize) -> Option<&StructuralRuntimeFrame> {
        self.frames.get(index)
    }

    pub fn clear(&mut self) {
        self.frames.clear();
        self.canonical_frame_count = 0;
    }

    pub fn replay_equivalent(&self, other: &Self) -> bool {
        self.frames == other.frames && self.replay_epoch == other.replay_epoch
    }

    pub fn seal(&self, replay_identity: RuntimeFrameIdentity) -> RuntimeReplaySnapshot {
        RuntimeReplaySnapshot {
            replay_epoch: self.replay_epoch,
            sealed_frames: self.frames.clone(),
            replay_identity,
        }
    }

    pub fn frames(&self) -> &[StructuralRuntimeFrame] {
        &self.frames
    }

    pub fn frames_mut(&mut self) -> &mut [StructuralRuntimeFrame] {
        &mut self.frames
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeReplaySnapshot {
    pub replay_epoch: u64,
    pub sealed_frames: Vec<StructuralRuntimeFrame>,
    pub replay_identity: RuntimeFrameIdentity,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::continuity::ContinuitySnapshot;
    use crate::emergence::StructuralEmergenceState;
    use crate::resonance::EmergenceResonanceSnapshot;
    use crate::convergence::StructuralConvergenceState;
    use crate::persistence::StructuralPersistenceField;

    fn dummy_frame(frame_sequence: u64, originating_tick: u64) -> StructuralRuntimeFrame {
        use mirage_core::platform_drift::{RuntimeDeterminismSeal, PlatformDeterminismSignature};
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
    fn replay_buffer_determinism() {
        let mut buf1 = RuntimeReplayBuffer::new(1);
        let mut buf2 = RuntimeReplayBuffer::new(1);
        
        let frame1 = dummy_frame(1, 100);
        let frame2 = dummy_frame(2, 200);

        buf1.push_frame(frame1.clone());
        buf1.push_frame(frame2.clone());

        buf2.push_frame(frame1.clone());
        buf2.push_frame(frame2.clone());

        assert!(buf1.replay_equivalent(&buf2));
    }

    #[test]
    fn replay_snapshot_equivalence() {
        let mut buf1 = RuntimeReplayBuffer::new(1);
        let mut buf2 = RuntimeReplayBuffer::new(1);

        let frame = dummy_frame(1, 100);
        buf1.push_frame(frame.clone());
        buf2.push_frame(frame.clone());

        let identity = RuntimeFrameIdentity {
            frame_sequence: 1,
            originating_tick: 100,
            deterministic_hash_seed: 42,
        };

        let snap1 = buf1.seal(identity.clone());
        let snap2 = buf2.seal(identity.clone());

        assert_eq!(snap1, snap2);
    }

    #[test]
    fn insertion_order_stability() {
        let mut buf = RuntimeReplayBuffer::new(1);
        let f1 = dummy_frame(1, 10);
        let f2 = dummy_frame(2, 20);
        buf.push_frame(f1.clone());
        buf.push_frame(f2.clone());

        assert_eq!(buf.frame_at(0).unwrap(), &f1);
        assert_eq!(buf.frame_at(1).unwrap(), &f2);
    }

    #[test]
    fn deterministic_replay_roundtrip() {
        let mut buf = RuntimeReplayBuffer::new(42);
        buf.push_frame(dummy_frame(1, 100));
        let identity = RuntimeFrameIdentity {
            frame_sequence: 1,
            originating_tick: 100,
            deterministic_hash_seed: 7,
        };
        let snapshot = buf.seal(identity.clone());
        assert_eq!(snapshot.replay_epoch, 42);
        assert_eq!(snapshot.sealed_frames.len(), 1);
        assert_eq!(snapshot.replay_identity, identity);
    }
}
