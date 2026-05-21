// ===================================================================
// mirage-core/src/spatial_validation.rs
// PURPOSE: Pure spatial validation routines for V7 runtime governance.
// ===================================================================

use crate::invariants::DeterministicInvariantViolation;

/// Validate that region IDs are sorted in strictly increasing order.
pub fn validate_region_ordering(region_ids: &[u64]) -> Result<(), DeterministicInvariantViolation> {
    for i in 1..region_ids.len() {
        if region_ids[i] <= region_ids[i - 1] {
            return Err(DeterministicInvariantViolation {
                invariant_name: "invalid_region_ordering",
                subsystem: "spatial_validation",
            });
        }
    }
    Ok(())
}

/// Validate that region generations are non-decreasing.
pub fn validate_region_monotonicity(generations: &[u64]) -> Result<(), DeterministicInvariantViolation> {
    for i in 1..generations.len() {
        if generations[i] < generations[i - 1] {
            return Err(DeterministicInvariantViolation {
                invariant_name: "invalid_region_monotonicity",
                subsystem: "spatial_validation",
            });
        }
    }
    Ok(())
}

/// Validate that streaming sequence indices are non-decreasing.
pub fn validate_streaming_sequence(sequence_indices: &[u64]) -> Result<(), DeterministicInvariantViolation> {
    for i in 1..sequence_indices.len() {
        if sequence_indices[i] < sequence_indices[i - 1] {
            return Err(DeterministicInvariantViolation {
                invariant_name: "invalid_streaming_sequence",
                subsystem: "spatial_validation",
            });
        }
    }
    Ok(())
}

/// Validate that residency states are within bounds (0..=3).
pub fn validate_residency_ordering(residency_states: &[u8]) -> Result<(), DeterministicInvariantViolation> {
    for &state in residency_states {
        if state > 3 {
            return Err(DeterministicInvariantViolation {
                invariant_name: "invalid_residency_state",
                subsystem: "spatial_validation",
            });
        }
    }
    Ok(())
}

/// Validate that two lists of spatial replay hashes are exactly identical.
pub fn validate_spatial_replay_equivalence(hashes_a: &[u64], hashes_b: &[u64]) -> Result<(), DeterministicInvariantViolation> {
    if hashes_a != hashes_b {
        return Err(DeterministicInvariantViolation {
            invariant_name: "spatial_replay_mismatch",
            subsystem: "spatial_validation",
        });
    }
    Ok(())
}

/// Validate that two lists of world snapshot hashes are exactly identical.
pub fn validate_world_snapshot_equivalence(hashes_a: &[u64], hashes_b: &[u64]) -> Result<(), DeterministicInvariantViolation> {
    if hashes_a != hashes_b {
        return Err(DeterministicInvariantViolation {
            invariant_name: "world_snapshot_mismatch",
            subsystem: "spatial_validation",
        });
    }
    Ok(())
}

/// Validate that transitions (source, target, seq_idx) are ordered lexicographically.
pub fn validate_region_transition_order(transitions: &[(u64, u64, u64)]) -> Result<(), DeterministicInvariantViolation> {
    for i in 1..transitions.len() {
        let (s1, t1, seq1) = transitions[i - 1];
        let (s2, t2, seq2) = transitions[i];
        if s1 > s2 || (s1 == s2 && t1 > t2) || (s1 == s2 && t1 == t2 && seq1 > seq2) {
            return Err(DeterministicInvariantViolation {
                invariant_name: "invalid_region_transition_order",
                subsystem: "spatial_validation",
            });
        }
    }
    Ok(())
}

/// Validate that nodes are sorted and all edges connect valid nodes in the graph.
pub fn validate_canonical_region_graph(nodes: &[u64], edges: &[(u64, u64)]) -> Result<(), DeterministicInvariantViolation> {
    for i in 1..nodes.len() {
        if nodes[i] <= nodes[i - 1] {
            return Err(DeterministicInvariantViolation {
                invariant_name: "unsorted_graph_nodes",
                subsystem: "spatial_validation",
            });
        }
    }
    for &(src, dst) in edges {
        if nodes.binary_search(&src).is_err() || nodes.binary_search(&dst).is_err() {
            return Err(DeterministicInvariantViolation {
                invariant_name: "orphaned_graph_edge",
                subsystem: "spatial_validation",
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_contracts::ForbiddenSpatialAuthority;

    #[test]
    fn reject_invalid_region_order() {
        let valid = vec![1, 2, 5, 10];
        let invalid = vec![1, 5, 2, 10];
        assert!(validate_region_ordering(&valid).is_ok());
        assert!(validate_region_ordering(&invalid).is_err());
    }

    #[test]
    fn reject_duplicate_region_ids() {
        let duplicate = vec![1, 2, 2, 3];
        assert!(validate_region_ordering(&duplicate).is_err());
    }

    #[test]
    fn reject_streaming_regression() {
        let valid = vec![100, 101, 101, 102];
        let invalid = vec![100, 101, 99, 102];
        assert!(validate_streaming_sequence(&valid).is_ok());
        assert!(validate_streaming_sequence(&invalid).is_err());
    }

    #[test]
    fn validate_world_replay_equivalence() {
        let a = vec![111, 222, 333];
        let b = vec![111, 222, 333];
        let c = vec![111, 222, 444];
        assert!(validate_world_snapshot_equivalence(&a, &b).is_ok());
        assert!(validate_world_snapshot_equivalence(&a, &c).is_err());
    }

    #[test]
    fn forbid_async_spatial_authority() {
        let auth = ForbiddenSpatialAuthority::AsyncScheduling;
        assert_eq!(auth.as_str(), "async scheduling");
    }
}

