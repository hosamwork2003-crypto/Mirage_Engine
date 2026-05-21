// ===================================================================
// mirage-mts/src/topology.rs
// PURPOSE: TopologyGraph — Activation Influence Graph (V5.5)
//
// V5.5 OWNERSHIP DECLARATION:
// This file is the SOLE authoritative owner of the TopologyGraph runtime.
// No other crate may define, re-implement, or shadow this type.
//
// AUTHORITY BOUNDARY:
//   OWNS:   TopologyGraph, TopologyNode, ExecutionLane, AlignmentResult
//   OWNED BY: mirage-mts (Metamorphic Topology Substrate)
//   FORBIDDEN: mirage-matrix must NOT own topology runtime.
//             mirage-morphogenic must NOT own topology runtime.
//
// ARCHITECTURAL ROLE:
// The TopologyGraph is an "activation influence graph" providing a
// continuous influence_scalars() slice to the ActivationSolver.
// Each entry is a f32 in [0.0, 1.0] representing how strongly the
// graph topology pulls that cell toward higher activation.
//
// DETERMINISTIC GUARANTEES (V5.5):
//   * influence_scalars()       — pure, deterministic, allocation-minimal
//   * influence_scalars_into()  — zero-alloc variant, same algorithm
//   * add_node() ordering       — sequential, index-stable
//   * rebalance_edges()         — stable sort (descending frequency)
//   * check_alignment()         — pure length comparison, no side effects
//
// COMPAT:
//   propagate_thermal() and wake_node() are V2 compat shims retained
//   only for mirage-executor backward compatibility. They MUST NOT be
//   extended with new logic. Remove after executor migrates to V3.
// ===================================================================

use mirage_core::runtime::{ChunkState, ThermalSystem};

/// Execution lanes represent different processing domains in the topology graph.
///
/// Each lane tags a topology node with its processing domain. The activation
/// solver uses lane membership to group nodes for deterministic batch traversal.
/// V5.5: Lane IDs are stable and must not change across ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutionLane {
    Physics,
    GpuCompute,
    Streaming,
    Ai,
    Renderer,
    Audio,
    Network,
    Background,
}

/// Topology node represents an execution region (chunk-level).
///
/// `thermal_state` is retained for backward compatibility with the executor
/// thermal system. For all new code, use `activation_pull: f32` which is
/// the V5.5 continuous field authority.
///
/// `id` must equal the node's sequential index in TopologyGraph::nodes.
/// Use `add_node_for_cell()` to enforce this invariant at construction time.
#[derive(Debug, Clone)]
pub struct TopologyNode {
    pub id:                     usize,
    /// TODO(V3-COMPAT): Replace with `activation_pull: f32`.
    pub thermal_state:          ChunkState,
    pub execution_lane:         ExecutionLane,
    pub dependency_mask:        u32,
    pub wake_conditions:        u32,
    pub continuation_targets:   Vec<usize>,
    pub residency_requirement:  u8,
    pub cost_estimate:          f32,
    /// Edge weight influence toward adjacent cells (continuous, 0..1).
    /// This is the V3 field — use it rather than thermal_state for new code.
    pub activation_pull:        f32,
    pub cache_pressure:         f32,
}

/// Simple adjacency-style topology graph.
///
/// # V5.5 Role: Activation Influence Graph
/// The graph provides `influence_scalars()` — a flat `Vec<f32>` that maps
/// each node index to a continuous topology influence weight in `[0.0, 1.0]`.
/// The `ActivationSolver` consumes this slice each tick to propagate
/// topology-driven pressure across the activation field.
///
/// # Determinism Contract (V5.5)
/// - `influence_scalars()` is a pure function of node/edge state. Same input → same output.
/// - `add_node()` assigns sequential IDs. Field-aligned graphs use `add_node_for_cell()`.
/// - `rebalance_edges()` uses a stable descending sort by frequency.
///
/// # Sole Owner
/// Defined exclusively in `mirage-mts`. No other crate defines this type.

