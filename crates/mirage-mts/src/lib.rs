// ===================================================================
// mirage-mts/src/lib.rs
// PURPOSE: Metamorphic Topology Substrate (MTS) — V6 Public API
//
// V6 OWNERSHIP DECLARATION:
// ---------------------------------------------------------------
// mirage-mts is the SOLE authoritative owner of the topology runtime.
//
// OWNS:
//   * TopologyGraph               — activation influence graph
//   * TopologyNode                — chunk-level topology node
//   * ExecutionLane               — processing domain tag
//   * AlignmentResult             — topology-field alignment check
//   * TopologyInfluenceProvider   — trait contract for activation solver
//   * InfluenceCache              — zero-alloc per-tick scratchpad
//   * propagation bridge          — deterministic, read-only extraction
//   * compute_propagation()       — deterministic grid propagation utility
//   * governance                  — pure deterministic topology validators
//
// MUST NOT:
//   * own orchestration / runtime execution — owned by mirage-mkr-core
//   * own continuity / emergence           — owned by mirage-morphogenic
//   * own runtime execution authority      — owned by mirage-mkr-core
//
// DETERMINISTIC & REPLAY GUARANTEES:
//   * influence_scalars()  — pure function, same input → same output
//   * topology traversal   — index-ordered, no hash-map dependence
//   * propagation bridge   — lexicographically sorted edge pairs
//   * lane IDs             — deterministic u64 from (from, to) indices
//   * governance validators — pure, primitive-only, no side effects
// ===================================================================

// Topology implementation is owned exclusively here.
pub mod topology;
pub mod propagation;
pub mod bridge;

// V5.5: Governance validators — pure deterministic topology authority checks.
pub mod governance;

pub use crate::bridge::build_structural_propagation_sequence;

pub use crate::topology::{
    AlignmentResult,
    ExecutionLane,
    TopologyGraph,
    TopologyNode,
};

pub use crate::propagation::compute_propagation;

// Re-export governance validators at crate root for ergonomic access.
pub use crate::governance::{
    validate_topology_canonical_ownership,
    validate_topology_node_ordering,
    validate_lane_ids_stable,
    validate_propagation_descriptor_order,
    validate_topology_replay_equivalence,
    validate_topology_full_consistency,
};

// =====================================================================
// TopologyInfluenceProvider — MTS trait contract
// =====================================================================

/// Trait contract for types that can supply per-cell topology influence
/// scalars to the activation solver.
///
/// Implementing this trait decouples the activation solver from the
/// concrete `TopologyGraph` type, enabling alternative implementations
/// (e.g., sparse graphs, procedural generators, GPU readback buffers).
///
/// # V5.5 Contract
/// * `fill_influence_scalars(buf)` must write exactly `len()` f32 values
///   into `buf`, each in the range `[0.0, 1.0]`.
/// * Output must be deterministic: same graph state → same scalars.
/// * Implementations must not depend on allocator or hash order.
pub trait TopologyInfluenceProvider {
    /// Number of cells this topology covers.
    fn len(&self) -> usize;

    /// Whether this topology covers zero cells.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Fill `buf` with per-cell influence scalars.
    ///
    /// `buf` will be resized to `self.len()` before the call if needed.
    fn fill_influence_scalars(&self, buf: &mut Vec<f32>);

    /// Assert topological alignment with a target field size.
    ///
    /// Default implementation is a no-op. Override to enforce alignment.
    fn assert_aligned(&self, field_cell_count: usize) {
        let _ = field_cell_count;
    }
}

// =====================================================================
// Blanket impl: TopologyGraph implements TopologyInfluenceProvider
// =====================================================================

impl TopologyInfluenceProvider for TopologyGraph {
    fn len(&self) -> usize {
        self.nodes.len()
    }

    fn fill_influence_scalars(&self, buf: &mut Vec<f32>) {
        self.influence_scalars_into(buf);
    }

    fn assert_aligned(&self, field_cell_count: usize) {
        self.assert_aligned(field_cell_count);
    }
}

// =====================================================================
// InfluenceCache — zero-allocation per-tick scratchpad
// =====================================================================

/// Pre-allocated scratchpad for topology influence scalars.
///
/// Avoids per-tick heap allocation when the solver queries the topology.
/// `refresh` fills the cache from any `TopologyInfluenceProvider`;
/// `as_slice` returns the cached scalar slice.
///
/// V5.5: The cache is deterministic — same provider state → same slice.
/// It never shrinks (amortized allocation, identical to `Vec::resize`).
pub struct InfluenceCache {
    buf: Vec<f32>,
}

impl InfluenceCache {
    /// Create an empty cache with pre-allocated capacity.
    pub fn new(capacity: usize) -> Self {
        Self { buf: Vec::with_capacity(capacity) }
    }

    /// Refresh the cache from a `TopologyInfluenceProvider`.
    ///
    /// Resizes the buffer if the topology grew. Never shrinks.
    pub fn refresh(&mut self, provider: &dyn TopologyInfluenceProvider) {
        let n = provider.len();
        if self.buf.len() != n {
            self.buf.resize(n, 0.0);
        }
        provider.fill_influence_scalars(&mut self.buf);
    }

    /// Return a reference to the cached scalar slice.
    #[inline]
    pub fn as_slice(&self) -> &[f32] {
        &self.buf
    }

    /// Return the current cached length.
    #[inline]
    pub fn len(&self) -> usize {
        self.buf.len()
    }

    /// Whether the cache is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.buf.is_empty()
    }
}

