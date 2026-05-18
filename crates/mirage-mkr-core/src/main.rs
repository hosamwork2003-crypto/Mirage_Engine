// ===================================================================
// mirage-mkr-core/src/main.rs  (V3 â€” Federated Stabilization Pass)
//
// PURPOSE:
// MKRWorld is the central runtime kernel of Mirage Engine V3.
// This file acts as the binary entry-point and integration test harness.
//
// ---------------------------------------------------------------
// MKR INTERNAL BOUNDARY NOTE (lib.rs migration)
// ---------------------------------------------------------------
// TODO(V3-MKR-INTERNAL): This file is currently main.rs (binary).
// In a future pass, MKRWorld and all subsystems should move to lib.rs,
// with main.rs becoming a thin binary wrapper.  This prepares MKR
// for use as a library crate by the engine orchestration layer.
//
// Migration order:
//   1. Create lib.rs that re-exports all public modules.
//   2. Move MKRWorld struct and impl into lib.rs.
//   3. Keep main.rs as a thin demo/test harness.
//
// DO NOT perform this migration in the current stabilization pass.
// The architecture must be proven stable first.
// ---------------------------------------------------------------
//
// V3 SUBSYSTEM TICK ORDER:
//   Phase 0  Topology pre-pass: compute influence_scalars()
//   Phase 1  Activation field step (decay â†’ diffuse â†’ pressure
//            â†’ activation â†’ execution_probability)
//   Phase 2  Emission gate: collect EmissionRequests from field
//   Phase 3  Renderer bridge: field â†’ chunk_runtime_states
//   Phase 4  [COMPAT] ThermalSystem::update_frame() (renderer compat)
//   Phase 5  [COMPAT] Stabilisation hook
//
// TODO(V3-CEK): Phase 2 output will be consumed by CEK to decide
// which continuation to launch per emission slot.
// ===================================================================


pub mod activation;
pub mod bridge;
pub mod emission;
pub mod emission_validation;  // V4-PASS02: differential emission shadow validation
pub mod pool;
pub mod protocol;   // V3 runtime protocol descriptors
pub mod regions;    // V3-DIFFERENTIAL: activation region partitioning
pub mod streaming;  // Streaming eligibility (NOT execution â€” that is OASIS)

// ===================================================================
// IMPORTS
// ===================================================================

use mirage_core::runtime::ThermalSystem;
use mirage_core::pool::RuntimeDirectory as CoreRuntimeDirectory;
use mirage_matrix::topology::TopologyGraph;

use crate::activation::{
    ActivationField, ActivationSolver, FieldDeltaTracker, PropagationFrontier,
    SparseValidationRunner, ParityComparisonResult,
};
use crate::activation::solver::SolverStepStats;
use crate::activation::frontier::FrontierStats;
use crate::bridge::RendererBridge;
use crate::emission::{EmissionGate, EmissionRequest, EMIT_GATE};
use crate::emission_validation::EmissionShadowValidator;
use crate::regions::RegionMap;

// ===================================================================
// MKR WORLD â€” V3 Runtime
// ===================================================================

/// Central runtime kernel for Mirage Engine V3.
///
/// # V3 Ownership Model
/// | Field                | Role                                        |
/// |----------------------|---------------------------------------------|
/// | `activation_field`   | Live continuous field â€” V3 authority        |
/// | `activation_solver`  | Stateless propagation driver                |
/// | `topology`           | Activation influence graph                  |
/// | `emission_gate`      | Fiber emission decision gate                |
/// | `renderer_bridge`    | Continuousâ†’discrete translation             |
/// | `delta_tracker`      | V3-DIFFERENTIAL: field change detection     |
/// | `region_map`         | V3-DIFFERENTIAL: grid-aligned region grid   |
/// | `propagation_frontier`| V3-DIFFERENTIAL: sparse propagation seeds  |
/// | `sparse_validator`   | V3-SPARSE: parity testing (non-authoritative) |
/// | `thermal`            | COMPAT ONLY â€” renderer/executor shim       |
/// | `directory`          | COMPAT ONLY â€” chunk_runtime_states         |
pub struct MKRWorld {
    // ---------------------------------------------------------------
    // V3 â€” Activation Field Layer (primary runtime authority)
    // ---------------------------------------------------------------

