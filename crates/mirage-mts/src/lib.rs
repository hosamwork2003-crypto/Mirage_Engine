// ===================================================================
// mirage-mts/src/lib.rs
// PURPOSE: Metamorphic Topology Substrate (MTS) — Public API Layer
//
// ARCHITECTURAL ROLE
// ---------------------------------------------------------------
// MTS sits between the raw topology data model (mirage-matrix) and
// the MKR core activation solver (mirage-mkr-core). It:
//
//   1. Re-exports the authoritative TopologyGraph and related types
//      from mirage-matrix under a stable MTS namespace.
//
//   2. Defines the TopologyInfluenceProvider trait — the primary
//      contract the activation solver depends on. Any type that
//      can supply a flat &[f32] influence scalar slice per cell
//      satisfies this trait.
//
//   3. Provides an InfluenceCache scratchpad that minimizes
//      per-tick allocation when the solver queries influence scalars.
//
// DEPENDENCY DIRECTION
// ---------------------------------------------------------------
//   mirage-matrix (data model)
//       ↓
//   mirage-mts    (topology API + trait contracts)
//       ↓
//   mirage-mkr-core (consumes TopologyInfluenceProvider)
//
// NO CIRCULAR DEPENDENCIES. mirage-mts does NOT depend on
// mirage-mkr-core or any activation-field types.
// ===================================================================

// Topology implementation is owned here (moved from mirage-matrix).
pub mod topology;
pub mod propagation;
pub mod bridge;

pub use crate::bridge::build_structural_propagation_sequence;

pub use crate::topology::{
    AlignmentResult,
    ExecutionLane,
    TopologyGraph,
    TopologyNode,
};

pub use crate::propagation::compute_propagation;

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
/// # Contract
/// `fill_influence_scalars(buf)` must write exactly `len()` f32 values
/// into `buf`, each in the range `[0.0, 1.0]`. Values represent how
/// strongly the graph topology pulls each cell toward higher activation.
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
    /// Resizes the buffer if the topology grew. Never shrinks (amortized
    /// allocation pattern identical to `Vec::resize`).
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
// Tests
// =====================================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn topology_graph_implements_provider() {
        let topo = TopologyGraph::new();
        let mut cache = InfluenceCache::new(0);
        cache.refresh(&topo);
        // Empty graph → empty cache
        assert_eq!(cache.len(), 0);
        assert!(cache.is_empty());
    }

    #[test]
    fn influence_cache_fills_from_graph_with_nodes() {
        let mut topo = TopologyGraph::new();
        topo.add_node(TopologyNode {
            id: 0,
            thermal_state: mirage_core::runtime::ChunkState::Dormant,
            execution_lane: ExecutionLane::Physics,
            dependency_mask: 0,
            wake_conditions: 0,
            continuation_targets: vec![],
            residency_requirement: 0,
            cost_estimate: 1.0,
            activation_pull: 0.8,
            cache_pressure: 0.0,
        });
        let mut cache = InfluenceCache::new(1);
        cache.refresh(&topo);
        assert_eq!(cache.len(), 1);
        let s = cache.as_slice()[0];
        assert!(s >= 0.0 && s <= 1.0, "influence scalar out of range: {}", s);
    }

    #[test]
    fn provider_trait_is_object_safe() {
        // Verify we can use TopologyInfluenceProvider as a trait object.
        let topo = TopologyGraph::new();
        let provider: &dyn TopologyInfluenceProvider = &topo;
        assert_eq!(provider.len(), 0);
        assert!(provider.is_empty());
    }

    #[test]
    fn influence_cache_resize_on_growth() {
        let mut cache = InfluenceCache::new(0);
        let mut topo = TopologyGraph::new();
        cache.refresh(&topo); // empty
        assert_eq!(cache.len(), 0);

        topo.add_node(TopologyNode {
            id: 0,
            thermal_state: mirage_core::runtime::ChunkState::Dormant,
            execution_lane: ExecutionLane::Background,
            dependency_mask: 0,
            wake_conditions: 0,
            continuation_targets: vec![],
            residency_requirement: 0,
            cost_estimate: 0.5,
            activation_pull: 0.5,
            cache_pressure: 0.0,
        });
        cache.refresh(&topo); // must resize to 1
        assert_eq!(cache.len(), 1);
    }
}