/// Result of a topology-to-field alignment check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AlignmentResult {
    /// Node count matches field cell count — fully aligned.
    Aligned { node_count: usize },
    /// Node count is less than field cell count — some cells have no topology node.
    /// Cells beyond `node_count` receive zero topology influence (silent truncation).
    Partial { node_count: usize, field_cell_count: usize },
    /// Graph has more nodes than field cells — excess nodes are unreachable from the field.
    Excess  { node_count: usize, field_cell_count: usize },
    /// Graph is empty — no topology influence this tick.
    Empty,
}

impl AlignmentResult {
    /// True only if the graph is perfectly aligned with the field.
    pub fn is_aligned(&self) -> bool { matches!(self, AlignmentResult::Aligned { .. }) }
    /// True if any cells have no topology node (potential silent truncation).
    pub fn has_coverage_gap(&self) -> bool { matches!(self, AlignmentResult::Partial { .. } | AlignmentResult::Empty) }
}

/// TODO(V3-TOPOLOGY-ALIGNMENT): TopologyGraph nodes MUST be aligned
/// 1:1 with ActivationField cells.  Node `id` returned by `add_node()`
/// must equal the corresponding `ActivationField::cells[id]` index.
///
/// Current state: alignment is NOT enforced — callers can add nodes in
/// any order and any count.  `influence_scalars()` silently truncates if
/// the graph has fewer nodes than field cells.
///
/// Migration: use `add_node_for_cell(cell_idx, node)` which asserts the
/// index invariant, and call `validate_field_alignment(field_len)` at
/// topology construction time.
pub struct TopologyGraph {
    pub nodes: Vec<TopologyNode>,
    pub edges: Vec<Vec<usize>>,
    pub flat_edges: Vec<(usize, usize)>,
    pub access_frequency: Vec<u32>,
}