    /// Live continuous activation field over the chunk grid.
    pub activation_field: ActivationField,

    /// Stateless solver: decay â†’ diffuse â†’ pressure â†’ activate â†’ probability.
    pub activation_solver: ActivationSolver,

    /// Statistics from the most recent solver step.
    pub last_step_stats: SolverStepStats,

    // ---------------------------------------------------------------
    // V3 â€” Differential Runtime Substrate (new this pass)
    // ---------------------------------------------------------------

    /// Tracks which cells changed since the last tick.
    ///
    /// TODO(V3-DIFFERENTIAL): Wire into Phase 2 (emission gate) and
    /// Phase 3 (renderer bridge) to skip unchanged cells.
    pub delta_tracker: FieldDeltaTracker,

    /// Grid-aligned region activity map.
    ///
    /// TODO(V3-DIFFERENTIAL): Wire into Phase 2 to skip Dormant regions
    /// entirely during the emission gate scan.
    pub region_map: RegionMap,

    /// Sparse propagation frontier for the next solver step.
    ///
    /// TODO(V3-DIFFERENTIAL): Wire into Phase 1 to run propagate_pressure()
    /// only on frontier cells instead of the full field.
    pub propagation_frontier: PropagationFrontier,

    /// Delta stats from the last tick (for diagnostics and adaptive tuning).
    pub last_frontier_stats: FrontierStats,

    // ---------------------------------------------------------------
    // V3 â€” Sparse Validation (non-authoritative, parity testing only)
    // ---------------------------------------------------------------

    /// Runs the sparse solver in shadow alongside the full solver.
    ///
    /// AUTHORITY: Full solver is always authoritative. Sparse result is
    /// never written to the live field.
    ///
    /// Enable with `world.sparse_validator.enable_parallel()`.
    /// Check results with `world.sparse_validator.report`.
    ///
    /// TODO(V3-SPARSE-VALIDATION): After SPARSE_PROMOTION_THRESHOLD
    /// consecutive passes, promote sparse solver to authoritative mode.
    pub sparse_validator: SparseValidationRunner,

    /// Last parity comparison result (None if validation is disabled).
    pub last_parity: Option<ParityComparisonResult>,

    // ---------------------------------------------------------------
    // V3 â€” Topology Wiring
    // ---------------------------------------------------------------

    /// Activation influence graph.
    ///
    /// Each tick, `topology.influence_scalars()` is called and the
    /// result is fed into `activation_solver.step()`.
    ///
    /// Nodes and edges are set by external subsystems (streaming,
    /// physics, rendering layer) via `topology_mut()`.
    ///
    /// TODO(V3-TOPOLOGY): Enforce field-cell alignment at add_node() time.
    pub topology: TopologyGraph,

    // ---------------------------------------------------------------
    // V3 â€” Fiber Emission Gate
    // ---------------------------------------------------------------

    /// Activation-driven fiber emission gate.
    ///
    /// Scans `execution_probability` each tick; produces a bounded
    /// list of `EmissionRequest`s for cells above the emission threshold.
    ///
    /// TODO(V3-DIFFERENTIAL): Transition to delta-aware emission:
    ///   scan only cells in delta_tracker.mask().iter_changed().
    pub emission_gate: EmissionGate,

    /// Emission requests produced by the gate during the last tick.
    pub last_emission: Vec<EmissionRequest>,

    // ---------------------------------------------------------------
    // V3 â€” Renderer Bridge
    // ---------------------------------------------------------------

    /// Translates `execution_probability` into discrete `ChunkState`.
    ///
    /// TODO(V3-DIFFERENTIAL): Translate only cells flagged as
    /// PROBABILITY_CHANGED in delta_tracker to avoid full-field write.
    pub renderer_bridge: RendererBridge,

    // ---------------------------------------------------------------
    // COMPAT â€” Legacy systems (scheduled for removal)
    // ---------------------------------------------------------------

    /// TODO(V3-COMPAT): ThermalSystem kept only for mirage-executor /
    /// mirage-renderer compatibility.  No longer runtime authority.
    pub thermal: ThermalSystem,

