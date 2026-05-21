/// ===================================================================
/// mirage-executor/src/lib.rs  (V4 — Stabilization Pass)
/// PURPOSE: Mirage Executor — Passive Execution Backend
///
/// ---------------------------------------------------------------
/// V3-EXECUTOR-PASSIVE: ROLE BOUNDARY
/// ---------------------------------------------------------------
///
/// The executor is a PASSIVE EXECUTION BACKEND.
/// It receives execution-compatible work descriptors and runs them.
/// It does NOT:
///   * decide execution eligibility (that is MKR/ActivationField)
///   * compute activation or topology pressure (that is MKRWorld)
///   * own scheduling authority (that is EmissionGate)
///   * own thermal state (ThermalSystem is a COMPAT MIRROR only)
///
/// V3 COMPATIBILITY NOTICE:
/// ThermalScheduler is COMPATIBILITY INFRASTRUCTURE in V3.
/// It reads discrete ChunkState enum arms (Hot/Resident/Predictive/Dormant)
/// which is the opposite of the V3 continuous activation field model.
///
/// TODO(V3-EXECUTOR-PASSIVE): Replace ThermalScheduler::schedule_frame()
/// with a field-driven fiber emitter:
///   fn schedule_from_emission(requests: &[SchedulingRequest])
/// where SchedulingRequest comes from mirage-mkr-core::bridge::ExecutionBridge.
/// The executor accepts pre-computed priorities; it does not re-derive them.
///
/// TODO(V3-EXECUTOR-PASSIVE): Remove ThermalSystem ownership from
/// ThermalScheduler.  The executor must not own a thermal model —
/// thermal truth comes from MKRWorld and is mirrored via compat shim.
///
/// TODO(V3-EXECUTOR-PASSIVE): ChunkTask::state: ChunkState must be
/// replaced with ChunkTask::priority: f32 as the SOLE scheduling input.
/// The executor must not branch on discrete enum arms for new code.
///
/// TODO(V3-REMOVE-THERMAL-1): Once all callers use SchedulingRequest,
/// delete ThermalScheduler::schedule_frame() and the ThermalSystem
/// ownership inside this struct.
///
/// SCHEDULING PRIORITY (COMPAT-ONLY):
/// HOT > RESIDENT > PREDICTIVE > DORMANT
///
/// ---------------------------------------------------------------
/// V4 AUTHORITY BOUNDARY ADDITIONS (Stabilization Pass)
/// ---------------------------------------------------------------
///
/// EXECUTOR IS FULLY PASSIVE:
/// In V4, ThermalScheduler has been made fully passive:
///   * Removed SynapseRegistry from ThermalScheduler.
///   * Removed ThermalSystem from ThermalScheduler.
///   * Deprecated schedule_frame().
///   * Replaced scheduling with ExecutionRequest-driven schedule_requests().
///
/// TODO(V4-OASIS-AUTHORITY): The executor must not make OASIS
/// residency decisions. Streaming hints come from ExecutionRequest
/// is_prefetch_hint, which the streaming coordinator reads.
/// The executor must not call OASIS APIs directly.
///
/// TODO(V4-RENDERER-PASSIVE): The executor must not write
/// ChunkState to RuntimeDirectory. Only RendererBridge is authorized
/// to do this. Verify that execute_task() and schedule_frame()
/// do not acquire a mutable RuntimeDirectory reference.
/// Verified: currently clean. Maintain this invariant.
/// ===================================================================

// Re-export thermal types
pub use mirage_core::runtime::{ChunkState, ThermalSystem, ChunkThermals};

pub mod fiber;
pub mod scheduler;

// =====================================================================
// AUTHORITY AND CAPABILITY TOKENS
// =====================================================================

/// Authority token enforcing that only authorized bridge/scheduler owner components
/// can create authoritative execution requests.
#[derive(Debug)]
pub struct ExecutionBridgeAuthority {
    _private: (),
}