// =====================================================================
// Tests — V5.5 Deterministic Suite (lib-level: InfluenceCache + Provider)
// =====================================================================
// Note: Topology governance tests live in governance.rs (mod governance::tests).
#[cfg(test)]
mod tests {
    use super::*;

    fn make_node(id: usize, pull: f32) -> TopologyNode {
        TopologyNode {
            id,
            thermal_state: mirage_core::runtime::ChunkState::Dormant,
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

    fn build_chain_graph(n: usize) -> TopologyGraph {
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
    // TopologyInfluenceProvider trait + InfluenceCache
    // ----------------------------------------------------------------

    #[test]
    fn topology_graph_implements_provider() {
        let topo = TopologyGraph::new();
        let mut cache = InfluenceCache::new(0);
        cache.refresh(&topo);
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn influence_cache_fills_from_graph_with_nodes() {
        let mut topo = TopologyGraph::new();
        topo.add_node(make_node(0, 0.8));
        let mut cache = InfluenceCache::new(1);
        cache.refresh(&topo);
        assert_eq!(cache.len(), 1);
        let s = cache.as_slice()[0];
        assert!(s >= 0.0 && s <= 1.0, "influence scalar out of range: {}", s);
    }

    #[test]
    fn provider_trait_is_object_safe() {
        let topo = TopologyGraph::new();
        let provider: &dyn TopologyInfluenceProvider = &topo;
        assert_eq!(provider.len(), 0);
        assert!(provider.is_empty());
    }

    #[test]
    fn influence_cache_resize_on_growth() {
        let mut cache = InfluenceCache::new(0);
        let mut topo = TopologyGraph::new();
        cache.refresh(&topo);
        assert_eq!(cache.len(), 0);

        topo.add_node(make_node(0, 0.5));
        cache.refresh(&topo);
        assert_eq!(cache.len(), 1);
    }

    // ----------------------------------------------------------------
    // Deterministic traversal — same input → same output
    // ----------------------------------------------------------------

    #[test]
    fn influence_scalars_deterministic() {
        let g = build_chain_graph(8);
        let s1 = g.influence_scalars();
        let s2 = g.influence_scalars();
        assert_eq!(s1.len(), s2.len(), "scalars length must be stable");
        for (i, (a, b)) in s1.iter().zip(s2.iter()).enumerate() {
            assert_eq!(a, b,
                "node {} scalar differs between calls: {} vs {}", i, a, b);
        }
    }

    // ----------------------------------------------------------------
    // Replay equivalence — two identical builds → same output
    // ----------------------------------------------------------------

    #[test]
    fn replay_equivalence_identical_graphs() {
        let g1 = build_chain_graph(6);
        let g2 = build_chain_graph(6);
        let s1 = g1.influence_scalars();
        let s2 = g2.influence_scalars();
        assert_eq!(s1.len(), s2.len());
        for (i, (a, b)) in s1.iter().zip(s2.iter()).enumerate() {
            assert_eq!(a, b,
                "replay: node {} scalar differs: {} vs {}", i, a, b);
        }
    }

    // ----------------------------------------------------------------
    // influence_scalars_into == influence_scalars (ownership isolation)
    // ----------------------------------------------------------------

    #[test]
    fn influence_scalars_into_matches_alloc_version() {
        let g = build_chain_graph(6);
        let alloc = g.influence_scalars();
        let mut buf = Vec::new();
        g.influence_scalars_into(&mut buf);
        assert_eq!(alloc.len(), buf.len());
        for (i, (a, b)) in alloc.iter().zip(buf.iter()).enumerate() {
            let diff = (a - b).abs();
            assert!(diff < f32::EPSILON,
                "node {} mismatch: alloc={} into={}", i, a, b);
        }
    }

    // ----------------------------------------------------------------
    // Canonical snapshot equality via InfluenceCache
    // ----------------------------------------------------------------

    #[test]
    fn canonical_snapshot_equality_via_cache() {
        let g = build_chain_graph(5);
        let mut cache1 = InfluenceCache::new(5);
        let mut cache2 = InfluenceCache::new(5);
        cache1.refresh(&g);
        cache2.refresh(&g);
        assert_eq!(cache1.as_slice(), cache2.as_slice(),
            "two cache refreshes from same graph must be identical");
    }

    // ----------------------------------------------------------------
    // History replay determinism — InfluenceCache stable over multiple refreshes
    // ----------------------------------------------------------------

    #[test]
    fn history_replay_determinism_multi_refresh() {
        let g = build_chain_graph(4);
        let mut cache = InfluenceCache::new(4);
        cache.refresh(&g);
        let snap1: Vec<f32> = cache.as_slice().to_vec();
        cache.refresh(&g);
        let snap2: Vec<f32> = cache.as_slice().to_vec();
        assert_eq!(snap1, snap2,
            "repeated refresh of unchanged graph must produce identical snapshots");
    }

    // ----------------------------------------------------------------
    // Governance re-exports: validators accessible at crate root
    // ----------------------------------------------------------------

    #[test]
    fn governance_validators_accessible_at_crate_root() {
        let g = build_chain_graph(3);
        assert!(validate_topology_canonical_ownership(&g).is_ok());
        assert!(validate_topology_node_ordering(&g).is_ok());
        assert!(validate_lane_ids_stable(&[0u64, 1, 2]).is_ok());
        assert!(validate_propagation_descriptor_order(&[0u64, 1, 2]).is_ok());
        let s = g.influence_scalars();
        assert!(validate_topology_replay_equivalence(&s, &s.clone()).is_ok());
        assert!(validate_topology_full_consistency(&g).is_ok());
    }
}
