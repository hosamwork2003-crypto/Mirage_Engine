// ===================================================================
// mirage-mkr-core/src/bridge/execution_bridge.rs
// PURPOSE: ExecutionBridge — EmissionRequest → Executor Protocol
//
// ROLE IN V3:
// The ExecutionBridge translates the MKR emission layer's output
// (EmissionRequest records) into executor-compatible scheduling
// requests, WITHOUT implementing autonomous scheduling, cognition,
// or CEK logic.
//
// This is a PROTOCOL BRIDGE only:
//   MKR emits:  EmissionRequest { cell_index, probability }
//   Executor expects:  ChunkTask { chunk_idx, priority, deadline_frame, state }
//
// The bridge performs the mechanical translation between these two
// type systems so the executor can continue running during the V3
// transition without being modified.
//
// ===================================================================
// AUTHORITY BOUNDARY DECLARATION (V4 Stabilization Pass)
// ===================================================================
//
// AUTHORITY MAP — what this bridge owns:
//   ✓ EmissionRequest → SchedulingRequest translation (1:1 mechanical)
//   ✓ ExecutionRequest protocol type (downstream of SchedulingRequest)
//   ✓ DifferentialExecutionPacket (sparse execution batch descriptor)
//   ✓ FrontierExecutionBatch (frontier-local execution group)
//   ✓ CEK machine lifecycle buffer (deferred_cek_queue)
//
// AUTHORITY MAP — what this bridge MUST NOT own:
//   ✗ Scheduling decisions (executor owns these)
//   ✗ Fiber spawning (executor owns fibers)
//   ✗ Activation field authority (MKR owns the field)
//   ✗ Topology pressure (MKR/MTS own topology)
//   ✗ Execution eligibility decisions (EmissionGate owns these)
//   ✗ Continuation lifecycle (mirage-cek owns Continuation type)
//   ✗ OASIS streaming/residency (OASIS owns residency authority)
//   ✗ Renderer state writes (renderer is passive; bridge only reads field)
//
// EXPECTED AUTHORITY FLOW (V4 stabilized):
//   MKR activation authority
//       ↓
//   EmissionGate (execution eligibility)
//       ↓
//   ExecutionBridge::translate() (mechanical type translation)
//       ↓
//   mirage-compute::FusedKernel (mathematical execution)
//       ↓
//   Executor-compatible scheduling request
//       ↓
//   mirage-executor (fiber execution)
//
// ARCHITECTURAL POSITION:
//   EmissionGate → EmissionRequest
//       ↓
//   ExecutionBridge::translate()
//       ↓
//   SchedulingRequest  [executor-compatible]
//       ↓
//   Caller passes to ThermalScheduler::task_queue OR future fiber emitter
//
// WHAT THIS BRIDGE DOES NOT DO:
//   * Does NOT decide WHICH cells to emit (that is EmissionGate).
//   * Does NOT decide HOW MANY fibers to spawn (future FiberPool).
//   * Does NOT implement CEK continuation selection.
//   * Does NOT own a thread pool or spawn any work.
//   * Does NOT read ChunkState enum arms.
//   * Does NOT write to OASIS residency tables.
//   * Does NOT write to renderer state buffers directly.
//
// TODO(V4-RENDERER-PASSIVE): RendererBridge is the only bridge
// permitted to write ChunkState to RuntimeDirectory. This bridge
// (ExecutionBridge) MUST NOT acquire a mutable reference to
// RuntimeDirectory at any point. Verified: currently clean.
//
// TODO(V4-OASIS-AUTHORITY): OASIS residency decisions are NOT driven
// by this bridge. The `is_prefetch_hint` field on SchedulingRequest
// is a HINT to the streaming coordinator only — OASIS decides whether
// to act on it. This bridge must not call OASIS APIs directly.
//
// PREPARATION FOR CEK:
// When CEK is implemented, the bridge's output type (`SchedulingRequest`)
// will be extended with a `continuation_id` field that CEK populates.
// The rest of the bridge is unchanged.
//
// TODO(V3-CEK-BRIDGE): Add `continuation_id: Option<CekContinuationId>`
//   to SchedulingRequest once CEK defines its continuation type.
// TODO(V3-BRIDGE-FIBER): Replace `priority: f32` with a full
//   `FiberEmissionSlot` once FiberPool is wired to MKRWorld.
// ===================================================================

