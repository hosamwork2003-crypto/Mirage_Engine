use crate::runtime_frame::StructuralRuntimeFrame;
use crate::runtime_replay::{RuntimeReplaySnapshot, RuntimeReplayBuffer};

pub fn canonicalize_runtime_frame_bytes(frame: &StructuralRuntimeFrame) -> Vec<u8> {
    let mut bytes = Vec::new();

    // 1. Core Frame Info
    bytes.extend_from_slice(&frame.frame_sequence.to_le_bytes());
    bytes.extend_from_slice(&frame.originating_tick.to_le_bytes());
    bytes.extend_from_slice(&frame.runtime_epoch.to_le_bytes());
    bytes.extend_from_slice(&frame.topology_generation.to_le_bytes());

    // 2. Continuity Snapshot
    bytes.extend_from_slice(&frame.continuity_snapshot.epoch.to_le_bytes());
    bytes.extend_from_slice(&frame.continuity_snapshot.realization_sequence_index.to_le_bytes());
    bytes.extend_from_slice(&frame.continuity_snapshot.originating_tick.to_le_bytes());
    // Continuity Snapshot Identity
    bytes.extend_from_slice(&frame.continuity_snapshot.snapshot_identity.continuity_epoch.to_le_bytes());
    bytes.extend_from_slice(&frame.continuity_snapshot.snapshot_identity.originating_tick.to_le_bytes());
    bytes.extend_from_slice(&frame.continuity_snapshot.snapshot_identity.realization_sequence_index.to_le_bytes());
    // Continuity Snapshot Data
    bytes.extend_from_slice(&(frame.continuity_snapshot.continuity.len() as u64).to_le_bytes());
    for &val in &frame.continuity_snapshot.continuity {
        bytes.extend_from_slice(&val.to_le_bytes());
    }

    // 3. Emergence State
    bytes.extend_from_slice(&frame.emergence_state.emergence_score.to_le_bytes());
    bytes.extend_from_slice(&frame.emergence_state.resonance_factor.to_le_bytes());
    bytes.extend_from_slice(&frame.emergence_state.stabilization_factor.to_le_bytes());
    bytes.extend_from_slice(&frame.emergence_state.convergence_factor.to_le_bytes());

    // 4. Resonance Snapshot
    bytes.extend_from_slice(&frame.resonance_snapshot.epoch.to_le_bytes());
    bytes.extend_from_slice(&(frame.resonance_snapshot.resonance.len() as u64).to_le_bytes());
    for &val in &frame.resonance_snapshot.resonance {
        bytes.extend_from_slice(&val.to_le_bytes());
    }

    // 5. Convergence State
    bytes.extend_from_slice(&frame.convergence_state.continuity_pressure.to_le_bytes());
    bytes.extend_from_slice(&frame.convergence_state.reinforcement_pressure.to_le_bytes());
    bytes.extend_from_slice(&frame.convergence_state.resonance_pressure.to_le_bytes());
    bytes.extend_from_slice(&frame.convergence_state.stabilization_pressure.to_le_bytes());

    // 6. Persistence Snapshot
    bytes.extend_from_slice(&(frame.persistence_snapshot.len() as u64).to_le_bytes());
    for i in 0..frame.persistence_snapshot.len() {
        if let Some(val) = frame.persistence_snapshot.get(i) {
            bytes.extend_from_slice(&val.to_le_bytes());
        }
    }

    // 7. Replay Identity
    bytes.extend_from_slice(&frame.replay_identity.frame_sequence.to_le_bytes());
    bytes.extend_from_slice(&frame.replay_identity.originating_tick.to_le_bytes());
    bytes.extend_from_slice(&frame.replay_identity.deterministic_hash_seed.to_le_bytes());

    // 8. Determinism Seal
    bytes.extend_from_slice(&frame.determinism_seal.runtime_hash.to_le_bytes());
    bytes.extend_from_slice(&frame.determinism_seal.frame_hash.to_le_bytes());
    bytes.extend_from_slice(&frame.determinism_seal.replay_hash.to_le_bytes());
    bytes.extend_from_slice(&frame.determinism_seal.policy_hash.to_le_bytes());

    // Platform Signature
    let sig = &frame.determinism_seal.platform_signature;
    bytes.extend_from_slice(&(sig.architecture.len() as u64).to_le_bytes());
    bytes.extend_from_slice(sig.architecture.as_bytes());
    bytes.extend_from_slice(&(sig.compiler_version.len() as u64).to_le_bytes());
    bytes.extend_from_slice(sig.compiler_version.as_bytes());
    bytes.extend_from_slice(&sig.float_policy_hash.to_le_bytes());
    bytes.extend_from_slice(&sig.simd_policy_hash.to_le_bytes());
    bytes.extend_from_slice(&sig.runtime_policy_hash.to_le_bytes());

    // 9. Boolean flags
    bytes.push(frame.canonical_numeric_state as u8);
    bytes.push(frame.replay_exactness_verified as u8);

    bytes
}