    /// TODO(V3-COMPAT): RuntimeDirectory mirrors chunk_runtime_states.
    pub directory: CoreRuntimeDirectory,

    // ---------------------------------------------------------------
    // V4 â€” Pass 02: Differential Emission Shadow Validation
    // ---------------------------------------------------------------

    /// Shadow emission validator.
    ///
    /// Runs `collect_from_changed()` alongside the authoritative `collect()`
    /// each tick.  Full emission is ALWAYS authoritative.  Shadow output
    /// is stored in `emission_shadow_validator.last_shadow_emission`.
    ///
    /// Enable with `world.emission_shadow_validator.enable_shadow()`.
    /// Check `world.emission_shadow_validator.validation_report` for parity.
    ///
    /// TODO(V4-PASS03): When `eligible_for_promotion()` returns true,
    /// promote `collect_from_changed()` to authority in Pass 03.
    pub emission_shadow_validator: EmissionShadowValidator,


    /// Persistent, reusable topology influence buffer.
    ///
    /// Allocated ONCE in `MKRWorld::new()` and reused every tick.
    /// Replaces the per-tick `Vec<f32>` allocation previously produced
    /// by `topology.influence_scalars()`.
    ///
    /// # Memory contract
    /// * Capacity == `field_width * field_height` at construction.
    /// * NEVER shrinks during runtime.
    /// * Written by `topology.influence_scalars_into()` every tick.
    /// * Read by `activation_solver.step()` every tick.
    /// * Read by `sparse_validator.validate_tick()` every tick.
    /// * No Arc / Mutex / RefCell â€” single-threaded deterministic ownership.
    pub topo_influence_buffer: Vec<f32>,

    /// Monotonic counter of unexpected capacity changes in `topo_influence_buffer`.
    ///
    /// Incremented whenever the buffer capacity changes after the first tick,
    /// which indicates an unplanned reallocation.  Expected steady-state value: 0.
    /// Inspect via `world.topology_buffer_reallocations` in diagnostics / tests.
    pub topology_buffer_reallocations: u64,

    // ---------------------------------------------------------------
    // Frame tracking
    // ---------------------------------------------------------------

    pub frame: u64,
    pub field_width: usize,
    pub field_height: usize,
}

impl MKRWorld {
    /// Create an MKRWorld for a `width Ã— height` chunk grid.
    ///
    /// The activation field, topology graph, emission gate, and renderer
    /// bridge are all sized to the same grid dimensions.
    pub fn new(width: usize, height: usize, _fiber_capacity: usize) -> Self {
        let total_chunks = width * height;
        Self {
            activation_field:   ActivationField::new(width, height),
            activation_solver:  ActivationSolver::new(),
            last_step_stats:    SolverStepStats::default(),

            // V3-DIFFERENTIAL substrate
            delta_tracker:        FieldDeltaTracker::new(total_chunks, EMIT_GATE),
            region_map:           RegionMap::new(width, height),
            propagation_frontier: PropagationFrontier::new(width, height),
            last_frontier_stats:  FrontierStats::default(),

            // V3-SPARSE validation (disabled by default)
            sparse_validator: SparseValidationRunner::new(width, height),
            last_parity:      None,

            topology:           TopologyGraph::new(),
            emission_gate:      EmissionGate::new(),
            last_emission:      Vec::new(),

            renderer_bridge:    RendererBridge::new(),

            thermal:            ThermalSystem::new(total_chunks),
            directory:          CoreRuntimeDirectory::new(total_chunks),

            // V4 â€” topology influence buffer (zero steady-state allocation)
            // Pre-allocate to exact field capacity.  influence_scalars_into()
            // will resize if topology grows beyond this, but for a fixed grid
            // this never happens after construction.
            topo_influence_buffer:          Vec::with_capacity(total_chunks),
            topology_buffer_reallocations:  0,

            // V4 â€” Pass 02: differential emission shadow validator (disabled by default)
            // Enable with: world.emission_shadow_validator.enable_shadow()
            emission_shadow_validator: EmissionShadowValidator::new(),

            frame:              0,
            field_width:        width,
            field_height:       height,
        }
    }

    // ---------------------------------------------------------------
    // External injection API
    // ---------------------------------------------------------------