impl ExecutionBridgeAuthority {
    pub(crate) fn new() -> Self {
        Self { _private: () }
    }
}

/// Compile-time capability token authorizing a component to traverse
/// the propagation frontier and emit execution requests.
#[derive(Debug)]
pub struct FrontierExecutionCapability {
    _private: (),
}

impl FrontierExecutionCapability {
    pub(crate) fn new() -> Self {
        Self { _private: () }
    }
}

/// Compile-time capability token authorizing a component to submit
/// execution requests/tasks to the scheduler.
#[derive(Debug)]
pub struct SchedulerCapability {
    _private: (),
}

impl SchedulerCapability {
    pub(crate) fn new() -> Self {
        Self { _private: () }
    }
}

// =====================================================================
// PROTOCOL TYPES (V4 Authoritative Pipeline)
// =====================================================================

/// Compatibility scheduling request, transitional infrastructure only in V4.
#[derive(Debug, Clone, Copy)]
pub struct SchedulingRequest {
    pub cell_index: usize,
    pub priority: f32,
    pub deadline_frame: u64,
    pub is_prefetch_hint: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ExecutionRequestId(pub u64);

impl std::fmt::Display for ExecutionRequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ReqID({})", self.0)
    }
}

/// Authoritative V4 execution request.
#[derive(Debug, Clone, Copy)]
pub struct ExecutionRequest {
    cell_index: usize,
    priority: f32,
    deadline_frame: u64,
    is_prefetch_hint: bool,
    captured_probability: f32,
    capability_mask: u32,
    target_identity: u32,
    intent_flags: u32,
    advisory_hints: Option<u32>,
    originating_tick: u64,
    emission_source_id: u32,
    originating_frontier_generation: u64,
    deterministic_sequence_index: u64,
    request_id: ExecutionRequestId,
}

impl ExecutionRequest {
    /// Restricted constructor requiring the FrontierExecutionCapability token.
    #[inline]
    pub fn new(
        cell_index: usize,
        priority: f32,
        deadline_frame: u64,
        is_prefetch_hint: bool,
        captured_probability: f32,
        capability_mask: u32,
        target_identity: u32,
        intent_flags: u32,
        advisory_hints: Option<u32>,
        originating_tick: u64,
        emission_source_id: u32,
        originating_frontier_generation: u64,
        deterministic_sequence_index: u64,
        _cap: &FrontierExecutionCapability,
    ) -> Self {
        let request_id = ExecutionRequestId(
            (originating_tick << 32)
                | ((emission_source_id as u64) << 24)
                | (deterministic_sequence_index & 0xFF_FFFF),
        );
        Self {
            cell_index,
            priority,
            deadline_frame,
            is_prefetch_hint,
            captured_probability,
            capability_mask,
            target_identity,
            intent_flags,
            advisory_hints,
            originating_tick,
            emission_source_id,
            originating_frontier_generation,
            deterministic_sequence_index,
            request_id,
        }
    }

    /// Construct from a SchedulingRequest. Restricted to authorized components.
    #[inline]
    pub fn from_scheduling(
        req: SchedulingRequest,
        captured_prob: f32,
        capability_mask: u32,
        target_identity: u32,
        intent_flags: u32,
        advisory_hints: Option<u32>,
        originating_tick: u64,
        emission_source_id: u32,
        originating_frontier_generation: u64,
        deterministic_sequence_index: u64,
        _cap: &FrontierExecutionCapability,
    ) -> Self {
        let request_id = ExecutionRequestId(
            (originating_tick << 32)
                | ((emission_source_id as u64) << 24)
                | (deterministic_sequence_index & 0xFF_FFFF),
        );
        Self {
            cell_index: req.cell_index,
            priority: req.priority,
            deadline_frame: req.deadline_frame,
            is_prefetch_hint: req.is_prefetch_hint,
            captured_probability: captured_prob,
            capability_mask,
            target_identity,
            intent_flags,
            advisory_hints,
            originating_tick,
            emission_source_id,
            originating_frontier_generation,
            deterministic_sequence_index,
            request_id,
        }
    }

