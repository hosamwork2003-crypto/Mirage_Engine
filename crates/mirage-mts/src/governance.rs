// ===================================================================
// mirage-mts/src/governance.rs
// PURPOSE: V5.5 Topology Governance — Pure Deterministic Validators
//
// AUTHORITY BOUNDARY:
//   This module provides PURE, DETERMINISTIC governance validators only.
//   It does NOT:
//     * mutate topology state
//     * own any runtime structures
//     * orchestrate or schedule anything
//     * depend on morphogenic or mkr-core
//
// ALL functions in this module are:
//   * pure (no side effects)
//   * deterministic (same input → same output)
//   * primitive-input where possible
//   * replay-safe
// ===================================================================

use crate::topology::TopologyGraph;

// =====================================================================
// Validator 1: Topology Canonical Ownership
// =====================================================================

/// Validate that topology runtime ownership is canonical.
///
/// Checks that the graph is internally consistent:
/// - `nodes.len() == edges.len()` (every node has an adjacency list)
///
/// This is the primary structural integrity invariant. A graph where
/// `nodes.len() != edges.len()` indicates a construction error — a node
/// was added without a corresponding edge slot, or vice versa.
///
/// Returns `Ok(())` if canonical, `Err(&'static str)` with description if not.
pub fn validate_topology_canonical_ownership(graph: &TopologyGraph) -> Result<(), &'static str> {
    if graph.nodes.len() != graph.edges.len() {
        return Err(
            "topology canonical ownership violated: nodes.len() != edges.len()"
        );
    }
    Ok(())
}

// =====================================================================
// Validator 2: Topology Node Ordering
// =====================================================================

/// Validate that topology node IDs form a contiguous sequential range
/// starting at 0.
///
/// Enforces the V5.5 invariant: `node.id == node's index in nodes[]`.
/// This invariant is required for:
///   * deterministic traversal (index-based, not ID-based)
///   * replay equivalence (position in nodes[] is canonical identity)
///   * field alignment (node at index i maps to ActivationField cell i)
///
/// Returns `Ok(())` if all IDs are canonical, `Err` otherwise.
pub fn validate_topology_node_ordering(graph: &TopologyGraph) -> Result<(), &'static str> {
    for (i, node) in graph.nodes.iter().enumerate() {
        if node.id != i {
            return Err(
                "topology node ordering violated: node.id does not match its index in nodes[]"
            );
        }
    }
    Ok(())
}

// =====================================================================
// Validator 3: Lane IDs Stable
// =====================================================================

/// Validate that a sequence of lane IDs is non-decreasing.
///
/// Lane IDs must be monotonically non-decreasing to guarantee:
///   * deterministic iteration order (no hash/allocator dependence)
///   * stable propagation sequencing across ticks
///   * replay equivalence (same sequence index → same lane processed)
///
/// Input: `indices` — a slice of u64 lane sequence indices.
/// Returns `Ok(())` if non-decreasing, `Err` on first violation.
pub fn validate_lane_ids_stable(indices: &[u64]) -> Result<(), &'static str> {
    for i in 1..indices.len() {
        if indices[i - 1] > indices[i] {
            return Err(
                "lane ordering violated: indices are not non-decreasing"
            );
        }
    }
    Ok(())
}

// =====================================================================
// Validator 4: Propagation Descriptor Order
// =====================================================================

/// Validate that propagation descriptor sequence indices are non-decreasing.
///
/// StructuralPropagationSequence descriptors must be ordered by their
/// `deterministic_sequence_index` field to guarantee:
///   * stable propagation ordering across replay
///   * canonical descriptor identity (index == position in sequence)
///   * no ordering drift between runs
///
/// This is an alias for `validate_lane_ids_stable` with propagation-specific
/// error messaging for traceability.
///
/// Returns `Ok(())` if ordered, `Err` on first violation.
pub fn validate_propagation_descriptor_order(indices: &[u64]) -> Result<(), &'static str> {
    for i in 1..indices.len() {
        if indices[i - 1] > indices[i] {
            return Err(
                "propagation descriptor ordering violated: sequence indices are not non-decreasing"
            );
        }
    }
    Ok(())
}

// =====================================================================
// Validator 5: Topology Replay Equivalence
// =====================================================================