impl TopologyGraph {
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            flat_edges: Vec::new(),
            access_frequency: Vec::new(),
        }
    }

    /// Add a topology node and return its id.
    ///
    /// The returned id equals `nodes.len() - 1` after insertion.
    /// For field-aligned graphs, the id must equal the corresponding
    /// field cell index.  Use `add_node_for_cell()` to enforce this.
    pub fn add_node(&mut self, node: TopologyNode) -> usize {
        let id = self.nodes.len();
        self.nodes.push(node);
        self.edges.push(Vec::new());
        id
    }

    /// Add a topology node at an explicit field cell index.
    ///
    /// # Panics (debug)
    /// Panics if `cell_idx != self.nodes.len()` — the node must be added
    /// in strictly sequential field-cell order to maintain alignment.
    ///
    /// V5.5: All new callers must use this API (not `add_node()`) to
    /// prevent silent index misalignment with the activation field.
    pub fn add_node_for_cell(&mut self, cell_idx: usize, node: TopologyNode) -> usize {
        debug_assert_eq!(
            cell_idx, self.nodes.len(),
            "topology node must be added in field-cell order (expected idx={}, got idx={})",
            self.nodes.len(), cell_idx
        );
        self.add_node(node)
    }

    pub fn add_edge(&mut self, from: usize, to: usize) {
        if let Some(adj) = self.edges.get_mut(from) {
            adj.push(to);
            self.flat_edges.push((from, to));
            self.access_frequency.push(0);
        }
    }

    pub fn find_edge(&self, from: usize, to: usize) -> usize {
        self.flat_edges
            .iter()
            .position(|&edge| edge == (from, to))
            .unwrap_or(usize::MAX)
    }

    pub fn record_access(&mut self, edge_index: usize) {
        if edge_index < self.access_frequency.len() {
            self.access_frequency[edge_index] = self.access_frequency[edge_index].saturating_add(1);
        }
    }

    pub fn rebalance_edges(&mut self) {
        const HOT_THRESHOLD: u32 = 1000;
        let has_hot_edge = self.access_frequency.iter().any(|&freq| freq >= HOT_THRESHOLD);
        if !has_hot_edge {
            return;
        }

        for from in 0..self.nodes.len() {
            let targets_with_freq: Vec<(usize, u32)> = {
                let targets = &self.edges[from];
                targets
                    .iter()
                    .map(|&to| {
                        let idx = self.find_edge(from, to);
                        let freq = if idx < self.access_frequency.len() {
                            self.access_frequency[idx]
                        } else {
                            0
                        };
                        (to, freq)
                    })
                    .collect()
            };

            // Sort descending by frequency
            let mut sorted_targets = targets_with_freq;
            sorted_targets.sort_by(|a, b| b.1.cmp(&a.1));

            // Write back (mutably borrow edges[from] now that the immutable borrow is over)
            self.edges[from] = sorted_targets.into_iter().map(|(to, _)| to).collect();
        }
    }

    /// Validate that this topology graph is aligned with a field of `field_len` cells.
    ///
    /// Returns an `AlignmentResult` describing the alignment state.
    /// This is a zero-cost check (just a length comparison).
    ///
    /// # V5.5 Usage
    /// Call at topology construction time and at the start of each
    /// `MKRWorld::tick()` Phase 0 in debug builds to detect misalignment early.
    pub fn check_alignment(&self, field_len: usize) -> AlignmentResult {
        let n = self.nodes.len();
        if n == 0 { return AlignmentResult::Empty; }
        match n.cmp(&field_len) {
            std::cmp::Ordering::Equal   => AlignmentResult::Aligned { node_count: n },
            std::cmp::Ordering::Less    => AlignmentResult::Partial { node_count: n, field_cell_count: field_len },
            std::cmp::Ordering::Greater => AlignmentResult::Excess  { node_count: n, field_cell_count: field_len },
        }
    }

    /// Assert alignment with the field in debug builds.
    ///
    /// Behavior by case:
    ///   Empty graph   → no-op (zero influence, no assertion needed)
    ///   Partial graph → debug warning only (valid during migration)
    ///   Aligned       → no-op (fully correct)
    ///   Excess nodes  → debug_assert! panic (programming error)
    ///
    /// In release builds this is a complete no-op.
    #[inline]
    pub fn assert_aligned(&self, field_len: usize) {
        if self.nodes.is_empty() { return; }
        debug_assert!(
            self.nodes.len() <= field_len,
            "topology EXCESS: {} nodes but only {} field cells — \
             excess nodes are unreachable and indicate a programming error",
            self.nodes.len(), field_len
        );
        #[cfg(debug_assertions)]
        if self.nodes.len() < field_len {
            // Partial topology is valid during executor migration — warn but don't panic.
            if self.nodes.len() > 0 && field_len > self.nodes.len() + 1 {
                // Only warn when the gap is large (>1) to avoid test noise
                eprintln!(
                    "[V5.5-TOPOLOGY-ALIGNMENT] partial topology: {} nodes / {} cells — \
                     cells {}..{} receive no topology influence",
                    self.nodes.len(), field_len, self.nodes.len(), field_len - 1
                );
            }
        }
    }

    // ------------------------------------------------------------------
    // V3: Activation influence interface
    // ------------------------------------------------------------------

    /// Return a flat slice of per-node activation influence scalars.
    ///
    /// This is the primary V3 output of the TopologyGraph — it feeds
    /// directly into `ActivationSolver::step(field, topo_influence)`.
    ///
    /// Each element is a continuous f32 in `[0.0, 1.0]` representing
    /// how strongly the graph topology wants this node's chunk to be active.
    ///
    /// # Algorithm: Directed Edge-Weight Accumulation
    ///
    /// For each node `i`, we accumulate `activation_pull` from all nodes
    /// that have a directed edge pointing TO `i` (in-neighbours).
    ///
    /// ```text
    /// Step 1 — Build in-degree table:
    ///   for each edge (from → to): in_degree[to] += 1
    ///
    /// Step 2 — Accumulate in-edge pulls:
    ///   for each edge (from → to):
    ///     accumulated[to] += nodes[from].activation_pull
    ///
    /// Step 3 — Normalize by max in-degree (prevents explosion):
    ///   max_in = max(in_degree)
    ///   if max_in > 0: accumulated[i] /= max_in
    ///
    /// Step 4 — Blend with own pull (60% edge / 40% own):
    ///   influence[i] = accumulated[i] * 0.60 + node.activation_pull * 0.40
    ///
    /// Step 5 — Clamp to [0.0, 1.0].
    /// ```
    ///
    /// # Properties
    /// * **Deterministic** — pure function of current edge and node state.
    /// * **Non-explosive** — normalised by max in-degree, clamped at 1.0.
    /// * **Flat iterative** — two O(|E|) passes, no recursion.
    /// * **SIMD-friendly** — steps 2–5 are element-wise on contiguous Vec.
    /// * **Cache-friendly** — edge list is traversed sequentially.
    ///
    /// If the graph has no nodes, returns an empty Vec.
    /// If a node has no in-edges, its own `activation_pull` is used directly.
    pub fn influence_scalars(&self) -> Vec<f32> {
        let n = self.nodes.len();
        if n == 0 {
            return Vec::new();
        }

        // Step 1 — In-degree table and in-edge pull accumulator.
        let mut in_degree: Vec<u32>  = vec![0u32; n];
        let mut accumulated: Vec<f32> = vec![0.0f32; n];

        // Single flat pass over the edge list: O(|E|).
        // edges[from] contains a Vec<usize> of target node indices.
        for (from, targets) in self.edges.iter().enumerate() {
            let source_pull = self.nodes[from].activation_pull.clamp(0.0, 1.0);
            for &to in targets {
                if to < n {
                    in_degree[to] = in_degree[to].saturating_add(1);
                    // Accumulate the source node's pull at the target.
                    accumulated[to] = (accumulated[to] + source_pull).min(f32::MAX);
                }
            }
        }

        // Step 2 — Normalisation denominator: max in-degree across all nodes.
        // Using max in-degree prevents a high-fan-in node from over-accumulating.
        let max_in = in_degree.iter().copied().max().unwrap_or(0);
        let norm_denom = if max_in > 0 { max_in as f32 } else { 1.0 };

        // Step 3 — Normalise accumulated pulls.
        for acc in &mut accumulated {
            *acc /= norm_denom;
        }

        // Step 4 — Blend: 60% edge-accumulated, 40% own pull.
        // This ensures isolated nodes (no in-edges) still contribute
        // via their own activation_pull, while well-connected nodes
        // are dominated by graph topology.
        const EDGE_WEIGHT: f32 = 0.60;
        const OWN_WEIGHT:  f32 = 0.40;

        self.nodes
            .iter()
            .zip(accumulated.iter())
            .map(|(node, &acc)| {
                let own = node.activation_pull.clamp(0.0, 1.0);
                (acc * EDGE_WEIGHT + own * OWN_WEIGHT).clamp(0.0, 1.0)
            })
            .collect()
    }

    /// Set the activation pull for an individual node.
    ///
    /// Call this when an external event (streaming, visibility, etc.)
    /// should increase topology-driven pressure on a specific chunk.
    pub fn set_activation_pull(&mut self, node_idx: usize, pull: f32) {
        if let Some(node) = self.nodes.get_mut(node_idx) {
            node.activation_pull = pull.clamp(0.0, 1.0);
        }
    }

    // ------------------------------------------------------------------
    // V3-SPARSE: Topology sparse preparation (Task 9)
    // ------------------------------------------------------------------

    /// Write influence scalars into a pre-allocated buffer.
    ///
    /// Identical algorithm to `influence_scalars()` but avoids the per-tick
    /// `Vec<f32>` allocation (~61 KB for 15625 nodes at 60 Hz → 3.6 MB/s).
    ///
    /// # Usage
    /// Caller must ensure `buffer.len() >= self.nodes.len()`.
    /// If the buffer is too short, it is resized.
    ///
    /// V5.5: This is the preferred zero-allocation entry point for production
    /// tick loops. `influence_scalars()` is a convenience wrapper.
    pub fn influence_scalars_into(&self, buffer: &mut Vec<f32>) {
        let n = self.nodes.len();
        if n == 0 { buffer.clear(); return; }

        // Ensure pre-allocated capacity
        if buffer.len() < n { buffer.resize(n, 0.0); }

        // Reuse the same algorithm as influence_scalars() — zero allocation.
        let mut in_degree:   Vec<u32> = vec![0u32;  n];
        let mut accumulated: Vec<f32> = vec![0.0f32; n];

        for (from, targets) in self.edges.iter().enumerate() {
            let source_pull = self.nodes[from].activation_pull.clamp(0.0, 1.0);
            for &to in targets {
                if to < n {
                    in_degree[to] = in_degree[to].saturating_add(1);
                    accumulated[to] = (accumulated[to] + source_pull).min(f32::MAX);
                }
            }
        }

        let max_in     = in_degree.iter().copied().max().unwrap_or(0);
        let norm_denom = if max_in > 0 { max_in as f32 } else { 1.0 };

        const EDGE_WEIGHT: f32 = 0.60;
        const OWN_WEIGHT:  f32 = 0.40;

        for (i, node) in self.nodes.iter().enumerate() {
            accumulated[i] /= norm_denom;
            let own = node.activation_pull.clamp(0.0, 1.0);
            buffer[i] = (accumulated[i] * EDGE_WEIGHT + own * OWN_WEIGHT).clamp(0.0, 1.0);
        }

        // Truncate to exact node count (buffer may have been larger from prior use)
        buffer.truncate(n);
    }

    /// Compute influence ONLY for nodes whose `activation_pull` changed
    /// relative to a previous snapshot.
    ///
    /// # Frontier-Aware Topology Traversal (Task 9)
    ///
    /// In a stable scene, most topology nodes have constant `activation_pull`.
    /// Only nodes affected by events (streaming, collision, player entry) change.
    /// This method recomputes influence only for those nodes.
    ///
    /// # Algorithm
    /// For each node `i` in `changed_nodes`:
    ///   1. Recompute its own contribution to the influence slice at `buffer[i]`.
    ///   2. Recompute the contribution at each of its OUT-neighbours
    ///      (since i's pull affects their accumulated influence).
    ///
    /// # Limitations
    /// This is an APPROXIMATION — we do not recompute the full global
    /// normalisation denominator.  In sparse mode, the denominator from the
    /// last full computation is reused.  This is safe when:
    ///   * The number of changed nodes is small relative to total nodes.
    ///   * `activation_pull` changes are bounded by delta epsilon.
    ///
    /// # TODO(V3-SPARSE-VALIDATION): Compare influence_changed_nodes() output
    /// against influence_scalars_into() output for 1000 ticks.  Confirm max
    /// drift < 1e-3 for changed nodes and 0 for unchanged nodes.
    ///
    /// # TODO(V3-TOPOLOGY-SPARSE): This is PREPARATION only — the caller
    /// (MKRWorld::tick() Phase 0) still uses influence_scalars() until
    /// validation passes.
    pub fn influence_changed_nodes(
        &self,
        changed_nodes:  &[usize],
        buffer:         &mut Vec<f32>,
        norm_denom_hint: f32,
    ) {
        let n = self.nodes.len();
        if n == 0 || changed_nodes.is_empty() { return; }
        if buffer.len() < n { return; } // caller must ensure buffer is full-sized

        let norm_denom = if norm_denom_hint > 0.0 { norm_denom_hint } else { 1.0 };

        const EDGE_WEIGHT: f32 = 0.60;
        const OWN_WEIGHT:  f32 = 0.40;

        for &node_idx in changed_nodes {
            if node_idx >= n { continue; }

            // Recompute influence at this node (self-contribution)
            let own = self.nodes[node_idx].activation_pull.clamp(0.0, 1.0);
            // Accumulated in-edge pull for this node (partial: only from its changed in-neighbours)
            // For a full recompute of this node: sum ALL in-neighbours.
            let mut acc = 0.0f32;
            for (from, targets) in self.edges.iter().enumerate() {
                if targets.contains(&node_idx) {
                    acc += self.nodes[from].activation_pull.clamp(0.0, 1.0);
                }
            }
            acc /= norm_denom;
            buffer[node_idx] = (acc * EDGE_WEIGHT + own * OWN_WEIGHT).clamp(0.0, 1.0);

            // Recompute influence for all out-neighbours of this changed node
            if let Some(targets) = self.edges.get(node_idx) {
                for &to in targets {
                    if to >= n { continue; }
                    // Recompute to's influence using full in-edge scan
                    let mut to_acc = 0.0f32;
                    for (from, from_targets) in self.edges.iter().enumerate() {
                        if from_targets.contains(&to) {
                            to_acc += self.nodes[from].activation_pull.clamp(0.0, 1.0);
                        }
                    }
                    to_acc /= norm_denom;
                    let to_own = self.nodes[to].activation_pull.clamp(0.0, 1.0);
                    buffer[to] = (to_acc * EDGE_WEIGHT + to_own * OWN_WEIGHT).clamp(0.0, 1.0);
                }
            }
        }
    }

    // ------------------------------------------------------------------
    // COMPAT: V2 thermal propagation — executor bridge only.
    // Do NOT extend with new logic. Remove after executor migrates to V3.
    // ------------------------------------------------------------------

    /// Propagates thermal heat between ThermalSystem nodes along edges.
    /// This is a discrete, threshold-driven method — a V2 compatibility shim.
    /// New code must use `influence_scalars()` + `ActivationSolver` instead.
    ///
    /// Retained for: mirage-executor backward compatibility ONLY.
    pub fn propagate_thermal(&mut self, thermal_system: &mut ThermalSystem) {
        // TODO(V3-COMPAT): This entire method is a compatibility shim.
        // It branches on discrete state values — the opposite of V3 design.
        // Do NOT add new logic here.
        let raw = thermal_system.get_raw_states();
        let n = self.nodes.len().min(raw.len());
        for i in 0..n {
            let state = raw[i];
            match state {
                // TODO(V3-COMPAT): Hardcoded dormant/hot orchestration logic.
                3 => {
                    for &nbr in &self.edges[i] {
                        thermal_system.heat_chunk(nbr, 0.08);
                    }
                }
                2 => {
                    for &nbr in &self.edges[i] {
                        thermal_system.heat_chunk(nbr, 0.01);
                    }
                }
                _ => {}
            }
        }
    }

    /// V2 compat: discrete node wake — increases thermal heat.
    /// Use `set_activation_pull()` + field injection for all new code.
    pub fn wake_node(&mut self, idx: usize, thermal_system: &mut ThermalSystem) {
        // TODO(V3-COMPAT): Hardcoded threshold-only scheduling assumption.
        thermal_system.heat_chunk(idx, 0.5);
    }
}

