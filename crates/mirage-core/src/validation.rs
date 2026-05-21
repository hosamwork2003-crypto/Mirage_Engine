use crate::invariants::DeterministicInvariantViolation;

pub trait RuntimeValidatable {
    fn validate_runtime_invariants(&self) -> Result<(), Vec<DeterministicInvariantViolation>>;
}

/// Lightweight helper for replay equivalence validation of two sequences of frames.
pub fn validate_replay_equivalence_frames<T: PartialEq>(a: &[T], b: &[T]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    if a.len() != b.len() {
        return Err(vec![DeterministicInvariantViolation { invariant_name: "replay_frame_length_mismatch", subsystem: "validation" }]);
    }
    for i in 0..a.len() {
        if a[i] != b[i] {
            return Err(vec![DeterministicInvariantViolation { invariant_name: "replay_frame_mismatch", subsystem: "validation" }]);
        }
    }
    Ok(())
}

/// Validate emergence numeric bounds (all fields must be within [0.0, 1.0]).
pub fn validate_emergence_bounds(emergence_score: f32, resonance_factor: f32, stabilization_factor: f32, convergence_factor: f32) -> Result<(), Vec<DeterministicInvariantViolation>> {
    let mut violations = Vec::new();
    if !(0.0..=1.0).contains(&emergence_score) { violations.push(DeterministicInvariantViolation { invariant_name: "emergence_score_bounds", subsystem: "emergence" }); }
    if !(0.0..=1.0).contains(&resonance_factor) { violations.push(DeterministicInvariantViolation { invariant_name: "resonance_factor_bounds", subsystem: "emergence" }); }
    if !(0.0..=1.0).contains(&stabilization_factor) { violations.push(DeterministicInvariantViolation { invariant_name: "stabilization_factor_bounds", subsystem: "emergence" }); }
    if !(0.0..=1.0).contains(&convergence_factor) { violations.push(DeterministicInvariantViolation { invariant_name: "convergence_factor_bounds", subsystem: "emergence" }); }
    if violations.is_empty() { Ok(()) } else { Err(violations) }
}

/// Validate a resonance descriptor sequence is stably ordered (non-decreasing sequence indices).
pub fn validate_resonance_sequence(indices: &[u64], subsystem: &'static str) -> Result<(), Vec<DeterministicInvariantViolation>> {
    for i in 1..indices.len() {
        if indices[i - 1] > indices[i] {
            return Err(vec![DeterministicInvariantViolation { invariant_name: "resonance_sequence_unstable", subsystem }]);
        }
    }
    Ok(())
}

/// Validate structural convergence state values are in [0,1].
pub fn validate_convergence_state(values: &[f32], subsystem: &'static str) -> Result<(), Vec<DeterministicInvariantViolation>> {
    for &v in values.iter() {
        if !(0.0..=1.0).contains(&v) {
            return Err(vec![DeterministicInvariantViolation { invariant_name: "convergence_value_out_of_bounds", subsystem }]);
        }
    }
    Ok(())
}

// =====================================================================
// V5.5 Structural Canonicalization Helpers
// =====================================================================

/// Canonicalize a sequence of descriptor indices by stable-sorting them
/// in non-decreasing order. Deterministic: no hash or allocator dependence.
///
/// Use this to canonicalize propagation/resonance/history sequences before
/// comparison or replay equivalence checks.
pub fn canonicalize_sequence_indices(indices: &mut Vec<u64>) {
    indices.sort();
}

/// Validate that a slice of topology descriptor indices is non-decreasing.
/// Returns Ok(()) if valid, Err with a violation if any index violates order.
pub fn validate_topology_descriptor_ordering(indices: &[u64]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    for i in 1..indices.len() {
        if indices[i - 1] > indices[i] {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "topology_descriptor_order_violated",
                subsystem: "topology",
            }]);
        }
    }
    Ok(())
}

/// Validate that any u64 index slice is non-decreasing (general stable index order check).
///
/// This is the primitive general-purpose validator for stable ordering.
/// Use for any sequence of indices that must be monotonically non-decreasing
/// to guarantee deterministic iteration and replay equivalence.
///
/// Unlike `validate_topology_descriptor_ordering`, this function is subsystem-agnostic
/// and can be used for lane IDs, history indices, resonance indices, etc.
///
/// Returns `Ok(())` if non-decreasing, `Err` containing a single violation on failure.
pub fn validate_stable_index_order(indices: &[u64], subsystem: &'static str) -> Result<(), Vec<DeterministicInvariantViolation>> {
    for i in 1..indices.len() {
        if indices[i - 1] > indices[i] {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "stable_index_order_violated",
                subsystem,
            }]);
        }
    }
    Ok(())
}