/// Validate that two influence scalar slices are identical within epsilon.
///
/// Two topology graphs built identically must produce identical
/// `influence_scalars()` output. This validator checks that two pre-computed
/// scalar slices are replay-equivalent.
///
/// Used to verify: given graph A and graph B built by the same sequence of
/// operations, `A.influence_scalars() == B.influence_scalars()` within f32 epsilon.
///
/// # Determinism Contract
/// This function is pure. Same inputs → same result. No heap nondeterminism.
///
/// Returns `Ok(())` if equivalent, `Err` describing first mismatch if not.
pub fn validate_topology_replay_equivalence(a: &[f32], b: &[f32]) -> Result<(), &'static str> {
    if a.len() != b.len() {
        return Err("topology replay equivalence violated: scalar slice lengths differ");
    }
    for (i, (&av, &bv)) in a.iter().zip(b.iter()).enumerate() {
        let _ = i;
        if (av - bv).abs() > f32::EPSILON * 4.0 {
            return Err(
                "topology replay equivalence violated: scalar values differ beyond epsilon"
            );
        }
    }
    Ok(())
}

// =====================================================================
// Validator 6: Topology Graph Internal Consistency
// =====================================================================

/// Validate complete internal consistency of a TopologyGraph.
///
/// Runs all structural validators:
///   1. Canonical ownership (nodes.len == edges.len)
///   2. Node ID ordering (sequential from 0)
///   3. Edge targets are within bounds (no out-of-range references)
///   4. flat_edges and access_frequency are consistent (same length)
///
/// Returns `Ok(())` if fully consistent, `Err` on first violation.
pub fn validate_topology_full_consistency(graph: &TopologyGraph) -> Result<(), &'static str> {
    validate_topology_canonical_ownership(graph)?;
    validate_topology_node_ordering(graph)?;

    // Validate all edge targets are in-bounds
    let n = graph.nodes.len();
    for (from, targets) in graph.edges.iter().enumerate() {
        for &to in targets {
            if to >= n {
                let _ = from;
                return Err("topology edge target is out of bounds");
            }
        }
    }

    // flat_edges and access_frequency must have the same length
    if graph.flat_edges.len() != graph.access_frequency.len() {
        return Err(
            "topology internal inconsistency: flat_edges.len() != access_frequency.len()"
        );
    }

    Ok(())
}

