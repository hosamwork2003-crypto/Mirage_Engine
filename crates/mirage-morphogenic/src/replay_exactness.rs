use crate::runtime_frame::StructuralRuntimeFrame;
use crate::runtime_replay::RuntimeReplaySnapshot;
use crate::canonical_serialization::{canonicalize_runtime_frame_bytes, canonicalize_snapshot_bytes};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct StrictReplayMode {
    pub exact_replay_required: bool,
    pub byte_equivalence_required: bool,
    pub deterministic_hash_required: bool,
    pub canonical_serialization_required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReplayExactnessReport {
    pub byte_equivalent: bool,
    pub deterministic_hash_match: bool,
    pub frame_sequence_match: bool,
    pub divergence_detected: bool,
}

pub fn verify_byte_replay_equivalence(a: &[u8], b: &[u8]) -> bool {
    a == b
}

pub fn verify_runtime_frame_exactness(
    a: &StructuralRuntimeFrame,
    b: &StructuralRuntimeFrame,
) -> ReplayExactnessReport {
    let bytes_a = canonicalize_runtime_frame_bytes(a);
    let bytes_b = canonicalize_runtime_frame_bytes(b);
    let byte_equivalent = bytes_a == bytes_b;
    let deterministic_hash_match = a.replay_identity.deterministic_hash_seed == b.replay_identity.deterministic_hash_seed;
    let frame_sequence_match = a.frame_sequence == b.frame_sequence;
    let divergence_detected = !byte_equivalent || !deterministic_hash_match || !frame_sequence_match;

    ReplayExactnessReport {
        byte_equivalent,
        deterministic_hash_match,
        frame_sequence_match,
        divergence_detected,
    }
}

pub fn verify_snapshot_exactness(
    a: &RuntimeReplaySnapshot,
    b: &RuntimeReplaySnapshot,
) -> ReplayExactnessReport {
    let bytes_a = canonicalize_snapshot_bytes(a);
    let bytes_b = canonicalize_snapshot_bytes(b);
    let byte_equivalent = bytes_a == bytes_b;
    let deterministic_hash_match = a.replay_identity.deterministic_hash_seed == b.replay_identity.deterministic_hash_seed;
    
    let mut frame_sequence_match = true;
    if a.sealed_frames.len() != b.sealed_frames.len() {
        frame_sequence_match = false;
    } else {
        for (f_a, f_b) in a.sealed_frames.iter().zip(b.sealed_frames.iter()) {
            if f_a.frame_sequence != f_b.frame_sequence {
                frame_sequence_match = false;
                break;
            }
        }
    }
    
    let divergence_detected = !byte_equivalent || !deterministic_hash_match || !frame_sequence_match;

    ReplayExactnessReport {
        byte_equivalent,
        deterministic_hash_match,
        frame_sequence_match,
        divergence_detected,
    }
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
            continuity_snapshot: ContinuitySnapshot::new(0, vec![]),
            emergence_state: StructuralEmergenceState::new(0.0, 0.0, 0.0, 0.0),
            resonance_snapshot: EmergenceResonanceSnapshot { epoch: 0, resonance: vec![] },
            convergence_state: StructuralConvergenceState::compute_convergence(0.0, 0.0, 0.0, 0.0),
            persistence_snapshot: StructuralPersistenceField::new(0),
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
    fn byte_exact_replay() {
        let f1 = dummy_frame(1, 100, 42);
        let f2 = dummy_frame(1, 100, 42);
        let report = verify_runtime_frame_exactness(&f1, &f2);
        assert!(report.byte_equivalent);
        assert!(!report.divergence_detected);
    }

    #[test]
    fn canonical_serialization_equivalence() {
        let f1 = dummy_frame(1, 100, 42);
        let f2 = dummy_frame(1, 100, 42);
        let bytes1 = canonicalize_runtime_frame_bytes(&f1);
        let bytes2 = canonicalize_runtime_frame_bytes(&f2);
        assert_eq!(bytes1, bytes2);
    }

    #[test]
    fn deterministic_hash_equivalence() {
        let f1 = dummy_frame(1, 100, 42);
        let f2 = dummy_frame(1, 100, 42);
        let report = verify_runtime_frame_exactness(&f1, &f2);
        assert!(report.deterministic_hash_match);
    }

    #[test]
    fn divergence_detection() {
        let f1 = dummy_frame(1, 100, 42);
        let f2 = dummy_frame(1, 100, 99); // different seed
        let report = verify_runtime_frame_exactness(&f1, &f2);
        assert!(report.divergence_detected);
        assert!(!report.deterministic_hash_match);
    }
}
