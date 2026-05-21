// ===================================================================
// mirage-executor/src/fiber.rs
// PURPOSE: Lightweight Fiber Execution Container
//
// ===================================================================
// AUTHORITY BOUNDARY DECLARATION (V4 Stabilization Pass — Task 8)
// ===================================================================
//
// EXECUTOR AUTHORITY:
//   ✓ Task execution
//   ✓ Fiber execution (run the closure, nothing more)
//   ✓ Scheduling implementation (when told by MKR)
//
// EXECUTOR MUST NOT BECOME:
//   ✗ Activation authority (MKR owns the activation field)
//   ✗ Topology authority (MKR/MTS own topology)
//   ✗ Continuation semantics layer (mirage-cek owns this)
//   ✗ Orchestration layer (MKRWorld is the orchestrator)
//   ✗ Cognition layer (forbidden in this phase entirely)
//
// FIBER AUTHORITY CONSTRAINTS:
//   * Fiber is an EXECUTION CONTAINER only.
//   * Fiber::resume() runs the stored closure — that is all.
//   * Fiber::suspend() is a no-op (cooperative yield point).
//   * FiberPool is a pre-allocated execution slot array.
//   * Neither Fiber nor FiberPool may evolve into:
//     - continuation semantic ownership
//     - activation authority
//     - topology scheduling
//     - predictive orchestration
//     - autonomous runtime governance
//
// BUDGET FIELD NOTE:
//   `Fiber::budget` is COMPATIBILITY-ONLY infrastructure.
//   It exists to support the legacy V3 fiber budget model.
//   It MUST NOT be transformed into:
//     - adaptive scheduler authority
//     - thermal priority authority
//     - topology execution authority
//     - cognition heuristics
//   Future execution policy derives from MKR activation authority,
//   NOT from executor-local heuristics stored in `budget`.
//
// TODO(V4-EXECUTOR-PASSIVE): Remove `budget` field from Fiber once
// all callers use SchedulingRequest::priority as the sole budget input.
// The executor must not re-derive priority from local state.
//
// TODO(V4-EXECUTOR-PASSIVE): ContinuationFn is a raw `Box<dyn FnMut()>`.
// This is NOT a CEK continuation. It is an executor-level callable unit.
// When CEK is wired, CEK evaluation (via CekEvalField) happens INSIDE the
// closure body. The fiber itself remains semantics-agnostic.
//
// TODO(V4-EXECUTOR-PASSIVE): FiberPool::spawn() currently overwrites
// slots via index mod poolsize — eviction is silent. Replace with an
// explicit eviction protocol that reports overflow to MKR, so that
// MKR can adjust emission rates to avoid fiber starvation.
// ===================================================================

use std::sync::atomic::{AtomicUsize, Ordering};

pub type ContinuationFn = Box<dyn FnMut() + Send>;

/// Lightweight fiber container — cooperative resumable execution unit.
///
/// # Authority
/// EXECUTION CONTAINER only. Fibers execute closures; they do not own
/// activation state, topology, or continuation semantics.
///
/// # Budget Field
/// `budget` is compatibility infrastructure. It MUST NOT be used to
/// derive scheduling policy. MKR activation authority drives scheduling.
pub struct Fiber {
    pub id: usize,
    pub continuation: Option<ContinuationFn>,
    /// Compatibility-only execution budget.
    ///
    /// TODO(V4-EXECUTOR-PASSIVE): Deprecated. Remove once all callers
    /// use SchedulingRequest::priority as the scheduling input.
    /// Do NOT use this field for adaptive scheduling decisions.
    pub budget: u32,
}

impl Fiber {
    pub fn new(id: usize, cont: ContinuationFn) -> Self {
        Self { id, continuation: Some(cont), budget: 100 }
    }

    /// Execute the stored closure once.
    ///
    /// # Authority
    /// Pure execution. No scheduling, no activation field writes,
    /// no continuation lifecycle management.
    pub fn resume(&mut self) {
        if let Some(f) = &mut self.continuation {
            (f)();
        }
    }

    /// Cooperative suspend — preserves the continuation for later resumption.
    ///
    /// Currently a no-op. Future: may record yield metadata for MKR audit.
    ///
    /// TODO(V4-EXECUTOR-PASSIVE): When fibers become multi-frame resumable,
    /// this method should record the suspension reason (budget exhausted,
    /// field quiescent, etc.) for MKR-side eviction policy decisions.
    /// The suspension reason MUST come from MKR, not from executor heuristics.
    pub fn suspend(&mut self) {
        // Cooperative suspend — continuation preserved
    }
}

/// Fixed-capacity fiber pool — pre-allocated execution slot array.
///
/// # Authority
/// EXECUTION CONTAINER POOL only. No activation authority, no scheduling
/// intelligence, no continuation semantics.
///
/// # Allocation Strategy
/// Pre-allocated at construction time. No heap churn on spawn in hot-path.
/// Silent eviction via slot index modulo — see TODO above.
pub struct FiberPool {
    pool: Vec<Option<Fiber>>,
    next_id: AtomicUsize,
}

impl FiberPool {
    pub fn with_capacity(cap: usize) -> Self {
let mut pool = Vec::with_capacity(cap);

for _ in 0..cap {
    pool.push(None);
}

Self {
    pool,
    next_id: AtomicUsize::new(0),
}
    }

    pub fn spawn(&mut self, cont: ContinuationFn) -> usize {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let idx = id % self.pool.len();
        self.pool[idx] = Some(Fiber::new(id, cont));
        id
    }

    pub fn resume(&mut self, id: usize) {
        let idx = id % self.pool.len();
        if let Some(f) = &mut self.pool[idx] {
            f.resume();
        }
    }
}