use crate::emission::{EmissionRequest, MAX_EMIT_PER_TICK};

/// Re-export CEKMachine from mirage-cek for backwards-compatible access.
/// New code should use `mirage_cek::CEKMachine` directly.
pub use mirage_cek::CEKMachine;

// =====================================================================
// RE-EXPORTED PROTOCOL TYPES (V4 Authoritative Pipeline)
// =====================================================================

pub use mirage_executor::{
    SchedulingRequest, ExecutionRequest, DifferentialExecutionPacket,
    FrontierExecutionBatch, ExecutionBridgeAuthority, FrontierExecutionCapability,
};

/// Default number of frames before a scheduling request expires.
pub const DEFAULT_DEADLINE_FRAMES: u64 = 4;

// =====================================================================
// EXECUTION BRIDGE
// =====================================================================

/// Protocol bridge: EmissionRequest → SchedulingRequest.
///
/// # Usage
/// ```rust
/// let bridge = ExecutionBridge::new(16);
/// let requests = bridge.translate(
///     world.emission_requests(),
///     world.frame,
/// );
/// for req in &requests {
///     // Pass to executor, fiber pool, or log for debugging
/// }
/// ```
pub struct ExecutionBridge {
    pub capacity: usize,
    pub deferred_cek_queue: std::cell::RefCell<Vec<CEKMachine>>,
    pub arena: std::cell::RefCell<mirage_cek::ContinuationArena>,
}

impl ExecutionBridge {
    pub fn new(fiber_budget: usize) -> Self {
        Self {
            capacity: fiber_budget,
            deferred_cek_queue: std::cell::RefCell::new(Vec::new()),
            arena: std::cell::RefCell::new(mirage_cek::ContinuationArena::with_capacity(2048)),
        }
    }

    /// Translate a slice of `EmissionRequest`s into `SchedulingRequest`s.
    ///
    /// Each `EmissionRequest` maps 1:1 to a `SchedulingRequest`.
    /// No filtering is applied — the emission gate already handles that.
    /// No scheduling logic is applied — the executor handles that.
    ///
    /// # Priority Mapping
    /// `priority = emission.probability` (identity mapping).
    ///
    /// This is intentional: the activation field already encodes the
    /// correct priority signal.  Remapping would lose information.
    ///
    /// # Deadline
    /// `deadline_frame = current_frame + DEFAULT_DEADLINE_FRAMES`.
    ///
    /// TODO(V3-CEK-BRIDGE): Replace with CEK-computed domain deadlines.
    pub fn translate(
        &self,
        emissions:     &[EmissionRequest],
        current_frame: u64,
    ) -> Vec<SchedulingRequest> {
        emissions
            .iter()
            .map(|e| SchedulingRequest {
                cell_index:      e.cell_index,
                priority:        e.probability,
                deadline_frame:  current_frame + DEFAULT_DEADLINE_FRAMES,
                is_prefetch_hint: e.probability < 0.15,
            })
            .collect()
    }

    /// Translate emissions filtered by region activity.
    ///
    /// **V3-SPARSE / Task 10: Differential scheduling preparation.**
    ///
    /// Only emits `SchedulingRequest`s for cells in regions whose activity
    /// state is `Warming`, `Active`, or `Hot`.  Dormant region cells are
    /// suppressed, even if they appear in the emission list.
    ///
    /// # Rationale
    /// In a differential runtime, dormant regions by definition have
    /// no significant field change.  Any emission requests from dormant
    /// regions are either stale (still_eligible from last activation) or
    /// noise (floating-point jitter above EMIT_GATE).
    ///
    /// # TODO(V3-SPARSE-VALIDATION): Run translate() and translate_region_filtered()
    /// in parallel to confirm that the suppressed set contains only genuinely
    /// low-priority requests.  Validate for 1000 ticks.
    ///
    /// # TODO(V3-CEK-BRIDGE): Add `region_id: u32` to SchedulingRequest so
    /// CEK can select region-local continuations.
    pub fn translate_region_filtered(
        &self,
        emissions:     &[EmissionRequest],
        current_frame: u64,
        region_map:    &crate::regions::RegionMap,
    ) -> Vec<SchedulingRequest> {
        emissions
            .iter()
            .filter(|e| {
                // Only emit for active/hot/warming regions.
                // Dormant regions are skipped — no scheduling overhead.
                !region_map.cell_is_dormant(e.cell_index)
            })
            .map(|e| SchedulingRequest {
                cell_index:      e.cell_index,
                priority:        e.probability,
                deadline_frame:  current_frame + DEFAULT_DEADLINE_FRAMES,
                is_prefetch_hint: e.probability < 0.15,
            })
            .collect()
    }

