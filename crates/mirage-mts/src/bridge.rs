//! Topology -> Structural bridge (deterministic extraction only).
//! Non-authoritative: produces immutable StructuralPropagationSequence
//! for consumption by mirage-morphogenic.

use crate::topology::TopologyGraph;
use mirage_morphogenic::{
    MorphogenicLaneId,
    StructuralProvenance,
    StructuralPropagationDescriptor,
    StructuralPropagationSequence,
};

/// Build an immutable StructuralPropagationSequence from a TopologyGraph.
/// Deterministic ordering: sorted by (from, to) lexicographic to ensure replay equivalence.
/// provenance fields are filled using provided parameters; lane_sequence_index is assigned deterministically.
pub fn build_structural_propagation_sequence(
    graph: &TopologyGraph,
    originating_tick: u64,
    topology_generation: u64,
    continuity_epoch: u64,
) -> StructuralPropagationSequence {
    // Collect edges deterministic-pairs
    let mut edges: Vec<(usize, usize)> = Vec::new();
    for from in 0..graph.edges.len() {
        // copy targets to avoid depending on internal ordering; sort for determinism
        let mut targets = graph.edges[from].clone();
        targets.sort_unstable();
        for &to in targets.iter() {
            edges.push((from, to));
        }
    }

    // Sort all pairs lexicographically for a deterministic global order.
    edges.sort_unstable_by(|a, b| a.cmp(b));

    let mut descriptors = Vec::with_capacity(edges.len());
    let mut seq_index: u64 = 0;

    for (from, to) in edges.into_iter() {
        // deterministic lane id: combine (from,to) into u64 (allocator-independent)
        let lane_id = MorphogenicLaneId(((from as u64) << 32) | (to as u64));
        // deterministic reinforcement weight: use source node activation_pull (read-only)
        let weight = graph.nodes.get(from).map(|n| n.activation_pull.clamp(0.0, 1.0)).unwrap_or(0.0);

        let prov = StructuralProvenance {
            originating_tick,
            topology_generation,
            lane_sequence_index: seq_index,
            continuity_epoch,
        };

        let desc = StructuralPropagationDescriptor {
            lane_id,
            deterministic_sequence_index: seq_index,
            provenance: prov,
            target_node: to,
            reinforcement_weight: weight,
        };

        descriptors.push(desc);
        seq_index = seq_index.wrapping_add(1);
    }

    StructuralPropagationSequence::from_descriptors(descriptors)
}