/// Validate replay equivalence of two f32 slices within epsilon tolerance.
/// More lenient than snapshot_equivalence — allows for floating point rounding.
pub fn validate_replay_equivalence_f32(a: &[f32], b: &[f32], subsystem: &'static str) -> Result<(), Vec<DeterministicInvariantViolation>> {
    if a.len() != b.len() {
        return Err(vec![DeterministicInvariantViolation { invariant_name: "replay_length_mismatch", subsystem }]);
    }
    for (i, (&av, &bv)) in a.iter().zip(b.iter()).enumerate() {
        let _ = i;
        if (av - bv).abs() > f32::EPSILON * 4.0 {
            return Err(vec![DeterministicInvariantViolation { invariant_name: "replay_f32_mismatch", subsystem }]);
        }
    }
    Ok(())
}

/// Validate resonance replay equivalence between two resonance factor slices.
pub fn validate_resonance_replay_equivalence(a: &[f32], b: &[f32]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    validate_replay_equivalence_f32(a, b, "resonance_replay")
}

/// Validate history replay equivalence between two history value slices.
pub fn validate_history_replay_equivalence(a: &[f32], b: &[f32]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    validate_replay_equivalence_f32(a, b, "history_replay")
}

/// Validate continuity equivalence between two continuity field value slices.
pub fn validate_continuity_equivalence(a: &[f32], b: &[f32]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    validate_replay_equivalence_f32(a, b, "continuity_equivalence")
}

// =====================================================================
// V6 Structural Canonicalization Helpers
// =====================================================================

/// Validate that runtime frame sequences are in non-decreasing order,
/// and within the same frame sequence, originating ticks are strictly increasing.
pub fn validate_runtime_frame_ordering(frame_sequences: &[u64], originating_ticks: &[u64]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    if frame_sequences.len() != originating_ticks.len() {
        return Err(vec![DeterministicInvariantViolation {
            invariant_name: "frame_ordering_mismatched_lengths",
            subsystem: "runtime_validation",
        }]);
    }
    for i in 1..frame_sequences.len() {
        if frame_sequences[i - 1] > frame_sequences[i] {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "frame_sequence_not_sorted",
                subsystem: "runtime_validation",
            }]);
        } else if frame_sequences[i - 1] == frame_sequences[i] {
            if originating_ticks[i - 1] >= originating_ticks[i] {
                return Err(vec![DeterministicInvariantViolation {
                    invariant_name: "duplicate_or_unsorted_tick",
                    subsystem: "runtime_validation",
                }]);
            }
        }
    }
    Ok(())
}

/// Validate that ticks are strictly increasing.
pub fn validate_runtime_tick_monotonicity(ticks: &[u64]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    for i in 1..ticks.len() {
        if ticks[i - 1] >= ticks[i] {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "tick_non_monotonic",
                subsystem: "runtime_validation",
            }]);
        }
    }
    Ok(())
}