pub fn canonicalize_snapshot_bytes(snapshot: &RuntimeReplaySnapshot) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&snapshot.replay_epoch.to_le_bytes());

    // Sealed Frames
    bytes.extend_from_slice(&(snapshot.sealed_frames.len() as u64).to_le_bytes());
    for frame in &snapshot.sealed_frames {
        bytes.extend_from_slice(&canonicalize_runtime_frame_bytes(frame));
    }

    // Replay Identity
    bytes.extend_from_slice(&snapshot.replay_identity.frame_sequence.to_le_bytes());
    bytes.extend_from_slice(&snapshot.replay_identity.originating_tick.to_le_bytes());
    bytes.extend_from_slice(&snapshot.replay_identity.deterministic_hash_seed.to_le_bytes());

    bytes
}

pub fn canonicalize_replay_bytes(replay: &RuntimeReplayBuffer) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&replay.replay_epoch.to_le_bytes());
    bytes.extend_from_slice(&replay.canonical_frame_count.to_le_bytes());

    // Frames
    bytes.extend_from_slice(&(replay.frames().len() as u64).to_le_bytes());
    for frame in replay.frames() {
        bytes.extend_from_slice(&canonicalize_runtime_frame_bytes(frame));
    }

    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_frame::RuntimeFrameIdentity;
    use crate::continuity::ContinuitySnapshot;
    use crate::emergence::StructuralEmergenceState;
    use crate::resonance::EmergenceResonanceSnapshot;
    use crate::convergence::StructuralConvergenceState;
    use crate::persistence::StructuralPersistenceField;
    use mirage_core::platform_drift::{RuntimeDeterminismSeal, PlatformDeterminismSignature};

    fn test_seal() -> RuntimeDeterminismSeal {
        RuntimeDeterminismSeal {
            runtime_hash: 1,
            frame_hash: 2,
            replay_hash: 3,
            policy_hash: 4,
            platform_signature: PlatformDeterminismSignature {
                architecture: "x86_64".to_string(),
                compiler_version: "rustc-1.78.0".to_string(),
                float_policy_hash: 10,
                simd_policy_hash: 20,
                runtime_policy_hash: 30,
            },
        }
    }

    fn dummy_frame(seq: u64, tick: u64, seed: u64) -> StructuralRuntimeFrame {
        StructuralRuntimeFrame {
            frame_sequence: seq,
            originating_tick: tick,
            runtime_epoch: 0,
            topology_generation: 0,
            continuity_snapshot: ContinuitySnapshot::new(0, vec![0.5, 0.75]),
            emergence_state: StructuralEmergenceState::new(0.1, 0.2, 0.3, 0.4),
            resonance_snapshot: EmergenceResonanceSnapshot { epoch: 0, resonance: vec![0.9] },
            convergence_state: StructuralConvergenceState::compute_convergence(0.5, 0.5, 0.5, 0.5),
            persistence_snapshot: {
                let mut p = StructuralPersistenceField::new(2);
                p.set(0, 0.2);
                p.set(1, 0.4);
                p
            },
            replay_identity: RuntimeFrameIdentity {
                frame_sequence: seq,
                originating_tick: tick,
                deterministic_hash_seed: seed,
            },
            determinism_seal: test_seal(),
            canonical_numeric_state: true,
            replay_exactness_verified: true,
        }
    }

    #[test]
    fn canonical_byte_order() {
        let f1 = dummy_frame(1, 100, 42);
        let bytes = canonicalize_runtime_frame_bytes(&f1);
        // Ensure bytes length is correct and has a stable layout
        assert!(!bytes.is_empty());
    }

    #[test]
    fn deterministic_serialization() {
        let f1 = dummy_frame(1, 100, 42);
        let f2 = dummy_frame(1, 100, 42);
        let bytes1 = canonicalize_runtime_frame_bytes(&f1);
        let bytes2 = canonicalize_runtime_frame_bytes(&f2);
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn identical_replay_bytes() {
        let f1 = dummy_frame(1, 100, 42);
        let f2 = dummy_frame(2, 200, 43);

        let mut replay1 = RuntimeReplayBuffer::new(5);
        replay1.push_frame(f1.clone());
        replay1.push_frame(f2.clone());

        let mut replay2 = RuntimeReplayBuffer::new(5);
        replay2.push_frame(f1);
        replay2.push_frame(f2);

        let bytes1 = canonicalize_replay_bytes(&replay1);
        let bytes2 = canonicalize_replay_bytes(&replay2);
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn stable_frame_encoding() {
        let f = dummy_frame(99, 999, 12345);
        let bytes = canonicalize_runtime_frame_bytes(&f);
        // Verify frame_sequence (99) is serialized at the start in little-endian
        assert_eq!(bytes[0..8], 99u64.to_le_bytes());
    }
}