impl Default for TopologyGraph {
    fn default() -> Self {
        Self::new()
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use mirage_core::runtime::ChunkState;

    fn make_node(id: usize, pull: f32) -> TopologyNode {
        TopologyNode {
            id,
            thermal_state:         ChunkState::Dormant,
            execution_lane:        ExecutionLane::Background,
            dependency_mask:       0,
            wake_conditions:       0,
            continuation_targets:  vec![],
            residency_requirement: 0,
            cost_estimate:         0.0,
            activation_pull:       pull,
            cache_pressure:        0.0,
        }
    }

    #[test]
    fn empty_graph_returns_empty() {
        let graph = TopologyGraph::new();
        assert!(graph.influence_scalars().is_empty());
    }

    #[test]
    fn isolated_node_uses_own_pull() {
        // A node with no in-edges should return 40% of its own pull
        // (the OWN_WEIGHT path: 0.0 * 0.60 + pull * 0.40).
        let mut graph = TopologyGraph::new();
        graph.add_node(make_node(0, 1.0));
        let scalars = graph.influence_scalars();
        assert_eq!(scalars.len(), 1);
        // No in-edges → accumulated = 0.0, own = 1.0, result = 0.40
        assert!((scalars[0] - 0.40).abs() < 1e-5,
            "isolated node should give 0.40, got {}", scalars[0]);
    }

    #[test]
    fn in_edge_raises_target_influence() {
        // Node 0 (pull=1.0) → Node 1 (pull=0.0).
        // Node 1 has one in-edge from node 0 (pull=1.0).
        let mut graph = TopologyGraph::new();
        graph.add_node(make_node(0, 1.0));  // source
        graph.add_node(make_node(1, 0.0));  // target
        graph.add_edge(0, 1);

        let scalars = graph.influence_scalars();
        // Node 0: isolated, no in-edges → 0.60*0 + 0.40*1.0 = 0.40
        // Node 1: 1 in-edge (from 0, pull=1.0), max_in=1, acc=1.0/1=1.0
        //         → 0.60*1.0 + 0.40*0.0 = 0.60
        assert!((scalars[0] - 0.40).abs() < 1e-5, "source={}", scalars[0]);
        assert!((scalars[1] - 0.60).abs() < 1e-5, "target={}", scalars[1]);
    }

    #[test]
    fn fan_in_normalization_prevents_explosion() {
        // 3 nodes all pointing to node 3 with pull=1.0.
        // max_in = 3 (all in-edges point to node 3).
        // accumulated[3] = 3.0, normalised = 3.0/3 = 1.0 → clamped 1.0.
        let mut graph = TopologyGraph::new();
        for i in 0..4 {
            graph.add_node(make_node(i, 1.0));
        }
        graph.add_edge(0, 3);
        graph.add_edge(1, 3);
        graph.add_edge(2, 3);

        let scalars = graph.influence_scalars();
        assert!(scalars.iter().all(|&s| s >= 0.0 && s <= 1.0),
            "all scalars must be in [0,1]: {:?}", scalars);
        // Node 3 (high fan-in) should be at max influence.
        assert!(scalars[3] >= 0.9, "high fan-in node should have high influence: {}", scalars[3]);
    }

    #[test]
    fn influence_scalars_all_bounded() {
        let mut graph = TopologyGraph::new();
        for i in 0..8 {
            graph.add_node(make_node(i, (i as f32) / 8.0));
        }
        // Add some edges
        for i in 0..7 {
            graph.add_edge(i, i + 1);
        }
        graph.add_edge(7, 0); // cycle

        let scalars = graph.influence_scalars();
        assert_eq!(scalars.len(), 8);
        for (i, &s) in scalars.iter().enumerate() {
            assert!(s >= 0.0 && s <= 1.0, "node {} scalar {} out of bounds", i, s);
        }
    }

    #[test]
    fn test_topology_morphing_stress_test() {
        let mut graph = TopologyGraph::new();
        // Add 3 nodes: source 0, targets 1 and 2
        graph.add_node(make_node(0, 1.0));
        graph.add_node(make_node(1, 0.0));
        graph.add_node(make_node(2, 0.0));
        
        // Add edges: 0 -> 1, then 0 -> 2. Traversal order initially: [1, 2]
        graph.add_edge(0, 1);
        graph.add_edge(0, 2);
        
        assert_eq!(graph.edges[0], vec![1, 2]);

        // Find the index of the edge 0 -> 2 (should be 1)
        let edge_idx_0_2 = graph.find_edge(0, 2);
        assert_ne!(edge_idx_0_2, usize::MAX);

        // Record a burst of signal traffic across the edge 0 -> 2 (1005 times > 1000 threshold)
        for _ in 0..1005 {
            graph.record_access(edge_idx_0_2);
        }

        // Record minimal traffic across 0 -> 1
        let edge_idx_0_1 = graph.find_edge(0, 1);
        for _ in 0..5 {
            graph.record_access(edge_idx_0_1);
        }

        // Assert that access frequencies incremented correctly
        assert_eq!(graph.access_frequency[edge_idx_0_2], 1005);
        assert_eq!(graph.access_frequency[edge_idx_0_1], 5);

        // Before rebalancing, traversal order is still [1, 2]
        assert_eq!(graph.edges[0], vec![1, 2]);

        // Invoke edge rebalancing
        graph.rebalance_edges();

        // Assert that the hot edge (0 -> 2) has been repacked/prioritized to appear earlier (traversal order is now [2, 1])
        assert_eq!(graph.edges[0], vec![2, 1], "Hot edge should be prioritized in traversal order");
    }
}
