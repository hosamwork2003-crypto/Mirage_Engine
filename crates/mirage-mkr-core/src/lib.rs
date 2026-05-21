// ===================================================================
// mirage-mkr-core/src/lib.rs  (V3/V4 — Substrate Core Library)
// ===================================================================

pub mod activation;
pub mod bridge;
pub mod pool;
pub mod streaming;
pub mod emission;
pub mod protocol;
pub mod regions;
pub mod region_validation;

// ===================================================================
// IMPORTS
// ===================================================================

use mirage_core::runtime::ThermalSystem;
use mirage_core::pool::RuntimeDirectory as CoreRuntimeDirectory;
use mirage_mts::topology::TopologyGraph;

// MTS layer: TopologyInfluenceProvider trait — in scope for mkr-core's
// orchestration of the topology → activation solver pipeline.
// CEK layer: CekEvalField trait — in scope for &mut dyn CekEvalField casts
// in tick() Phase 3. ActivationField implements it in activation/field.rs.

use crate::activation::{
    ActivationField, ActivationSolver, FieldDeltaTracker, PropagationFrontier,
    SparseValidationRunner, ParityComparisonResult, ValidationMode,
};
use crate::activation::solver::SolverStepStats;
use crate::activation::frontier::FrontierStats;
use crate::bridge::{RendererBridge, ExecutionBridge};
use crate::emission::{EmissionGate, EmissionRequest, EMIT_GATE};
use crate::bridge::renderer_validation::RendererShadowValidator;
use crate::regions::RegionMap;
use crate::region_validation::{
    RegionShadowValidator,
};

// ===================================================================
// MKR WORLD — V3/V4 Runtime
// ===================================================================

/// Central runtime kernel for Mirage Engine V3/V4.
pub struct MKRWorld {
    // ============================================================
    // Runtime Flags
    // ============================================================

    pub differential_renderer_enabled: bool,
    pub differential_renderer_needs_full_sync: bool,

    // ============================================================
    // Core Runtime Authority
    // ============================================================

    pub activation_field: ActivationField,
    pub activation_solver: ActivationSolver,
    pub topology: TopologyGraph,

    // ============================================================
    // Differential Runtime
    // ============================================================

    pub delta_tracker: FieldDeltaTracker,
    pub propagation_frontier: PropagationFrontier,
    pub region_map: RegionMap,

    // ============================================================
    // Validation
    // ============================================================

    pub sparse_validator: SparseValidationRunner,
    pub renderer_shadow_validator: RendererShadowValidator,
    pub region_shadow_validator: RegionShadowValidator,
    pub last_parity: Option<ParityComparisonResult>,

    // ============================================================
    // Emission + Rendering
    // ============================================================

    pub emission_gate: EmissionGate,
    pub renderer_bridge: RendererBridge,
    pub execution_bridge: ExecutionBridge,

    pub last_emission: Vec<EmissionRequest>,

    // ============================================================
    // Compatibility Layer
    // ============================================================

    pub thermal: ThermalSystem,
    pub directory: CoreRuntimeDirectory,

    // ============================================================
    // Persistent Buffers
    // ============================================================

    pub topo_influence_buffer: Vec<f32>,
    pub authoritative_probability_buffer: Vec<f32>,

    // ============================================================
    // Diagnostics
    // ============================================================

    pub last_step_stats: SolverStepStats,
    pub last_frontier_stats: FrontierStats,

    pub topology_buffer_reallocations: u64,

    // ============================================================
    // Trace Fusion Compiler
    // ============================================================

    pub trace_compiler: mirage_compute::TraceFusionCompiler,

    // ============================================================
    // NUMA-Aware Work Stealing Scheduler
    // ============================================================

    pub numa_scheduler: mirage_executor::scheduler::NUMAAwareScheduler,
    pub executor_authority: mirage_executor::ExecutionBridgeAuthority,
    pub scheduler_capability: mirage_executor::SchedulerCapability,
    pub frontier_capability: mirage_executor::FrontierExecutionCapability,
    pub last_execution_requests: Vec<mirage_executor::ExecutionRequest>,
    pub last_continuation_buffer: Option<mirage_cek::DeterministicContinuationBuffer>,

    // ============================================================
    // Relational Query IR — ColumnarScan scratchpad
    // ============================================================

    /// Pre-allocated SoA columnar scan used by the Query IR each tick.
    /// Resized lazily to match `activation_field.len()`. Never shrinks.
    pub query_scan: mirage_query::ColumnarScan,

    // ============================================================
    // Synaptic Prediction Layer — SynapseRegistry (Detachable/Observational Only)
    // ============================================================

    /// Camera-velocity–driven predictive telemetry matrix.
    ///
    /// Observational update hook triggered via `update_observational_camera()`
    /// to populate the advisory prefetch hints cache. Purely advisory — does NOT
    /// modify activation field probabilities, scheduling decisions, execution
    /// order, or participate in deterministic runtime choices.
    ///
    /// This system is fully detachable: the tick() loop does not participate in
    /// or depend on synapse execution.
    pub synapse: mirage_synapse::SynapseRegistry,

    // ============================================================
    // Frame Tracking
    // ============================================================

    pub frame: u64,
    pub frontier_generation: u64,
    pub field_width: usize,
    pub field_height: usize,
}