    /// Translate a single `EmissionRequest` (for per-cell queries).
    #[inline]
    pub fn translate_one(
        &self,
        emission:      EmissionRequest,
        current_frame: u64,
    ) -> SchedulingRequest {
        SchedulingRequest {
            cell_index:      emission.cell_index,
            priority:        emission.probability,
            deadline_frame:  current_frame + DEFAULT_DEADLINE_FRAMES,
            is_prefetch_hint: emission.probability < 0.15,
        }
    }

    /// Filter and sort scheduling requests by priority (descending).
    ///
    /// Returns requests above `min_priority`, sorted highest-first.
    /// This is a convenience method; the executor can also sort itself.
    pub fn priority_filter<'a>(
        &self,
        requests:     &'a mut Vec<SchedulingRequest>,
        min_priority: f32,
    ) -> &'a [SchedulingRequest] {
        requests.retain(|r| r.priority >= min_priority);
        requests.sort_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cell_index.cmp(&b.cell_index))
        });
        requests.as_slice()
    }

    /// Estimate the CPU budget fraction this request batch requires.
    ///
    /// Returns a value in [0.0, 1.0] — 1.0 means the full `MAX_EMIT_PER_TICK`
    /// budget is consumed.  Useful for load-balancing across subsystems.
    pub fn budget_fraction(&self, requests: &[SchedulingRequest]) -> f32 {
        (requests.len() as f32 / MAX_EMIT_PER_TICK as f32).min(1.0)
    }

    /// Dynamically translate raw EmissionRequests into low-overhead captured CEK Machines.
    ///
    /// Closures operate on `&mut dyn mirage_cek::CekEvalField` rather than the
    /// concrete `ActivationField` type. This decouples mirage-cek from
    /// mirage-mkr-core, eliminating the circular dependency.
    ///
    /// # Authority Boundary Note (V4 Stabilization)
    /// This method creates CEK machines and pushes continuations.
    /// This IS within the bridge's authority as the orchestration point
    /// between MKR emission and CEK lifecycle.
    ///
    /// HOWEVER — the CEK machine lifecycle (evaluate_all, eviction) MUST
    /// remain here or in MKRWorld::tick(). Do NOT move CEK lifecycle
    /// management into mirage-compute or mirage-executor.
    ///
    /// TODO(V4-COMPUTE-BOUNDARY): When CEK evaluation produces
    /// ComputeContinuation records, pipe them through TraceFusionCompiler
    /// here before passing to FusedKernel. The kernel receives DATA,
    /// not the CEK machine reference.
    ///
    /// TODO(V4-RENDERER-PASSIVE): bootstrap_cek_context MUST NOT write
    /// to RuntimeDirectory or RendererBridge. CEK evaluation results
    /// flow through ActivationField → RendererBridge passively.
    ///
    /// TODO(V4-OASIS-AUTHORITY): bootstrap_cek_context MUST NOT trigger
    /// OASIS streaming decisions. Streaming hints come from SchedulingRequest
    /// is_prefetch_hint, which OASIS reads independently.
    pub fn bootstrap_cek_context(
        &self,
        emissions: &[ExecutionRequest],
        topo_influence: &[f32],
    ) -> Vec<CEKMachine> {
        let shared_influence: std::sync::Arc<[f32]> = std::sync::Arc::from(topo_influence);
        let mut machines = Vec::with_capacity(emissions.len());
        let mut arena = self.arena.borrow_mut();
        for req in emissions {
            let cell_idx = req.cell_index();
            let prob_signal = req.priority();
            let mut machine = CEKMachine::new(cell_idx, shared_influence.clone(), prob_signal, req.request_id());

            let prov = mirage_cek::ContinuationProvenance::from_request(req, 0);

            let node = mirage_cek::ContinuationDescriptor {
                continuation_id: mirage_cek::ContinuationId(req.request_id().0),
                request_id: req.request_id(),
                sequence_index: req.deterministic_sequence_index(),
                provenance: prov,
                op: mirage_cek::ContinuationOp::AdjustExecProbability {
                    cell_idx,
                    delta: prob_signal,
                },
            };
            let idx = arena.insert(node);
            machine.push_kontinuation(idx);

            machines.push(machine);
        }
        machines
    }

    /// Evict machines from the deferred queue if their underlying cells fade below a certain noise floor.
    /// This protects the heap from persistent stale memory leaking over un-invoked cells.
    pub fn evict_quiescent_cek_states(&self, field: &crate::activation::field::ActivationField) {
        let mut queue = self.deferred_cek_queue.borrow_mut();
        let mut arena = self.arena.borrow_mut();
        const QUIESCENT_FLOOR: f32 = 1e-4;
        queue.retain(|machine| {
            let keep = if machine.control_cell < field.cells.len() {
                let prob = field.cells[machine.control_cell].execution_probability;
                prob >= QUIESCENT_FLOOR
            } else {
                false
            };
            if !keep {
                for &idx in &machine.kontinuation_stack {
                    arena.remove(idx);
                }
            }
            keep
        });
    }

    /// Statefully collect newly generated context fields and merge them directly with our deferred lifecycle backlog
    pub fn process_and_queue_cek_context(
        &self,
        emissions: &[ExecutionRequest],
        topo_influence: &[f32],
    ) {
        let mut new_machines = self.bootstrap_cek_context(emissions, topo_influence);
        let mut queue = self.deferred_cek_queue.borrow_mut();
        queue.append(&mut new_machines);
    }
}

