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

// =====================================================================
// SCHEDULING REQUEST — Executor-compatible work descriptor
// =====================================================================

/// Executor-compatible scheduling request produced by `ExecutionBridge`.
///
/// This type is intentionally kept minimal.  It carries only what the
/// executor needs to prioritise and execute work.  CEK will extend it
/// with a continuation identifier when CEK is implemented.
///
/// # V3 Design
/// `priority` is derived directly from `execution_probability` — no
/// enum-arm translation, no threshold branching.  The executor receives
/// a continuous priority weight that it can use as-is or discretise
/// internally.
#[derive(Debug, Clone, Copy)]
pub struct SchedulingRequest {
    /// Flat field cell index — the chunk this request is for.
    pub cell_index: usize,

    /// Continuous execution priority in [0.0, 1.0].
    ///
    /// Derived directly from `EmissionRequest::probability`.
    /// Higher = execute sooner / allocate more budget.
    pub priority: f32,

    /// Frame by which this request expires (if not executed).
    ///
    /// Currently set to `current_frame + DEFAULT_DEADLINE_FRAMES`.
    /// Future: CEK will compute domain-specific deadlines.
    pub deadline_frame: u64,

    /// Whether this request represents a prefetch hint (vs. execution demand).
    ///
    /// Prefetch hints are used to trigger streaming without spawning fibers.
    /// TODO(V3-STREAM): Wire this to StreamingCoordinator decisions.
    pub is_prefetch_hint: bool,
}

/// Default number of frames before a scheduling request expires.
pub const DEFAULT_DEADLINE_FRAMES: u64 = 4;

// =====================================================================
// EXECUTION BRIDGE
// =====================================================================

/// Protocol bridge: EmissionRequest → SchedulingRequest.
///
/// Stateless — all context is passed through method arguments.
///
/// # Usage
/// ```rust
/// let bridge = ExecutionBridge::new();
/// let requests = bridge.translate(
///     world.emission_requests(),
///     world.frame,
/// );
/// for req in &requests {
///     // Pass to executor, fiber pool, or log for debugging
/// }
/// ```
pub struct ExecutionBridge;

impl ExecutionBridge {
    pub fn new() -> Self { Self }

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
        requests.sort_unstable_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
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
}

impl Default for ExecutionBridge {
    fn default() -> Self { Self::new() }
}

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
        let bridge = ExecutionBridge::new();
        let emissions = vec![make_emission(0, 0.8), make_emission(1, 0.3)];
        let requests = bridge.translate(&emissions, 100);
        assert_eq!(requests.len(), 2);
        assert!((requests[0].priority - 0.8).abs() < 1e-6);
        assert!((requests[1].priority - 0.3).abs() < 1e-6);
    }

    #[test]
    fn translate_sets_correct_deadline() {
        let bridge = ExecutionBridge::new();
        let emissions = vec![make_emission(5, 0.5)];
        let requests = bridge.translate(&emissions, 200);
        assert_eq!(requests[0].deadline_frame, 200 + DEFAULT_DEADLINE_FRAMES);
    }

    #[test]
    fn low_probability_marked_as_prefetch_hint() {
        let bridge = ExecutionBridge::new();
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
        let bridge = ExecutionBridge::new();
        let requests = bridge.translate(&[], 0);
        assert!(requests.is_empty());
    }

    #[test]
    fn budget_fraction_bounded() {
        let bridge = ExecutionBridge::new();
        let emissions: Vec<EmissionRequest> = (0..200)
            .map(|i| make_emission(i, 0.5))
            .collect();
        let requests = bridge.translate(&emissions, 0);
        let fraction = bridge.budget_fraction(&requests);
        assert!(fraction >= 0.0 && fraction <= 1.0,
            "budget fraction out of range: {}", fraction);
    }
}