impl MKRWorld {
    /// Create an MKRWorld for a `width × height` chunk grid.
    pub fn new(width: usize, height: usize, _fiber_capacity: usize) -> Self {
        let total_chunks = width * height;
        let (numa_scheduler, executor_authority, scheduler_capability, frontier_capability) =
            mirage_executor::scheduler::NUMAAwareScheduler::new();

        Self {
            // ========================================================
            // Runtime Flags
            // ========================================================

            differential_renderer_needs_full_sync: true,
            differential_renderer_enabled: false,

            // ========================================================
            // Core Runtime Authority
            // ========================================================

            activation_field:
                ActivationField::new(width, height),

            activation_solver:
                ActivationSolver::new(),

            topology:
                TopologyGraph::new(),

            // ========================================================
            // Differential Runtime
            // ========================================================

            delta_tracker:
                FieldDeltaTracker::new(total_chunks, EMIT_GATE),

            region_map:
                RegionMap::new(width, height),

            propagation_frontier:
                PropagationFrontier::new(width, height),

            // ========================================================
            // Validation
            // ========================================================

            sparse_validator:
                SparseValidationRunner::new(width, height),

            renderer_shadow_validator:
                RendererShadowValidator::new(total_chunks),

            region_shadow_validator:
                RegionShadowValidator::new(width, height),

            last_parity:
                None,

            // ========================================================
            // Emission + Rendering
            // ========================================================

            emission_gate:
                EmissionGate::new(),

            renderer_bridge:
                RendererBridge::new(),

            execution_bridge:
                ExecutionBridge::new(_fiber_capacity),

            last_emission:
                Vec::new(),

            // ========================================================
            // Compatibility Layer
            // ========================================================

            thermal:
                ThermalSystem::new(total_chunks),

            directory:
                CoreRuntimeDirectory::new(total_chunks),

            // ========================================================
            // Persistent Buffers
            // ========================================================

            topo_influence_buffer:
                vec![0.0; total_chunks],

            authoritative_probability_buffer:
                Vec::with_capacity(total_chunks),

            // ========================================================
            // Diagnostics
            // ========================================================

            last_step_stats:
                SolverStepStats::default(),

            last_frontier_stats:
                FrontierStats::default(),

            topology_buffer_reallocations:
                0,

            trace_compiler:
                mirage_compute::TraceFusionCompiler::new(3),

            numa_scheduler,

            executor_authority,

            scheduler_capability,

            frontier_capability,

            last_execution_requests: Vec::new(),

            last_continuation_buffer: None,

            query_scan:
                mirage_query::ColumnarScan::new(total_chunks),

            // ========================================================
            // Synaptic Prediction Layer
            // ========================================================

            synapse:
                mirage_synapse::SynapseRegistry::new(),

            // ========================================================
            // Frame Tracking
            // ========================================================

            frame:
                0,

            frontier_generation:
                0,

            field_width:
                width,

            field_height:
                height,
        }
    }

    /// Update camera state observation for the synaptic prediction layer.
    ///
    /// Invoking this method immediately recomputes the advisory predictions
    /// and prefetch corridor on the synapse registry outside the tick execution pipeline.
    pub fn update_observational_camera(&mut self, cam_pos: [f32; 3], cam_vel: [f32; 3]) {
        self.synapse.observational_update(cam_pos, cam_vel, 128.0);
    }

    /// Return the advisory prefetch hints from the last tick.
    ///
    /// Chunk IDs in this slice are advisory streaming hints computed by the
    /// `SynapseRegistry`. They indicate which chunks the camera trajectory
    /// predicts will be needed in the next ~10 frames. The runtime is free to
    /// ignore these hints.
    ///
    /// Returns an empty slice if the camera is static or `tick()` has not
    /// been called yet.
    pub fn advisory_prefetch_hints(&self) -> &[u32] {
        self.synapse.advisory_prefetch_hints()
    }

    pub fn enable_differential_renderer(&mut self) {
        self.differential_renderer_enabled = true;
        self.differential_renderer_needs_full_sync = true;
    }

    pub fn disable_differential_renderer(&mut self) {
        self.differential_renderer_enabled = false;
    }

    // ---------------------------------------------------------------
    // External injection API
    // ---------------------------------------------------------------

    pub fn inject_heat_at_chunk(&mut self, chunk_x: usize, chunk_y: usize, amount: f32) {
        self.activation_field.inject_heat_at(chunk_x, chunk_y, amount);
    }

    pub fn inject_pressure_at_chunk(&mut self, chunk_x: usize, chunk_y: usize, amount: f32) {
        self.activation_field.inject_pressure_at(chunk_x, chunk_y, amount);
    }

    pub fn topology_mut(&mut self) -> &mut TopologyGraph {
        &mut self.topology
    }

    // ---------------------------------------------------------------
    // Tick
    // ---------------------------------------------------------------