    /// Inject heat at chunk grid coordinates (x, y).
    ///
    /// Raises the heat signal in the activation field at that position.
    /// TODO(V3-CEK): Will be driven by CEK field packets.
    pub fn inject_heat_at_chunk(&mut self, chunk_x: usize, chunk_y: usize, amount: f32) {
        self.activation_field.inject_heat_at(chunk_x, chunk_y, amount);
    }

    /// Inject execution demand pressure at chunk grid coordinates.
    ///
    /// TODO(V3-CEK): Will be driven by topology edge traversal in CEK.
    pub fn inject_pressure_at_chunk(&mut self, chunk_x: usize, chunk_y: usize, amount: f32) {
        self.activation_field.inject_pressure_at(chunk_x, chunk_y, amount);
    }

    /// Mutable reference to the topology graph.
    ///
    /// Callers (streaming layer, physics, renderer) use this to set
    /// `activation_pull` values on nodes and add directed edges.
    pub fn topology_mut(&mut self) -> &mut TopologyGraph {
        &mut self.topology
    }

    // ---------------------------------------------------------------
    // V3 Tick
    // ---------------------------------------------------------------

    /// Main V3 runtime tick â€” the MKR heartbeat.
    ///
    /// # Phase Execution Order
    ///
    /// **Phase 0 â€” Topology influence pre-pass**
    /// Calls `topology.influence_scalars()` to build the per-cell
    /// topology pull vector.  This is a non-mutating read of the graph.
    ///
    /// **Phase 1 â€” Activation field step**
    /// Passes topology influence into `activation_solver.step()`.
    /// The solver runs: decay â†’ diffuse â†’ pressure â†’ activation â†’
    /// execution_probability over the entire field in one sequential pass.
    ///
    /// **Phase 2 â€” Emission gate**
    /// Scans `execution_probability`; collects cells above `EMIT_GATE`
    /// into `last_emission` (bounded to `MAX_EMIT_PER_TICK`).
    /// Output is sorted by descending probability.
    ///
    /// **Phase 3 â€” Renderer bridge**
    /// Translates `execution_probability` into discrete `ChunkState`
    /// and writes it to `directory.chunk_runtime_states` so the legacy
    /// renderer continues to receive correct state data.
    ///
    /// **Phase 4 â€” [COMPAT] ThermalSystem sync**
    /// Runs `thermal.update_frame()` so mirage-executor's
    /// `ThermalScheduler` still sees non-stale states.
    ///
    /// **Phase 5 â€” [COMPAT] Stabilisation**
    /// Reserved hook for future boundary stabilisation logic.
    pub fn tick(&mut self) {
        // ============================================================
        // PHASE 0 â€” TOPOLOGY INFLUENCE PRE-PASS
        // ============================================================
        // V4 (Authority Migration Pass 01): zero-allocation topology influence.
        // `influence_scalars_into()` writes into the persistent buffer owned
        // by MKRWorld.  No Vec<f32> is allocated on the heap this tick.
        //
        // ALIGNMENT ASSERTION (debug builds only, zero cost in release):
        self.topology.assert_aligned(self.field_width * self.field_height);

        // Record capacity before write so we can detect unexpected reallocations.
        let cap_before = self.topo_influence_buffer.capacity();
        self.topology.influence_scalars_into(&mut self.topo_influence_buffer);

        // Reallocation guard: capacity must never increase after tick 0.
        // In release builds this compiles to nothing (debug_assert).
        debug_assert_eq!(
            self.topo_influence_buffer.capacity(), cap_before,
            "topo_influence_buffer reallocation detected â€” \
             capacity {} â†’ {} (field_width={}, field_height={})",
            cap_before, self.topo_influence_buffer.capacity(),
            self.field_width, self.field_height,
        );
        // Production-safe counter: increment on any capacity change.
        if self.topo_influence_buffer.capacity() != cap_before {
            self.topology_buffer_reallocations =
                self.topology_buffer_reallocations.saturating_add(1);
        }

        // ============================================================
        // PHASE 0.5 â€” PRE-TICK SNAPSHOT FOR SPARSE VALIDATION
        // ============================================================
        // If sparse validation is active, snapshot the field state before
        // the full solver runs so the sparse solver can start from the
        // same initial conditions.
        //
        // TODO(V3-SPARSE-VALIDATION): This snapshot is zero-cost when
        // validation is disabled (branch is predicted never-taken).
        if self.sparse_validator.is_active() {
            self.sparse_validator.snapshot_pre_tick(&self.activation_field);
        }

        // ============================================================
        // PHASE 1 â€” ACTIVATION FIELD STEP (primary runtime work)
        // ============================================================
        // Full solver is ALWAYS authoritative.
        // TODO(V3-DIFFERENTIAL): Replace with sparse step after validation.
        // TODO(V3-SPARSE-VALIDATION): After promotion, step() becomes
        // the fallback and step_sparse() becomes the primary path.
        self.last_step_stats = self.activation_solver.step(
            &mut self.activation_field,
            &self.topo_influence_buffer,
        );

        // ============================================================
        // PHASE 1.5 â€” FIELD DELTA COMPUTATION (V3-DIFFERENTIAL)
        // ============================================================
        // Compute which cells changed since last tick.
        // The resulting mask seeds the propagation frontier.
        //
        // TODO(V3-DIFFERENTIAL): Once frontier is driving propagation,
        // move delta computation BEFORE Phase 1 so the frontier is
        // built from the previous tick's snapshot and Phase 1 only
        // processes frontier cells.
        let delta_mask = self.delta_tracker.compute(&self.activation_field);

        // ============================================================
        // PHASE 1.6 â€” PROPAGATION FRONTIER BUILD (V3-DIFFERENTIAL)
        // ============================================================
        // Expand changed cells to their 4-neighbours.
        // Currently coexists with full propagation (validation mode).
        let used_sparse = self.propagation_frontier.build_from_delta(
            delta_mask,
            self.field_width,
            self.field_height,
        );
        self.last_frontier_stats = FrontierStats {
            frontier_cells: self.propagation_frontier.frontier_size(),
            total_cells:    self.field_width * self.field_height,
            used_sparse,
            density:        self.propagation_frontier.density(),
        };

        // ============================================================
        // PHASE 1.7 â€” REGION MAP REFRESH (V3-DIFFERENTIAL)
        // ============================================================
        // Update region activity states from the refreshed field.
        // TODO(V3-DIFFERENTIAL): Only refresh regions containing changed cells.
        self.region_map.refresh(&self.activation_field);

        // ============================================================
        // PHASE 1.8 â€” SPARSE VALIDATION (V3-SPARSE)
        // ============================================================
        // Run sparse solver on the shadow field and compare against the
        // live field (full solver result).  Full solver stays authoritative.
        //
        // TODO(V3-SPARSE-VALIDATION): Wire validate_tick() result into
        // a runtime diagnostic ring buffer for trending analysis.
        self.last_parity = self.sparse_validator.validate_tick(
            &self.activation_field,
            &self.propagation_frontier,
            &self.topo_influence_buffer,
        );

        // ============================================================
        // PHASE 2 â€” FIBER EMISSION GATE  (AUTHORITATIVE)
        // ============================================================
        // Full-field O(N) scan.  ALWAYS authoritative.
        // TODO(V4-PASS03): After shadow validation achieves
        // PASS_PROMOTION_THRESHOLD, promote collect_from_changed() here.
        let requests = self.emission_gate.collect(&self.activation_field);
        self.last_emission.clear();
        self.last_emission.extend_from_slice(requests);

        // ============================================================
        // PHASE 2.5 â€” DIFFERENTIAL EMISSION SHADOW VALIDATION (V4-PASS02)
        // ============================================================
        // Shadow-runs collect_from_changed() and compares against the
        // authoritative collect() output produced above.
        //
        // Authority: ALWAYS collect().  last_emission is NEVER modified here.
        // The shadow output is stored in:
        //   world.emission_shadow_validator.last_shadow_emission
        //   world.emission_shadow_validator.last_report
        //   world.emission_shadow_validator.validation_report
        //
        // Enable with: world.emission_shadow_validator.enable_shadow()
        // Disable with: world.emission_shadow_validator.disable()
        if self.emission_shadow_validator.is_active() {
            // Pass the delta mask computed in Phase 1.5 and the
            // authoritative requests from Phase 2 to the shadow validator.
            let delta_mask  = self.delta_tracker.mask();
            let authoritative = &self.last_emission;
            self.emission_shadow_validator.validate_tick(
                &self.activation_field,
                delta_mask,
                authoritative,
            );
        }

        // ============================================================
        // PHASE 3 â€” RENDERER BRIDGE (field â†’ chunk_runtime_states)
        // ============================================================
        // TODO(V3-SPARSE-VALIDATION): Add apply_changed_cells() helper
        // that only translates cells flagged PROBABILITY_CHANGED in delta.
        // Current: writes ALL chunk states every frame (O(N)).
        // Future (Task 8): apply_changed_cells(&field, &delta_tracker, &mut directory)
        if self.directory.chunk_runtime_states.len() == self.activation_field.len() {
            self.renderer_bridge.apply_to_directory(
                &self.activation_field,
                &mut self.directory,
            );
        }

        // ============================================================
        // PHASE 4 â€” [COMPAT] ThermalSystem sync
        // TODO(V3-COMPAT): Remove after mirage-executor migration.
        // ============================================================
        self.sync_compat_thermal();

        // ============================================================
        // PHASE 5 â€” [COMPAT] Stabilisation hook
        // TODO(V3-COMPAT): Remove once V3 fiber emission is live.
        // ============================================================
        self.synchronize();

        self.frame = self.frame.wrapping_add(1);

    }