/// Stably sort a mutable vector of keys representing (frame_sequence, originating_tick) tuples.
pub fn canonicalize_runtime_frames(frame_keys: &mut Vec<(u64, u64)>) {
    frame_keys.sort_by(|a, b| {
        a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_replay_equivalence_ok() {
        let a = [1u64, 2, 3];
        let b = [1u64, 2, 3];
        assert!(validate_replay_equivalence_frames(&a, &b).is_ok());
    }

    #[test]
    fn validate_replay_equivalence_fail() {
        let a = [1u64, 2, 3];
        let b = [1u64, 4, 3];
        assert!(validate_replay_equivalence_frames(&a, &b).is_err());
    }

    #[test]
    fn emergence_bounds_validator_ok() {
        assert!(validate_emergence_bounds(0.5, 0.2, 0.3, 1.0).is_ok());
    }

    #[test]
    fn emergence_bounds_validator_fail() {
        assert!(validate_emergence_bounds(1.2, 0.0, 0.0, 0.0).is_err());
    }

    // ----------------------------------------------------------------
    // Canonicalization helpers
    // ----------------------------------------------------------------

    #[test]
    fn canonicalize_sequence_indices_sorts_stable() {
        let mut v = vec![5u64, 2, 8, 1, 3];
        canonicalize_sequence_indices(&mut v);
        assert_eq!(v, vec![1, 2, 3, 5, 8]);
    }

    #[test]
    fn canonicalize_idempotent() {
        let mut v = vec![0u64, 1, 2, 3];
        canonicalize_sequence_indices(&mut v);
        let first = v.clone();
        canonicalize_sequence_indices(&mut v);
        assert_eq!(v, first, "canonicalize must be idempotent");
    }

    #[test]
    fn topology_descriptor_ordering_valid() {
        let indices = [0u64, 1, 2, 3];
        assert!(validate_topology_descriptor_ordering(&indices).is_ok());
    }

    #[test]
    fn topology_descriptor_ordering_invalid() {
        let indices = [0u64, 3, 1];
        assert!(validate_topology_descriptor_ordering(&indices).is_err());
    }

    // ----------------------------------------------------------------
    // validate_stable_index_order (V5.5 Phase 8 addition)
    // ----------------------------------------------------------------

    #[test]
    fn stable_index_order_valid_sorted() {
        let indices = [0u64, 1, 1, 2, 5, 10];
        assert!(validate_stable_index_order(&indices, "test").is_ok());
    }

    #[test]
    fn stable_index_order_rejects_unsorted() {
        let indices = [0u64, 5, 3];
        assert!(validate_stable_index_order(&indices, "test").is_err());
    }

    #[test]
    fn stable_index_order_accepts_empty() {
        assert!(validate_stable_index_order(&[], "test").is_ok());
    }

    #[test]
    fn stable_index_order_accepts_single() {
        assert!(validate_stable_index_order(&[42u64], "test").is_ok());
    }

    #[test]
    fn stable_index_order_subsystem_is_preserved_in_error() {
        let indices = [1u64, 0];
        let err = validate_stable_index_order(&indices, "lane_ids").unwrap_err();
        assert_eq!(err[0].subsystem, "lane_ids");
        assert_eq!(err[0].invariant_name, "stable_index_order_violated");
    }

    // ----------------------------------------------------------------
    // Replay equivalence
    // ----------------------------------------------------------------

    #[test]
    fn resonance_replay_equivalence_ok() {
        let a = [0.5f32, 0.3, 0.8];
        let b = [0.5f32, 0.3, 0.8];
        assert!(validate_resonance_replay_equivalence(&a, &b).is_ok());
    }

    #[test]
    fn resonance_replay_equivalence_fail() {
        let a = [0.5f32, 0.3];
        let b = [0.5f32, 0.9];
        assert!(validate_resonance_replay_equivalence(&a, &b).is_err());
    }

    #[test]
    fn history_replay_equivalence_ok() {
        let a = [0.1f32, 0.2];
        let b = [0.1f32, 0.2];
        assert!(validate_history_replay_equivalence(&a, &b).is_ok());
    }

    #[test]
    fn continuity_equivalence_ok() {
        let a = [0.0f32, 0.5, 1.0];
        let b = [0.0f32, 0.5, 1.0];
        assert!(validate_continuity_equivalence(&a, &b).is_ok());
    }

    #[test]
    fn resonance_sequence_validator_ok() {
        let indices = [0u64, 1, 1, 2];
        assert!(validate_resonance_sequence(&indices, "resonance").is_ok());
    }

    #[test]
    fn convergence_state_all_in_bounds() {
        let v = [0.0f32, 0.5, 1.0];
        assert!(validate_convergence_state(&v, "convergence").is_ok());
    }

    #[test]
    fn convergence_state_out_of_bounds() {
        let v = [0.0f32, 1.5];
        assert!(validate_convergence_state(&v, "convergence").is_err());
    }

    // ----------------------------------------------------------------
    // V6 helpers
    // ----------------------------------------------------------------

    #[test]
    fn v6_frame_ordering() {
        let seqs = vec![1, 1, 2];
        let ticks = vec![100, 200, 300];
        assert!(validate_runtime_frame_ordering(&seqs, &ticks).is_ok());

        let bad_ticks = vec![100, 100, 200];
        assert!(validate_runtime_frame_ordering(&seqs, &bad_ticks).is_err());
    }

    #[test]
    fn v6_tick_monotonicity() {
        let ticks = vec![10, 20, 30];
        assert!(validate_runtime_tick_monotonicity(&ticks).is_ok());

        let bad_ticks = vec![10, 10, 20];
        assert!(validate_runtime_tick_monotonicity(&bad_ticks).is_err());
    }

    #[test]
    fn v6_canonicalize_frames() {
        let mut keys = vec![(2, 200), (1, 100), (1, 50)];
        canonicalize_runtime_frames(&mut keys);
        assert_eq!(keys, vec![(1, 50), (1, 100), (2, 200)]);
    }
}