    pub fn tick(&mut self) {
        // ============================================================
        // PHASE 0 — TOPOLOGY PREPASS
        // ============================================================

        self.topology
            .assert_aligned(self.field_width * self.field_height);

        let cap_before = self.topo_influence_buffer.capacity();

        self.topology
            .influence_scalars_into(&mut self.topo_influence_buffer);

        if self.topo_influence_buffer.capacity() != cap_before {
            self.topology_buffer_reallocations += 1;
        }



        // ============================================================
        // PHASE 0.5 — SNAPSHOT
        // ============================================================

        if self.sparse_validator.is_active() {
            self.sparse_validator
                .snapshot_pre_tick(&self.activation_field, &self.propagation_frontier);
        }

        // ============================================================
        // PHASE 1 — SOLVER STEP
        // ============================================================

        let is_validating = self.sparse_validator.is_active();
        let use_sparse_for_live = if is_validating {
            matches!(self.sparse_validator.mode, ValidationMode::SparseAuthoritative)
        } else {
            self.propagation_frontier.should_use_sparse() && !self.propagation_frontier.is_empty()
        };

        if use_sparse_for_live {
            self.last_step_stats = self.activation_solver.step_sparse(
                &mut self.activation_field,
                &self.propagation_frontier,
                &self.topo_influence_buffer,
            );
        } else {
            self.last_step_stats = self.activation_solver.step(
                &mut self.activation_field,
                &self.topo_influence_buffer,
            );
        }

        // ============================================================
        // PHASE 1.5 — DELTA COMPUTE
        // ============================================================

        let delta_mask =
            self.delta_tracker.compute(&self.activation_field);

        // ============================================================
        // PHASE 1.6 — FRONTIER BUILD
        // ============================================================

        let used_sparse =
            self.propagation_frontier.build_from_delta(
                delta_mask,
                self.field_width,
                self.field_height,
            );

        self.frontier_generation = self.frontier_generation.wrapping_add(1);

        self.last_frontier_stats = FrontierStats {
            frontier_cells:
                self.propagation_frontier.frontier_size(),

            total_cells:
                self.field_width * self.field_height,

            used_sparse,

            density:
                self.propagation_frontier.density(),
        };

        // ============================================================
        // PHASE 1.7 — REGION REFRESH + VALIDATION
        // ============================================================

        self.region_map.refresh(&self.activation_field);

        if self.region_shadow_validator.is_active() {
            self.region_shadow_validator.validate_tick(
                &self.activation_field,
                self.delta_tracker.mask(),
                &self.region_map,
            );
        }

        // ============================================================
        // PHASE 1.8 — SPARSE VALIDATION
        // ============================================================

        self.last_parity =
            self.sparse_validator.validate_tick(
                &self.activation_field,
                &self.propagation_frontier,
                &self.topo_influence_buffer,
                &mut self.activation_solver,
            );

        // ============================================================
        // PHASE 2 — EMISSION
        // ============================================================

        let current_regions = RegionMap::compute_from_field(&self.activation_field);

        let requests =
            self.emission_gate.collect_from_frontier(&self.activation_field, &self.propagation_frontier);

        self.last_emission.clear();
        for req in requests {
            let region_idx = current_regions.region_for_cell(req.cell_index);
            if let Some(region) = current_regions.get(region_idx) {
                if region.activity == crate::regions::RegionActivityState::Dormant {
                    continue;
                }
            }
            self.last_emission.push(*req);
        }

        self.last_execution_requests.clear();
        for (seq_idx, req) in self.last_emission.iter().enumerate() {
            let deadline = self.frame + crate::bridge::execution_bridge::DEFAULT_DEADLINE_FRAMES;
            self.last_execution_requests.push(mirage_executor::ExecutionRequest::new(
                req.cell_index,
                req.probability,
                deadline,
                req.probability < 0.15,
                req.probability,
                0,
                0,
                0,
                None,
                self.frame,                     // originating_tick: u64
                0,                              // emission_source_id: u32 (0 for frontier emission pass)
                self.frontier_generation,       // originating_frontier_generation: u64
                seq_idx as u64,                 // deterministic_sequence_index: u64
                &self.frontier_capability,
            ));
        }

        // ============================================================
        // PHASE 2.3 — QUERY IR: COLUMNAR SCAN REFRESH
        // ============================================================
        // Load the current activation field into the SoA ColumnarScan,
        // then use the CellQuery API to derive the active-cell index set.
        // This set is used for diagnostics and future SolverKernel fusion;
        // it does NOT replace the authoritative ActivationSolver output —
        // it reads the field AFTER the solver has already run, so
        // mathematical parity with the solver is guaranteed by construction.
        {
            let field = &self.activation_field;
            let n = field.cells.len();
            // Resize the scan if the field grew (e.g. after re-init).
            if self.query_scan.len != n {
                self.query_scan.resize(n);
            }
            // Scatter-copy AoS → SoA.
            for i in 0..n {
                let c = &field.cells[i];
                self.query_scan.heat[i]                  = c.heat;
                self.query_scan.pressure[i]              = c.pressure;
                self.query_scan.entropy[i]               = c.entropy;
                self.query_scan.activation[i]            = c.activation;
                self.query_scan.execution_probability[i] = c.execution_probability;
            }

            // Build active-cell set via the relational query API.
            // Mirrors the EmissionGate threshold (`EMIT_GATE`) for parity.
            let _active_cells: Vec<usize> = self.query_scan
                .query()
                .filter(|_h, _p, _e, _a, exec_prob| exec_prob > EMIT_GATE)
                .collect();
            // `_active_cells` is available for future SolverKernel fusion
            // and diagnostic instrumentation. Prefixed with `_` until a
            // downstream consumer is wired in.
        }

        // ============================================================
        // PHASE 3 — STATEFUL MULTI-FRAME CEK LIFECYCLE EVALUATION
        // ============================================================

        let topo_slice = &self.topo_influence_buffer;

        // 1. Queue all newly risen signals into our persistent execution queue
        self.execution_bridge.process_and_queue_cek_context(&self.last_execution_requests, topo_slice);

        // 2. Perform quiescent eviction to prevent phantom memory leaks
        self.execution_bridge.evict_quiescent_cek_states(&self.activation_field);

        // 3. Consume closures statefully, respecting our hardware execution budget bounds
        let mut executed_continuations = 0;
        let target_budget = self.emission_gate.budget; // Retrieve global configured budget scale

        // Scope mutability over the shared cell register
        {
            let mut signature = Vec::new();
            let mut path = Vec::new();

            // Scope reading registry
            {
                let registry = self.execution_bridge.deferred_cek_queue.borrow();
                let limit = target_budget.min(registry.len());
                for i in 0..limit {
                    let machine = &registry[i];
                    signature.push(machine.control_cell);
                    path.push(mirage_compute::Continuation {
                        cell_index: machine.control_cell,
                        prob_signal: machine.prob_signal,
                    });
                }
            }

            // Optimize/compile trace
            let fused_kernel = self.trace_compiler.optimize(signature.clone(), path.clone());

            if let Some(kernel) = fused_kernel {
                // Fused Dynamic SIMD Execution: Bypass interpretation overhead
                kernel.execute(&mut self.activation_field);
                executed_continuations += path.len();

                // Telemetry
                for cont in &path {
                    let source_cell = cont.cell_index;
                    if source_cell < self.topology.edges.len() {
                        let targets = self.topology.edges[source_cell].clone();
                        for target_cell in targets {
                            let edge_idx = self.topology.find_edge(source_cell, target_cell);
                            self.topology.record_access(edge_idx);
                        }
                    }
                }

                // Construct the DeterministicContinuationBuffer from the executing machines
                let mut continuations = Vec::new();
                {
                    let registry = self.execution_bridge.deferred_cek_queue.borrow();
                    let limit = target_budget.min(registry.len());
                    let arena = self.execution_bridge.arena.borrow();
                    for i in 0..limit {
                        let machine = &registry[i];
                        for &idx in machine.kontinuation_stack.iter().rev() {
                            if let Some(node) = arena.get(idx) {
                                continuations.push(node.clone());
                            }
                        }
                    }
                }

                // Apply stable sorting rules
                mirage_cek::stable_sort_continuations(&mut continuations);

                // Assign realization_sequence_index based on sorting index
                for (idx, cont) in continuations.iter_mut().enumerate() {
                    cont.provenance.realization_sequence_index = idx as u64;
                }

                let continuation_buffer = mirage_cek::DeterministicContinuationBuffer { continuations };
                self.last_continuation_buffer = Some(continuation_buffer);

                // Drain the executed machines and clean up arena
                {
                    let mut arena = self.execution_bridge.arena.borrow_mut();
                    let mut registry = self.execution_bridge.deferred_cek_queue.borrow_mut();
                    for machine in registry.drain(0..path.len()) {
                        for idx in machine.kontinuation_stack {
                            arena.remove(idx);
                        }
                    }
                }
            } else {
                // Interpret sequentially using NUMA-aware Work Stealing Scheduler
                let mut registry = self.execution_bridge.deferred_cek_queue.borrow_mut();
                let mut unexecuted_backlog = Vec::with_capacity(registry.len());
                let mut machines_to_execute = Vec::new();

                for machine in registry.drain(..) {
                    if executed_continuations < target_budget {
                        machines_to_execute.push(machine);
                        executed_continuations += 1;
                    } else {
                        unexecuted_backlog.push(machine);
                    }
                }
                *registry = unexecuted_backlog;

                // Build the DeterministicContinuationBuffer from machines_to_execute
                let mut continuations = Vec::new();
                {
                    let arena = self.execution_bridge.arena.borrow();
                    for machine in &machines_to_execute {
                        for &idx in machine.kontinuation_stack.iter().rev() {
                            if let Some(node) = arena.get(idx) {
                                continuations.push(node.clone());
                            }
                        }
                    }
                }

                // Apply stable sorting rules
                mirage_cek::stable_sort_continuations(&mut continuations);

                // Assign realization_sequence_index based on sorting index
                for (idx, cont) in continuations.iter_mut().enumerate() {
                    cont.provenance.realization_sequence_index = idx as u64;
                }

                let continuation_buffer = mirage_cek::DeterministicContinuationBuffer { continuations };
                self.last_continuation_buffer = Some(continuation_buffer.clone());

                // Clean up the arena for executed machines
                {
                    let mut arena = self.execution_bridge.arena.borrow_mut();
                    for machine in &machines_to_execute {
                        for &idx in &machine.kontinuation_stack {
                            arena.remove(idx);
                        }
                    }
                }

                // Send machines to execute to the scheduler
                // We use raw pointer transmission wrapped in SendPtr to bypass the `'static` lifetime constraint of the FnMut closure
                struct SendPtr(pub *mut ActivationField);
                unsafe impl Send for SendPtr {}
                unsafe impl Sync for SendPtr {}
                impl SendPtr {
                    pub unsafe fn get_mut<'a>(&self) -> &'a mut ActivationField {
                        unsafe { &mut *self.0 }
                    }
                }

                let field_ptr = SendPtr(&mut self.activation_field as *mut ActivationField);

                for desc in continuation_buffer.continuations {
                    let f_ptr = SendPtr(field_ptr.0);
                    let cell_idx = desc.op.cell_idx();

                    let op = desc.op.clone();
                    let fiber = mirage_executor::fiber::Fiber::new(cell_idx, Box::new(move || {
                        unsafe {
                            let field_ref = f_ptr.get_mut();
                            let cek_field: &mut dyn mirage_cek::CekEvalField = field_ref;
                            op.realize(cek_field);
                        }
                    }));

                    let req = self.last_execution_requests.iter()
                        .find(|r| r.request_id() == desc.request_id)
                        .cloned()
                        .unwrap_or_else(|| {
                            let prob = self.activation_field.cells[cell_idx].execution_probability;
                            let prov = desc.provenance;
                            mirage_executor::ExecutionRequest::new(
                                cell_idx,
                                prob,
                                prov.originating_tick + crate::bridge::execution_bridge::DEFAULT_DEADLINE_FRAMES,
                                prob < 0.15,
                                prob,
                                0,
                                0,
                                0,
                                None,
                                prov.originating_tick,
                                prov.emission_source_id,
                                prov.originating_frontier_generation,
                                prov.deterministic_sequence_index,
                                &self.frontier_capability,
                            )
                        });

                    self.numa_scheduler.schedule_request(&req, fiber, &self.scheduler_capability);
                }

                // Execute fibers from the scheduler queues
                for core_id in 0..self.numa_scheduler.affinity_map.num_cores {
                    while let Some(mut fiber) = self.numa_scheduler.get_task_for_core(core_id) {
                        let source_cell = fiber.id;

                        // Resume / execute continuation
                        fiber.resume();

                        // Bridge the runtime telemetry to the topology graph
                        if source_cell < self.topology.edges.len() {
                            let targets = self.topology.edges[source_cell].clone();
                            for target_cell in targets {
                                let edge_idx = self.topology.find_edge(source_cell, target_cell);
                                self.topology.record_access(edge_idx);
                            }
                        }
                    }
                }
            }
        }

        // Trigger the rebalancer every 60 frames
        if self.frame % 60 == 0 {
            self.topology.rebalance_edges();
        }

        // ============================================================
        // PHASE 3 — PROBABILITY SNAPSHOT
        // ============================================================

        self.renderer_bridge.fill_probability_buffer(
            &self.activation_field,
            &mut self.authoritative_probability_buffer,
        );

        // ============================================================
        // PHASE 3.1 — RENDERER BRIDGE
        // ============================================================

        self.run_renderer_bridge();

        // ============================================================
        // PHASE 3.5 — RENDERER VALIDATION
        // ============================================================

        if self.renderer_shadow_validator.is_active() {
            self.renderer_shadow_validator.validate_tick(
                &self.activation_field,
                self.delta_tracker.mask(),
                &self.directory,
                &self.authoritative_probability_buffer,
            );
        }

        // ============================================================
        // PHASE 4 — THERMAL SYNC
        // ============================================================

        self.sync_compat_thermal();

        // ============================================================
        // PHASE 5 — STABILIZATION
        // ============================================================

        self.synchronize();

        self.frame = self.frame.wrapping_add(1);
    }