    // ---------------------------------------------------------------
    // Compatibility shims (scheduled for removal)
    // ---------------------------------------------------------------

    /// TODO(V3-COMPAT): Keeps ThermalSystem current for executor compat.
    fn sync_compat_thermal(&mut self) {
        self.thermal.update_frame();
    }

    /// TODO(V3-COMPAT): Placeholder inherited from V2.
    fn synchronize(&mut self) {
        // TODO(V3): Replace with field boundary stabilisation pass.
    }

    // ---------------------------------------------------------------
    // Diagnostic / read API
    // ---------------------------------------------------------------

    /// Mean activation across the entire field.
    pub fn mean_activation(&self) -> f32 {
        self.activation_field.mean_activation()
    }

    /// Mean execution probability across the entire field.
    pub fn mean_execution_probability(&self) -> f32 {
        self.activation_field.mean_execution_probability()
    }

    /// Stats from the most recent solver step.
    pub fn step_stats(&self) -> &SolverStepStats {
        &self.last_step_stats
    }

    /// Immutable access to the activation field (for downstream bridging).
    pub fn activation_field(&self) -> &ActivationField {
        &self.activation_field
    }

    /// Emission requests produced by the most recent tick.
    ///
    /// Non-empty only if cells were above the emission gate this frame.
    /// Cleared at the start of the next tick.
    pub fn emission_requests(&self) -> &[EmissionRequest] {
        &self.last_emission
    }
}