    #[inline] pub fn cell_index(&self) -> usize { self.cell_index }
    #[inline] pub fn priority(&self) -> f32 { self.priority }
    #[inline] pub fn deadline_frame(&self) -> u64 { self.deadline_frame }
    #[inline] pub fn is_prefetch_hint(&self) -> bool { self.is_prefetch_hint }
    #[inline] pub fn captured_probability(&self) -> f32 { self.captured_probability }
    #[inline] pub fn capability_mask(&self) -> u32 { self.capability_mask }
    #[inline] pub fn target_identity(&self) -> u32 { self.target_identity }
    #[inline] pub fn intent_flags(&self) -> u32 { self.intent_flags }
    #[inline] pub fn advisory_hints(&self) -> Option<u32> { self.advisory_hints }
    #[inline] pub fn originating_tick(&self) -> u64 { self.originating_tick }
    #[inline] pub fn emission_source_id(&self) -> u32 { self.emission_source_id }
    #[inline] pub fn originating_frontier_generation(&self) -> u64 { self.originating_frontier_generation }
    #[inline] pub fn deterministic_sequence_index(&self) -> u64 { self.deterministic_sequence_index }
    #[inline] pub fn request_id(&self) -> ExecutionRequestId { self.request_id }

    #[inline]
    pub fn is_expired(&self, current_frame: u64) -> bool {
        current_frame > self.deadline_frame
    }

    #[inline]
    pub fn is_stale(&self, current_probability: f32, staleness_threshold: f32) -> bool {
        self.captured_probability - current_probability > staleness_threshold
    }
}

/// A sparse differential execution packet.
#[derive(Debug, Clone)]
pub struct DifferentialExecutionPacket {
    requests: Vec<ExecutionRequest>,
    tick: u64,
    frontier_density: usize,
}

impl DifferentialExecutionPacket {
    pub fn new(tick: u64, _auth: &ExecutionBridgeAuthority) -> Self {
        Self {
            requests: Vec::new(),
            tick,
            frontier_density: 0,
        }
    }

    #[inline]
    pub fn push(&mut self, req: ExecutionRequest, _auth: &ExecutionBridgeAuthority) {
        self.requests.push(req);
        self.frontier_density = self.requests.len();
    }

    #[inline] pub fn requests(&self) -> &[ExecutionRequest] { &self.requests }
    #[inline] pub fn tick(&self) -> u64 { self.tick }
    #[inline] pub fn frontier_density(&self) -> usize { self.frontier_density }
    #[inline] pub fn is_empty(&self) -> bool { self.requests.is_empty() }
    #[inline] pub fn len(&self) -> usize { self.requests.len() }
}

/// A frontier-local execution batch.
#[derive(Debug, Clone)]
pub struct FrontierExecutionBatch {
    region_id: u32,
    requests: Vec<ExecutionRequest>,
    affinity_hint: usize,
}

impl FrontierExecutionBatch {
    pub fn new(region_id: u32, affinity_hint: usize, _auth: &ExecutionBridgeAuthority) -> Self {
        Self {
            region_id,
            requests: Vec::new(),
            affinity_hint,
        }
    }

    #[inline]
    pub fn push(&mut self, req: ExecutionRequest, _auth: &ExecutionBridgeAuthority) {
        self.requests.push(req);
    }

    #[inline] pub fn region_id(&self) -> u32 { self.region_id }
    #[inline] pub fn requests(&self) -> &[ExecutionRequest] { &self.requests }
    #[inline] pub fn affinity_hint(&self) -> usize { self.affinity_hint }
    #[inline] pub fn is_empty(&self) -> bool { self.requests.is_empty() }
    #[inline] pub fn len(&self) -> usize { self.requests.len() }
}