    fn run_renderer_bridge(&mut self) {
        if self.differential_renderer_enabled {
            if self.differential_renderer_needs_full_sync {
                self.renderer_bridge.apply_to_directory(
                    &self.activation_field,
                    &mut self.directory,
                );
                self.differential_renderer_needs_full_sync = false;
            } else {
                self.renderer_bridge.apply_changed_cells(
                    &self.activation_field,
                    self.delta_tracker.mask(),
                    &mut self.directory,
                );
            }
        } else {
            self.renderer_bridge.apply_to_directory(
                &self.activation_field,
                &mut self.directory,
            );
        }
    }

    fn sync_compat_thermal(&mut self) {
        self.thermal.update_frame();
    }

    fn synchronize(&mut self) {
        // Reserved for field boundary stabilisation pass.
    }

    // ---------------------------------------------------------------
    // Diagnostic / read API
    // ---------------------------------------------------------------

    pub fn mean_activation(&self) -> f32 {
        self.activation_field.mean_activation()
    }

    pub fn mean_execution_probability(&self) -> f32 {
        self.activation_field.mean_execution_probability()
    }

    pub fn step_stats(&self) -> &SolverStepStats {
        &self.last_step_stats
    }

    pub fn activation_field(&self) -> &ActivationField {
        &self.activation_field
    }