// ===================================================================
// BINARY ENTRY POINT
// ===================================================================

fn main() {
    let mut world = MKRWorld::new(16, 16, 256);

    // Seed the topology graph with some influence nodes so the wiring
    // is actually exercised at startup.
    {
        use mirage_matrix::topology::{TopologyNode, ExecutionLane};
        use mirage_core::runtime::ChunkState;

        let topo = world.topology_mut();

        // Add a hot central node (chunk 8*16+8 = index 136)
        let _center = topo.add_node(TopologyNode {
            id:                    0,
            thermal_state:         ChunkState::Hot,
            execution_lane:        ExecutionLane::Physics,
            dependency_mask:       0,
            wake_conditions:       0,
            continuation_targets:  vec![],
            residency_requirement: 0,
            cost_estimate:         1.0,
            activation_pull:       0.9,
            cache_pressure:        0.2,
        });

        // Add a warm neighbouring node
        let _neighbour = topo.add_node(TopologyNode {
            id:                    1,
            thermal_state:         ChunkState::Resident,
            execution_lane:        ExecutionLane::Streaming,
            dependency_mask:       0,
            wake_conditions:       0,
            continuation_targets:  vec![0],
            residency_requirement: 0,
            cost_estimate:         0.5,
            activation_pull:       0.4,
            cache_pressure:        0.1,
        });

        topo.add_edge(0, 1);
    }

    // Inject heat to seed the field
    world.inject_heat_at_chunk(8, 8, 1.0);
    world.inject_heat_at_chunk(4, 4, 0.7);
    world.inject_pressure_at_chunk(8, 8, 0.5);

    // Run 10 ticks
    for _ in 0..10 {
        world.tick();
    }

    let stats = world.step_stats();
    println!(
        "[MKR V3] step={} mean_activation={:.4} mean_probability={:.4} \
         high_prob_cells={} emission_requests={}",
        stats.step,
        stats.mean_activation,
        stats.mean_execution_probability,
        stats.high_probability_count,
        world.emission_requests().len(),
    );
}