impl Default for ExecutionBridge {
    fn default() -> Self {
        Self::new(128)
    }
}

// =====================================================================
// CEK MACHINE SUBSTRATE
// =====================================================================
//
// CEKMachine is defined in `mirage-cek` and re-exported above.
// The continuation closures use `&mut dyn mirage_cek::CekEvalField`
// instead of the concrete `ActivationField`, breaking the circular
// dependency between mirage-cek and mirage-mkr-core.
//
// `ActivationField` implements `CekEvalField` in
// `crates/mirage-mkr-core/src/activation/field.rs`.
//
// evaluate_all() in lib.rs tick() now calls:
//   machine.evaluate_all(&mut self.activation_field as &mut dyn CekEvalField)


// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emission::EmissionRequest;

    fn make_emission(cell: usize, prob: f32) -> EmissionRequest {
        EmissionRequest { cell_index: cell, probability: prob }
    }

    #[test]
    fn translate_maps_probability_to_priority() {
        let bridge = ExecutionBridge::new(16);
        let emissions = vec![make_emission(0, 0.8), make_emission(1, 0.3)];
        let requests = bridge.translate(&emissions, 100);
        assert_eq!(requests.len(), 2);
        assert!((requests[0].priority - 0.8).abs() < 1e-6);
        assert!((requests[1].priority - 0.3).abs() < 1e-6);
    }

    #[test]
    fn translate_sets_correct_deadline() {
        let bridge = ExecutionBridge::new(16);
        let emissions = vec![make_emission(5, 0.5)];
        let requests = bridge.translate(&emissions, 200);
        assert_eq!(requests[0].deadline_frame, 200 + DEFAULT_DEADLINE_FRAMES);
    }

    #[test]
    fn low_probability_marked_as_prefetch_hint() {
        let bridge = ExecutionBridge::new(16);
        let emissions = vec![
            make_emission(0, 0.05),  // below 0.15 threshold
            make_emission(1, 0.80),  // above 0.15 threshold
        ];
        let requests = bridge.translate(&emissions, 0);
        assert!(requests[0].is_prefetch_hint, "low prob should be prefetch hint");
        assert!(!requests[1].is_prefetch_hint, "high prob should not be prefetch hint");
    }

    #[test]
    fn empty_emissions_produce_empty_requests() {
        let bridge = ExecutionBridge::new(16);
        let requests = bridge.translate(&[], 0);
        assert!(requests.is_empty());
    }

    #[test]
    fn budget_fraction_bounded() {
        let bridge = ExecutionBridge::new(16);
        let emissions: Vec<EmissionRequest> = (0..200)
            .map(|i| make_emission(i, 0.5))
            .collect();
        let requests = bridge.translate(&emissions, 0);
        let fraction = bridge.budget_fraction(&requests);
        assert!(fraction >= 0.0 && fraction <= 1.0,
            "budget fraction out of range: {}", fraction);
    }

    #[test]
    fn test_cek_machine_bootstrap_and_stack_drain() {
        use crate::activation::field::ActivationField;
        
        let bridge = ExecutionBridge::new(16);
        let (_, _, _, cap) = mirage_executor::scheduler::NUMAAwareScheduler::new();
        let emissions = vec![
            ExecutionRequest::new(2, 0.85, 100, false, 0.85, 0, 0, 0, None, 0, 0, 0, 0, &cap),
        ];
        let topo = vec![0.0; 64];
        let mut field = ActivationField::new(8, 8);
        let mut machines = bridge.bootstrap_cek_context(&emissions, &topo);
        
        assert_eq!(machines.len(), 1);
        assert_eq!(machines[0].control_cell, 2);
        assert_eq!(machines[0].kontinuation_stack.len(), 1);
        
        // Authoritatively evaluate the stack context onto our live test field
        machines[0].evaluate_all(&mut field, &bridge.arena.borrow());
        assert_eq!(machines[0].kontinuation_stack.len(), 0);
        assert!(field.cells[2].execution_probability > 0.0, "CEK frame must mutably alter field states");
    }

    #[test]
    fn test_continuation_lifecycle_multi_frame_persistence() {
        use crate::activation::field::ActivationField;

        // Initialize with a strict budget limitation of exactly 1 context swap per frame
        let bridge = ExecutionBridge::new(1);
        let mut field = ActivationField::new(8, 8);
        field.cells[10].execution_probability = 0.9;
        field.cells[20].execution_probability = 0.8;
        field.cells[30].execution_probability = 0.7;

        let (_, _, _, cap) = mirage_executor::scheduler::NUMAAwareScheduler::new();
        // Flood the gateway with 3 heavy cellular emissions to purposely blast past bounds
        let emissions = vec![
            ExecutionRequest::new(10, 0.9, 100, false, 0.9, 0, 0, 0, None, 0, 0, 0, 0, &cap),
            ExecutionRequest::new(20, 0.8, 100, false, 0.8, 0, 0, 0, None, 0, 0, 0, 1, &cap),
            ExecutionRequest::new(30, 0.7, 100, false, 0.7, 0, 0, 0, None, 0, 0, 0, 2, &cap),
        ];
        let topo = vec![0.0; 64];

        // Frame 1: Queue and evaluate under strict budget restrictions
        bridge.process_and_queue_cek_context(&emissions, &topo);
        {
            let mut queue = bridge.deferred_cek_queue.borrow_mut();
            assert_eq!(queue.len(), 3, "All 3 contexts must be successfully statefully initialized");
            
            let mut unexecuted = Vec::new();
            let mut count = 0;
            for mut m in queue.drain(..) {
                if count < 1 {
                    m.evaluate_all(&mut field, &bridge.arena.borrow());
                    count += 1;
                } else {
                    unexecuted.push(m);
                }
            }
            *queue = unexecuted;
        }

        // Verify exactly 2 overflow context modules got cleanly preserved
        assert_eq!(bridge.deferred_cek_queue.borrow().len(), 2, "Overflow contexts must statefully persist");
        
        // Clear out inactive elements via the noise floor manager to verify filter stability
        field.cells[20].execution_probability = 0.0; // Force cell 20 into a dead state
        bridge.evict_quiescent_cek_states(&field);
        assert_eq!(bridge.deferred_cek_queue.borrow().len(), 1, "Dead context should be evicted cleanly");
    }
}