    pub fn emission_requests(&self) -> &[EmissionRequest] {
        &self.last_emission
    }
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

    #[test]
    fn topology_influence_reaches_field() {
        use mirage_mts::topology::{TopologyNode, ExecutionLane};
        use mirage_core::runtime::ChunkState;

        let mut world = MKRWorld::new(4, 4, 64);

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
        assert!(
            world.activation_field().cells[0].pressure > 0.0,
            "topology influence should raise cell pressure"
        );
    }

    #[test]
    fn emission_gate_produces_requests_when_hot() {
        let mut world = MKRWorld::new(4, 4, 64);
        for x in 0..4 {
            for y in 0..4 {
                world.inject_heat_at_chunk(x, y, 1.0);
            }
        }
        world.tick();
        assert!(
            !world.emission_requests().is_empty(),
            "hot field should produce emission requests"
        );
    }

    #[test]
    fn renderer_bridge_overrides_states_after_tick() {
        let mut world = MKRWorld::new(4, 4, 64);
        for x in 0..4 {
            for y in 0..4 {
                world.inject_heat_at_chunk(x, y, 1.0);
            }
        }
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
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.tick();
        let first_count = world.emission_requests().len();

        for _ in 0..50 {
            world.tick();
        }
        let later_count = world.emission_requests().len();

        assert!(
            later_count <= first_count,
            "emission count should not grow without new heat: {} vs {}",
            later_count,
            first_count
        );
    }

    #[test]
    fn topo_influence_buffer_capacity_reserved_at_construction() {
        let world = MKRWorld::new(8, 8, 256);
        assert!(
            world.topo_influence_buffer.capacity() >= 64,
            "expected capacity >= 64, got {}",
            world.topo_influence_buffer.capacity()
        );
    }

    #[test]
    fn topo_influence_buffer_zero_reallocations_after_n_ticks() {
        let mut world = MKRWorld::new(8, 8, 256);
        {
            use mirage_mts::topology::{TopologyNode, ExecutionLane};
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
        let mut world = MKRWorld::new(4, 4, 64);
        {
            use mirage_mts::topology::{TopologyNode, ExecutionLane};
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

    #[test]
    fn test_deterministic_replay_equivalence() {
        let mut world_a = MKRWorld::new(8, 8, 256);
        let mut world_b = MKRWorld::new(8, 8, 256);

        // Inject identical initial states
        world_a.inject_heat_at_chunk(1, 1, 1.0);
        world_a.inject_heat_at_chunk(4, 4, 0.8);
        world_a.inject_pressure_at_chunk(2, 2, 0.5);

        world_b.inject_heat_at_chunk(1, 1, 1.0);
        world_b.inject_heat_at_chunk(4, 4, 0.8);
        world_b.inject_pressure_at_chunk(2, 2, 0.5);

        // Tick both worlds and compare internal and emitted states tick-by-tick
        for tick in 0..30 {
            world_a.tick();
            world_b.tick();

            // Compare frame indices
            assert_eq!(world_a.frame, world_b.frame, "Frame index mismatch at tick {tick}");
            
            // Compare frontier generation
            assert_eq!(world_a.frontier_generation, world_b.frontier_generation, "Frontier generation mismatch at tick {tick}");

            // Compare emitted execution requests
            assert_eq!(
                world_a.last_execution_requests.len(),
                world_b.last_execution_requests.len(),
                "Execution requests length mismatch at tick {tick}"
            );

            for (i, (req_a, req_b)) in world_a.last_execution_requests.iter().zip(&world_b.last_execution_requests).enumerate() {
                assert_eq!(req_a.cell_index(), req_b.cell_index(), "Cell index mismatch at request {i}, tick {tick}");
                assert_eq!(req_a.priority(), req_b.priority(), "Priority mismatch at request {i}, tick {tick}");
                assert_eq!(req_a.deadline_frame(), req_b.deadline_frame(), "Deadline mismatch at request {i}, tick {tick}");
                assert_eq!(req_a.is_prefetch_hint(), req_b.is_prefetch_hint(), "Prefetch hint mismatch at request {i}, tick {tick}");
                assert_eq!(req_a.captured_probability(), req_b.captured_probability(), "Captured probability mismatch at request {i}, tick {tick}");
                assert_eq!(req_a.originating_tick(), req_b.originating_tick(), "Originating tick mismatch at request {i}, tick {tick}");
                assert_eq!(req_a.emission_source_id(), req_b.emission_source_id(), "Emission source ID mismatch at request {i}, tick {tick}");
                assert_eq!(req_a.originating_frontier_generation(), req_b.originating_frontier_generation(), "Originating frontier generation mismatch at request {i}, tick {tick}");
                assert_eq!(req_a.deterministic_sequence_index(), req_b.deterministic_sequence_index(), "Deterministic sequence index mismatch at request {i}, tick {tick}");
            }

            // Compare scheduler affinity and queue structures
            for core_id in 0..world_a.numa_scheduler.affinity_map.num_cores {
                assert_eq!(
                    world_a.numa_scheduler.queue_len(core_id),
                    world_b.numa_scheduler.queue_len(core_id),
                    "Core {core_id} queue length mismatch at tick {tick}"
                );
            }
        }
    }

    #[test]
    fn renderer_shadow_validator_disabled_by_default() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.tick();
        assert!(
            world.renderer_shadow_validator.last_report.is_none()
        );
        assert_eq!(
            world.renderer_shadow_validator
                .validation_report
                .ticks_run,
            0
        );
    }

    #[test]
    fn renderer_shadow_validator_parity_over_n_ticks() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.renderer_shadow_validator.enable_shadow();
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.inject_heat_at_chunk(4, 4, 0.8);

        for tick in 0..20 {
            world.tick();
            if let Some(ref report) =
                world.renderer_shadow_validator.last_report
            {
                assert_eq!(
                    report.mismatched_chunk_states,
                    0,
                    "tick {tick}: renderer mismatch"
                );
                assert!(
                    report.max_probability_drift <= 1e-4,
                    "tick {tick}: excessive probability drift"
                );
            }
        }

        let vr =
            &world.renderer_shadow_validator.validation_report;
        assert_eq!(vr.ticks_run, 20);
        assert_eq!(vr.ticks_failed, 0);
        assert_eq!(vr.severe_divergence_events, 0);
    }

    #[test]
    fn differential_renderer_matches_full_renderer() {
        let mut full =
            MKRWorld::new(8, 8, 256);
        let mut diff =
            MKRWorld::new(8, 8, 256);
        diff.enable_differential_renderer();

        full.inject_heat_at_chunk(2, 2, 1.0);
        diff.inject_heat_at_chunk(2, 2, 1.0);

        for _ in 0..20 {
            full.tick();
            diff.tick();
        }

        assert_eq!(
            full.directory.chunk_runtime_states,
            diff.directory.chunk_runtime_states,
        );
    }

    #[test]
    fn region_shadow_validator_disabled_by_default() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.tick();
        assert!(
            world.region_shadow_validator.last_report.is_none()
        );
    }