// ===================================================================
// TESTS
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mkr_world_creation() {
        let world = MKRWorld::new(8, 8, 256);
        assert_eq!(world.activation_field.len(), 64);
        assert_eq!(world.frame, 0);
    }

    #[test]
    fn tick_advances_frame() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.tick();
        assert_eq!(world.frame, 1);
    }

    #[test]
    fn heat_injection_propagates_through_tick() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.tick();
        assert!(world.mean_activation() > 0.0);
    }

    #[test]
    fn execution_probability_is_bounded() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.inject_heat_at_chunk(1, 1, 1.0);
        world.tick();
        let p = world.mean_execution_probability();
        assert!(p >= 0.0 && p <= 1.0, "probability out of range: {}", p);
    }

    #[test]
    fn stats_step_matches_tick_count() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.tick();
        world.tick();
        world.tick();
        assert_eq!(world.step_stats().step, 3);
    }

    // ---------------------------------------------------------------
    // Topology wiring tests
    // ---------------------------------------------------------------

    #[test]
    fn topology_influence_reaches_field() {
        use mirage_matrix::topology::{TopologyNode, ExecutionLane};
        use mirage_core::runtime::ChunkState;

        let mut world = MKRWorld::new(4, 4, 64);

        // Add one node with max pull
        world.topology_mut().add_node(TopologyNode {
            id: 0,
            thermal_state: ChunkState::Hot,
            execution_lane: ExecutionLane::Physics,
            dependency_mask: 0,
            wake_conditions: 0,
            continuation_targets: vec![],
            residency_requirement: 0,
            cost_estimate: 1.0,
            activation_pull: 1.0,
            cache_pressure: 0.0,
        });

        world.tick();
        // With topology pull = 1.0 for cell 0, its pressure must be > 0
        // after one tick (injected via solver propagate_pressure phase).
        assert!(
            world.activation_field().cells[0].pressure > 0.0,
            "topology influence should raise cell pressure"
        );
    }

    #[test]
    fn emission_gate_produces_requests_when_hot() {
        let mut world = MKRWorld::new(4, 4, 64);
        // Max heat at every cell â€” forces high execution_probability
        for x in 0..4 {
            for y in 0..4 {
                world.inject_heat_at_chunk(x, y, 1.0);
            }
        }
        world.tick();
        // At least some requests should be emitted
        assert!(
            !world.emission_requests().is_empty(),
            "hot field should produce emission requests"
        );
    }

    #[test]
    fn renderer_bridge_overrides_states_after_tick() {
        let mut world = MKRWorld::new(4, 4, 64);
        // Inject maximum heat â€” bridge should translate to Hot
        for x in 0..4 {
            for y in 0..4 {
                world.inject_heat_at_chunk(x, y, 1.0);
            }
        }
        // Multiple ticks to let probability saturate
        for _ in 0..5 {
            world.tick();
        }
        use mirage_core::runtime::ChunkState;
        let has_non_dormant = world
            .directory
            .chunk_runtime_states
            .iter()
            .any(|&s| s != ChunkState::Dormant);
        assert!(has_non_dormant, "bridge should produce non-dormant states for a hot field");
    }

    #[test]
    fn emission_requests_cleared_each_tick() {
        let mut world = MKRWorld::new(4, 4, 64);
        // Hot tick
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.tick();
        let first_count = world.emission_requests().len();

        // Cold tick (no new heat â€” heat decays)
        for _ in 0..50 {
            world.tick();
        }
        let later_count = world.emission_requests().len();

        // After enough decay, emission count should fall
        assert!(
            later_count <= first_count,
            "emission count should not grow without new heat: {} vs {}",
            later_count,
            first_count
        );
    }

    // ---------------------------------------------------------------
    // V4 â€” Authority Migration Pass 01: Allocation validation tests
    // ---------------------------------------------------------------

    #[test]
    fn topo_influence_buffer_capacity_reserved_at_construction() {
        // The buffer must be pre-allocated to the exact field size.
        // capacity() >= len() always; we check that at least total_chunks
        // were reserved so the first influence_scalars_into() never grows.
        let world = MKRWorld::new(8, 8, 256);
        assert!(
            world.topo_influence_buffer.capacity() >= 64,
            "expected capacity >= 64, got {}",
            world.topo_influence_buffer.capacity()
        );
    }

    #[test]
    fn topo_influence_buffer_zero_reallocations_after_n_ticks() {
        // Run N ticks and assert the reallocation counter stays at zero.
        // This validates steady-state zero-allocation behaviour.
        let mut world = MKRWorld::new(8, 8, 256);
        {
            use mirage_matrix::topology::{TopologyNode, ExecutionLane};
            use mirage_core::runtime::ChunkState;
            let topo = world.topology_mut();
            topo.add_node(TopologyNode {
                id: 0, thermal_state: ChunkState::Hot,
                execution_lane: ExecutionLane::Physics,
                dependency_mask: 0, wake_conditions: 0,
                continuation_targets: vec![], residency_requirement: 0,
                cost_estimate: 1.0, activation_pull: 0.8, cache_pressure: 0.1,
            });
        }
        world.inject_heat_at_chunk(0, 0, 0.5);
        for _ in 0..20 {
            world.tick();
        }
        assert_eq!(
            world.topology_buffer_reallocations, 0,
            "expected zero reallocations, got {}",
            world.topology_buffer_reallocations
        );
    }

    #[test]
    fn topo_influence_buffer_values_in_valid_range() {
        // After a tick, every element in the buffer must be in [0.0, 1.0].
        // This validates that influence_scalars_into() produces the same
        // bounded output as influence_scalars().
        let mut world = MKRWorld::new(4, 4, 64);
        {
            use mirage_matrix::topology::{TopologyNode, ExecutionLane};
            use mirage_core::runtime::ChunkState;
            let topo = world.topology_mut();
            topo.add_node(TopologyNode {
                id: 0, thermal_state: ChunkState::Resident,
                execution_lane: ExecutionLane::Streaming,
                dependency_mask: 0, wake_conditions: 0,
                continuation_targets: vec![], residency_requirement: 0,
                cost_estimate: 0.5, activation_pull: 0.6, cache_pressure: 0.1,
            });
        }
        world.tick();
        for &val in &world.topo_influence_buffer {
            assert!(
                (0.0..=1.0).contains(&val),
                "topo_influence_buffer contains out-of-range value: {val}"
            );
        }
    }

    // ---------------------------------------------------------------
    // V4 -- Pass 02: Shadow emission validator integration tests
    // ---------------------------------------------------------------

    #[test]
    fn emission_shadow_validator_disabled_by_default() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.tick();
        assert!(
            world.emission_shadow_validator.last_report.is_none(),
            "disabled validator must produce no report"
        );
        assert_eq!(world.emission_shadow_validator.validation_report.ticks_run, 0);
    }

    #[test]
    fn emission_shadow_validator_parity_over_n_ticks() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.emission_shadow_validator.enable_shadow();
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.inject_heat_at_chunk(4, 4, 0.8);
        world.inject_pressure_at_chunk(2, 2, 0.5);

        for tick in 0..20 {
            world.tick();
            if let Some(ref report) = world.emission_shadow_validator.last_report {
                assert_eq!(report.missing_from_differential, 0,
                    "tick {tick}: missing cells in differential");
                assert_eq!(report.extra_in_differential, 0,
                    "tick {tick}: extra cells in differential");
            }
        }

        let vr = &world.emission_shadow_validator.validation_report;
        assert_eq!(vr.ticks_run, 20);
        assert_eq!(vr.ticks_failed, 0);
        assert_eq!(vr.peak_missing, 0);
        assert_eq!(vr.peak_extra, 0);
    }
}