// =====================================================================
// Tests — V5.5 Governance Suite
// =====================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use crate::topology::{TopologyGraph, TopologyNode, ExecutionLane};
    use mirage_core::runtime::ChunkState;

    fn make_node(id: usize, pull: f32) -> TopologyNode {
        TopologyNode {
            id,
            thermal_state: ChunkState::Dormant,
            execution_lane: ExecutionLane::Background,
            dependency_mask: 0,
            wake_conditions: 0,
            continuation_targets: vec![],
            residency_requirement: 0,
            cost_estimate: 0.0,
            activation_pull: pull,
            cache_pressure: 0.0,
        }
    }

    fn build_canonical_graph(n: usize) -> TopologyGraph {
        let mut g = TopologyGraph::new();
        for i in 0..n {
            g.add_node(make_node(i, (i as f32) / (n as f32).max(1.0)));
        }
        for i in 0..n.saturating_sub(1) {
            g.add_edge(i, i + 1);
        }
        g
    }

    // ----------------------------------------------------------------
    // validate_topology_canonical_ownership
    // ----------------------------------------------------------------

    #[test]
    fn canonical_ownership_valid_for_empty_graph() {
        let g = TopologyGraph::new();
        assert!(validate_topology_canonical_ownership(&g).is_ok());
    }

    #[test]
    fn canonical_ownership_valid_for_chain_graph() {
        let g = build_canonical_graph(5);
        assert!(validate_topology_canonical_ownership(&g).is_ok());
    }

    // ----------------------------------------------------------------
    // validate_topology_node_ordering
    // ----------------------------------------------------------------

    #[test]
    fn node_ordering_valid_for_sequential_add() {
        let g = build_canonical_graph(6);
        assert!(validate_topology_node_ordering(&g).is_ok(),
            "sequential add_node must produce canonical ordering");
    }

    #[test]
    fn node_ordering_detects_mismatched_id() {
        // Build a graph where node.id doesn't match index by manually constructing
        let mut g = TopologyGraph::new();
        // Add a node with id=99 at index 0
        g.add_node(make_node(99, 0.5));
        assert!(validate_topology_node_ordering(&g).is_err(),
            "mismatched node id must be detected");
    }

    // ----------------------------------------------------------------
    // validate_lane_ids_stable
    // ----------------------------------------------------------------

    #[test]
    fn lane_ids_stable_accepts_non_decreasing() {
        let indices = [0u64, 0, 1, 2, 5, 5, 10];
        assert!(validate_lane_ids_stable(&indices).is_ok());
    }

    #[test]
    fn lane_ids_stable_rejects_decreasing() {
        let indices = [0u64, 5, 3];
        assert!(validate_lane_ids_stable(&indices).is_err());
    }

    #[test]
    fn lane_ids_stable_accepts_empty() {
        assert!(validate_lane_ids_stable(&[]).is_ok());
    }

    #[test]
    fn lane_ids_stable_accepts_single() {
        assert!(validate_lane_ids_stable(&[42u64]).is_ok());
    }

    // ----------------------------------------------------------------
    // validate_propagation_descriptor_order
    // ----------------------------------------------------------------

    #[test]
    fn propagation_descriptor_order_accepts_sorted() {
        let indices = [0u64, 1, 2, 3, 4];
        assert!(validate_propagation_descriptor_order(&indices).is_ok());
    }

    #[test]
    fn propagation_descriptor_order_rejects_unsorted() {
        let indices = [0u64, 2, 1, 3];
        assert!(validate_propagation_descriptor_order(&indices).is_err());
    }

    // ----------------------------------------------------------------
    // validate_topology_replay_equivalence
    // ----------------------------------------------------------------

    #[test]
    fn replay_equivalence_identical_slices() {
        let a = build_canonical_graph(5).influence_scalars();
        let b = build_canonical_graph(5).influence_scalars();
        assert!(validate_topology_replay_equivalence(&a, &b).is_ok(),
            "two identically-built graphs must produce replay-equivalent scalars");
    }

    #[test]
    fn replay_equivalence_rejects_different_lengths() {
        let a = [0.5f32, 0.5];
        let b = [0.5f32];
        assert!(validate_topology_replay_equivalence(&a, &b).is_err());
    }

    #[test]
    fn replay_equivalence_rejects_different_values() {
        let a = [0.5f32, 0.5];
        let b = [0.5f32, 0.9];
        assert!(validate_topology_replay_equivalence(&a, &b).is_err());
    }

    #[test]
    fn replay_equivalence_accepts_empty() {
        assert!(validate_topology_replay_equivalence(&[], &[]).is_ok());
    }

    // ----------------------------------------------------------------
    // validate_topology_full_consistency
    // ----------------------------------------------------------------

    #[test]
    fn full_consistency_valid_for_canonical_graph() {
        let g = build_canonical_graph(4);
        assert!(validate_topology_full_consistency(&g).is_ok());
    }

    #[test]
    fn full_consistency_valid_for_empty_graph() {
        let g = TopologyGraph::new();
        assert!(validate_topology_full_consistency(&g).is_ok());
    }

    // ----------------------------------------------------------------
    // Stable propagation ordering: build_structural_propagation_sequence
    // ----------------------------------------------------------------

    #[test]
    fn propagation_sequence_is_stably_ordered() {
        let g = build_canonical_graph(4);
        let seq1 = crate::bridge::build_structural_propagation_sequence(&g, 1, 1, 1);
        let seq2 = crate::bridge::build_structural_propagation_sequence(&g, 1, 1, 1);
        // Both sequences must have identical descriptor count
        let d1 = &seq1.descriptors;
        let d2 = &seq2.descriptors;
        assert_eq!(d1.len(), d2.len(),
            "stable propagation: sequence length must be identical across runs");
        // Sequence indices must be non-decreasing
        let indices1: Vec<u64> = d1.iter().map(|d| d.deterministic_sequence_index).collect();
        let indices2: Vec<u64> = d2.iter().map(|d| d.deterministic_sequence_index).collect();
        assert!(validate_propagation_descriptor_order(&indices1).is_ok(),
            "propagation sequence 1 must be non-decreasing");
        assert!(validate_propagation_descriptor_order(&indices2).is_ok(),
            "propagation sequence 2 must be non-decreasing");
        assert_eq!(indices1, indices2,
            "sequence indices must be identical across runs");
    }

    // ----------------------------------------------------------------
    // Canonical topology traversal: identical graphs → identical scalars
    // ----------------------------------------------------------------

    #[test]
    fn canonical_traversal_identical_graphs() {
        let g1 = build_canonical_graph(8);
        let g2 = build_canonical_graph(8);
        let s1 = g1.influence_scalars();
        let s2 = g2.influence_scalars();
        assert!(validate_topology_replay_equivalence(&s1, &s2).is_ok(),
            "canonical traversal: identical graphs must produce identical scalars");
    }

    // ----------------------------------------------------------------
    // Stable descriptor sorting: lane IDs from bridge are non-decreasing
    // ----------------------------------------------------------------

    #[test]
    fn bridge_lane_ids_are_non_decreasing() {
        let g = build_canonical_graph(5);
        let seq = crate::bridge::build_structural_propagation_sequence(&g, 0, 0, 0);
        let lane_ids: Vec<u64> = seq.descriptors
            .iter()
            .map(|d| d.lane_id.0)
            .collect();
        // Lane IDs are computed as (from << 32) | to — lexicographic order
        // They must be non-decreasing because edges are sorted before assignment
        assert!(validate_lane_ids_stable(&lane_ids).is_ok(),
            "bridge lane IDs must be non-decreasing (lexicographically sorted)");
    }
}