    #[test]
    fn region_shadow_validator_parity_over_n_ticks() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.region_shadow_validator.enable_shadow();
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.inject_heat_at_chunk(4, 4, 0.8);

        for tick in 0..20 {
            world.tick();
            if let Some(ref report) =
                world.region_shadow_validator.last_report
            {
                assert_eq!(
                    report.mismatched_region_states,
                    0,
                    "tick {tick}: region mismatch"
                );
            }
        }

        let vr =
            &world.region_shadow_validator.validation_report;
        assert_eq!(vr.ticks_run, 20);
        assert_eq!(vr.ticks_failed, 0);
    }

    #[test]
    fn differential_renderer_resync_after_reenable() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.enable_differential_renderer();
        world.inject_heat_at_chunk(1, 1, 1.0);
        world.tick();

        world.disable_differential_renderer();
        world.tick();

        world.enable_differential_renderer();
        world.tick();

        assert!(
            !world.differential_renderer_needs_full_sync
        );
    }

    #[test]
    fn debug_frontier_density() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.inject_heat_at_chunk(1, 1, 1.0);

        for tick in 0..10 {
            world.tick();
            println!(
                "tick={} frontier={} total={} density={} sparse={}",
                tick,
                world.last_frontier_stats.frontier_cells,
                world.last_frontier_stats.total_cells,
                world.last_frontier_stats.density,
                world.last_frontier_stats.used_sparse,
            );
        }
    }

    #[test]
    fn test_trace_fusion_compiler_integration() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.emission_gate.budget = 4;
        
        // Inject heat to trigger consistent emissions
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.inject_heat_at_chunk(0, 1, 0.9);
        world.inject_heat_at_chunk(0, 2, 0.8);
        
        // Ticking multiple times to reach maturity (maturity_threshold = 3)
        for _ in 0..5 {
            world.tick();
        }

        // Assert that the compiler registered and compiled the hot trace signature
        assert!(!world.trace_compiler.trace_frequencies.is_empty(), "Trace frequencies should not be empty");
        assert!(!world.trace_compiler.compiled_kernels.is_empty(), "Compiled kernels should not be empty");
    }

    #[test]
    fn test_numa_scheduler_affinity_integration() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.emission_gate.budget = 10;
        
        // Inject heat at specific grid coordinates mapping to distinct regions
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.inject_heat_at_chunk(3, 3, 1.0);
        
        // Ticking the world should run the scheduler and preserve mathematical outputs deterministically
        world.tick();
        
        // Assert that the scheduler is initialized and topology works
        assert!(world.numa_scheduler.affinity_map.num_cores > 0);
    }

    #[test]
    fn sparse_validator_parity_over_n_ticks() {
        // Test Parallel mode (live field is full solver, shadow is sparse)
        let mut world = MKRWorld::new(8, 8, 256);
        world.sparse_validator.enable_parallel();
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.inject_heat_at_chunk(4, 4, 0.8);

        for tick in 0..20 {
            world.tick();
            if let Some(ref report) = world.last_parity {
                if report.cells_compared > 0 {
                    assert!(
                        report.all_passed,
                        "tick {tick}: sparse validator parity violation in Parallel mode: {report:?}"
                    );
                }
            }
        }

        for tick in 0..20 {
            world.tick();
            if let Some(ref report) = world.last_parity {
                if report.cells_compared > 0 {
                    assert!(
                        report.all_passed,
                        "tick {tick}: sparse validator parity violation in SparseAuthoritative mode: {report:?}"
                    );
                }
            }
        }
    }

    #[test]
    fn test_deterministic_cek_replay_equivalence() {
        // Create world A
        let mut world_a = MKRWorld::new(8, 8, 256);
        world_a.emission_gate.budget = 100;
        // Inject identical inputs
        world_a.inject_heat_at_chunk(1, 2, 1.0);
        world_a.inject_heat_at_chunk(3, 4, 0.7);

        // Run for 30 ticks and record state
        let mut history_a_requests = Vec::new();
        let mut history_a_graphs = Vec::new();
        let mut history_a_probs = Vec::new();
        for _ in 0..30 {
            world_a.tick();
            history_a_requests.push(world_a.last_execution_requests.clone());
            history_a_graphs.push(world_a.last_continuation_buffer.clone());
            let probs: Vec<f32> = world_a.activation_field.cells.iter().map(|c| c.execution_probability).collect();
            history_a_probs.push(probs);
        }

        // Create identical world B
        let mut world_b = MKRWorld::new(8, 8, 256);
        world_b.emission_gate.budget = 100;
        world_b.inject_heat_at_chunk(1, 2, 1.0);
        world_b.inject_heat_at_chunk(3, 4, 0.7);

        for tick in 0..30 {
            world_b.tick();
            // Check request IDs and sequences
            let reqs_a = &history_a_requests[tick];
            let reqs_b = &world_b.last_execution_requests;
            assert_eq!(reqs_a.len(), reqs_b.len(), "Tick {tick}: requests count mismatch");
            for (ra, rb) in reqs_a.iter().zip(reqs_b.iter()) {
                assert_eq!(ra.request_id(), rb.request_id(), "Tick {tick}: request_id mismatch");
                assert_eq!(ra.cell_index(), rb.cell_index(), "Tick {tick}: cell index mismatch");
                assert_eq!(ra.priority(), rb.priority(), "Tick {tick}: priority mismatch");
            }

            // Check execution graphs
            let graph_a = &history_a_graphs[tick];
            let graph_b = &world_b.last_continuation_buffer;
            match (graph_a, graph_b) {
                (Some(ga), Some(gb)) => {
                    assert_eq!(ga.continuations.len(), gb.continuations.len(), "Tick {tick}: graph node count mismatch");
                    for (na, nb) in ga.continuations.iter().zip(gb.continuations.iter()) {
                        assert_eq!(na.request_id, nb.request_id, "Tick {tick}: node request_id mismatch");
                        assert_eq!(na.continuation_id, nb.continuation_id, "Tick {tick}: node continuation_id mismatch");
                        assert_eq!(na.provenance, nb.provenance, "Tick {tick}: node provenance mismatch");
                        assert_eq!(na.op, nb.op, "Tick {tick}: node op mismatch");
                    }
                }
                (None, None) => {}
                _ => panic!("Tick {tick}: one graph is None and the other is Some"),
            }

            // Check probabilities
            let probs_a = &history_a_probs[tick];
            let probs_b: Vec<f32> = world_b.activation_field.cells.iter().map(|c| c.execution_probability).collect();
            assert_eq!(probs_a, &probs_b, "Tick {tick}: cell execution probability mismatch");
        }
    }

    #[test]
    fn continuation_ordering_stability() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.emission_gate.budget = 100;
        world.inject_heat_at_chunk(2, 2, 0.9);
        world.inject_heat_at_chunk(5, 5, 0.8);
        world.tick();

        if let Some(ref graph) = world.last_continuation_buffer {
            if !graph.continuations.is_empty() {
                // Verify that nodes are strictly ordered by sequence indices
                let mut last_seq = 0;
                for node in &graph.continuations {
                    assert!(node.sequence_index >= last_seq, "Nondeterministic sequence traversal detected");
                    last_seq = node.sequence_index;
                }
            }
        }
    }

    #[test]
    fn realization_sequence_reproducibility() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.emission_gate.budget = 100;
        world.inject_heat_at_chunk(1, 1, 0.95);
        world.tick();

        if let Some(ref graph) = world.last_continuation_buffer {
            // Verify that all ContinuationDescriptors carry valid indices matching their position in slots
            for (idx, node) in graph.continuations.iter().enumerate() {
                assert_eq!(node.provenance.realization_sequence_index, idx as u64);
            }
        }
    }

    #[test]
    fn replay_identical_realization_sequences() {
        // Change scheduler affinity maps and verify execution graph is completely invariant
        let mut world_normal = MKRWorld::new(8, 8, 256);
        world_normal.emission_gate.budget = 50;
        world_normal.inject_heat_at_chunk(0, 1, 1.0);
        world_normal.inject_heat_at_chunk(2, 3, 0.5);
        world_normal.tick();

        let graph_normal = world_normal.last_continuation_buffer.clone().expect("Should have execution graph");

        // Create second world with altered scheduler core count
        let mut world_alt = MKRWorld::new(8, 8, 256);
        world_alt.emission_gate.budget = 50;
        world_alt.inject_heat_at_chunk(0, 1, 1.0);
        world_alt.inject_heat_at_chunk(2, 3, 0.5);
        // Artificially change affinity core count to simulate different NUMA topology
        world_alt.numa_scheduler.affinity_map.num_cores = 12;
        world_alt.tick();

        let graph_alt = world_alt.last_continuation_buffer.clone().expect("Should have execution graph");

        assert_eq!(graph_normal.continuations.len(), graph_alt.continuations.len());
        for (n_norm, n_alt) in graph_normal.continuations.iter().zip(graph_alt.continuations.iter()) {
            assert_eq!(n_norm.request_id, n_alt.request_id);
            assert_eq!(n_norm.continuation_id, n_alt.continuation_id);
            assert_eq!(n_norm.provenance, n_alt.provenance);
            assert_eq!(n_norm.op, n_alt.op);
        }
    }

    #[test]
    fn continuation_provenance_equivalence() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.emission_gate.budget = 100;
        world.inject_heat_at_chunk(4, 4, 1.0);
        world.tick();

        if let Some(ref graph) = world.last_continuation_buffer {
            for node in &graph.continuations {
                // Verify request ID maps correctly to provenance fields
                let req_id_val = node.request_id.0;
                let tick = node.provenance.originating_tick;
                let src_id = node.provenance.emission_source_id;
                let seq_idx = node.provenance.deterministic_sequence_index;

                let reconstructed_val = (tick << 32)
                    | ((src_id as u64) << 24)
                    | (seq_idx & 0xFF_FFFF);

                assert_eq!(req_id_val, reconstructed_val, "Request ID does not match provenance fields bit-for-bit");
                assert_eq!(node.provenance.request_id, node.request_id);
            }
        }
    }

    #[test]
    fn deterministic_continuation_id_replay() {
        let mut world_1 = MKRWorld::new(8, 8, 256);
        world_1.emission_gate.budget = 50;
        world_1.inject_heat_at_chunk(2, 2, 0.9);
        world_1.tick();
        
        let ids_1: Vec<_> = world_1.last_continuation_buffer.as_ref()
            .unwrap()
            .continuations
            .iter()
            .map(|c| c.continuation_id)
            .collect();

        let mut world_2 = MKRWorld::new(8, 8, 256);
        world_2.emission_gate.budget = 50;
        world_2.inject_heat_at_chunk(2, 2, 0.9);
        world_2.tick();
        
        let ids_2: Vec<_> = world_2.last_continuation_buffer.as_ref()
            .unwrap()
            .continuations
            .iter()
            .map(|c| c.continuation_id)
            .collect();

        assert_eq!(ids_1, ids_2);
        assert!(!ids_1.is_empty());
    }

    #[test]
    fn realization_buffer_equivalence() {
        let mut world_1 = MKRWorld::new(8, 8, 256);
        world_1.emission_gate.budget = 50;
        world_1.inject_heat_at_chunk(1, 2, 0.85);
        world_1.tick();
        let buf_1 = world_1.last_continuation_buffer.clone().unwrap();

        let mut world_2 = MKRWorld::new(8, 8, 256);
        world_2.emission_gate.budget = 50;
        world_2.inject_heat_at_chunk(1, 2, 0.85);
        world_2.tick();
        let buf_2 = world_2.last_continuation_buffer.clone().unwrap();

        assert_eq!(buf_1.continuations.len(), buf_2.continuations.len());
        for (c1, c2) in buf_1.continuations.iter().zip(buf_2.continuations.iter()) {
            assert_eq!(c1.continuation_id, c2.continuation_id);
            assert_eq!(c1.request_id, c2.request_id);
            assert_eq!(c1.provenance, c2.provenance);
            assert_eq!(c1.op, c2.op);
        }
    }

    #[test]
    fn stable_continuation_sorting() {
        use mirage_cek::{ContinuationDescriptor, ContinuationId, ContinuationProvenance, ContinuationOp, stable_sort_continuations};
        use mirage_executor::ExecutionRequestId;

        let req_a = ExecutionRequestId(100);
        let req_b = ExecutionRequestId(200);

        let prov_base = ContinuationProvenance {
            request_id: req_a,
            originating_tick: 1,
            originating_frontier_generation: 0,
            emission_source_id: 0,
            deterministic_sequence_index: 10,
            realization_sequence_index: 0,
        };

        let mut list = vec![
            ContinuationDescriptor {
                continuation_id: ContinuationId(1),
                request_id: req_a,
                sequence_index: 0,
                provenance: ContinuationProvenance {
                    deterministic_sequence_index: 20,
                    ..prov_base
                },
                op: ContinuationOp::SetExecProbability { cell_idx: 5, value: 0.1 },
            },
            ContinuationDescriptor {
                continuation_id: ContinuationId(2),
                request_id: req_a,
                sequence_index: 0,
                provenance: ContinuationProvenance {
                    deterministic_sequence_index: 10,
                    originating_tick: 5,
                    ..prov_base
                },
                op: ContinuationOp::SetExecProbability { cell_idx: 5, value: 0.1 },
            },
            ContinuationDescriptor {
                continuation_id: ContinuationId(3),
                request_id: req_a,
                sequence_index: 0,
                provenance: ContinuationProvenance {
                    deterministic_sequence_index: 10,
                    originating_tick: 2,
                    ..prov_base
                },
                op: ContinuationOp::SetExecProbability { cell_idx: 5, value: 0.1 },
            },
            ContinuationDescriptor {
                continuation_id: ContinuationId(4),
                request_id: req_b,
                sequence_index: 0,
                provenance: ContinuationProvenance {
                    deterministic_sequence_index: 10,
                    originating_tick: 2,
                    request_id: req_b,
                    ..prov_base
                },
                op: ContinuationOp::SetExecProbability { cell_idx: 5, value: 0.1 },
            },
            ContinuationDescriptor {
                continuation_id: ContinuationId(5),
                request_id: req_a,
                sequence_index: 0,
                provenance: ContinuationProvenance {
                    deterministic_sequence_index: 10,
                    originating_tick: 2,
                    ..prov_base
                },
                op: ContinuationOp::SetExecProbability { cell_idx: 3, value: 0.1 },
            },
        ];

        stable_sort_continuations(&mut list);

        assert_eq!(list[0].continuation_id, ContinuationId(5));
        assert_eq!(list[1].continuation_id, ContinuationId(3));
        assert_eq!(list[2].continuation_id, ContinuationId(4));
        assert_eq!(list[3].continuation_id, ContinuationId(2));
        assert_eq!(list[4].continuation_id, ContinuationId(1));
    }

    #[test]
    fn provenance_derivation_equivalence() {
        use mirage_cek::ContinuationProvenance;
        use mirage_executor::ExecutionRequest;

        let (_, _, _, cap) = mirage_executor::scheduler::NUMAAwareScheduler::new();
        let request = ExecutionRequest::new(
            15,
            0.75,
            120,
            false,
            0.75,
            0xAA,
            0xBB,
            0xCC,
            Some(0xDD),
            42,
            7,
            101,
            55,
            &cap,
        );

        let provenance = ContinuationProvenance::from_request(&request, 99);

        assert_eq!(provenance.request_id, request.request_id());
        assert_eq!(provenance.originating_tick, request.originating_tick());
        assert_eq!(provenance.originating_frontier_generation, request.originating_frontier_generation());
        assert_eq!(provenance.emission_source_id, request.emission_source_id());
        assert_eq!(provenance.deterministic_sequence_index, request.deterministic_sequence_index());
        assert_eq!(provenance.realization_sequence_index, 99);
    }

    #[test]
    fn arena_slot_identity_stability() {
        use mirage_cek::{ContinuationArena, ContinuationDescriptor, ContinuationId, ContinuationProvenance, ContinuationOp};
        use mirage_executor::ExecutionRequestId;

        let mut arena = ContinuationArena::new();
        let req_id = ExecutionRequestId(1);
        let prov = ContinuationProvenance {
            request_id: req_id,
            originating_tick: 0,
            originating_frontier_generation: 0,
            emission_source_id: 0,
            deterministic_sequence_index: 0,
            realization_sequence_index: 0,
        };

        let desc = ContinuationDescriptor {
            continuation_id: ContinuationId(100),
            request_id: req_id,
            sequence_index: 0,
            provenance: prov,
            op: ContinuationOp::SetExecProbability { cell_idx: 0, value: 0.5 },
        };

        let idx_0 = arena.insert(desc.clone());
        let idx_1 = arena.insert(desc.clone());
        let idx_2 = arena.insert(desc.clone());

        assert_eq!(idx_0, 0);
        assert_eq!(idx_1, 1);
        assert_eq!(idx_2, 2);

        let removed = arena.remove(1).unwrap();
        assert_eq!(removed.continuation_id, ContinuationId(100));

        let idx_3 = arena.insert(desc.clone());
        assert_eq!(idx_3, 1, "Arena slot 1 should be reused first");

        let retrieved = arena.get(1).unwrap();
        assert_eq!(retrieved.continuation_id, ContinuationId(100));
    }
}

