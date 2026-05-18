// ===================================================================
// mirage-mkr-core/src/streaming/mod.rs  (V3 — Federated Stabilization Pass)
// PURPOSE: StreamingCoordinator — Activation-Driven Streaming Gate
//
// ---------------------------------------------------------------
// STREAMING OWNERSHIP BOUNDARY (CANONICAL)
// ---------------------------------------------------------------
//
// MKR (this module) is ONLY responsible for:
//   * Computing streaming ELIGIBILITY from execution_probability
//   * Classifying cells into Prefetch / PromoteResident actions
//   * Returning a bounded StreamingDecision slice to the caller
//   * Providing heat values for stream-completion feedback
//
// TODO(V3-OASIS-CANONICAL): The caller is responsible for forwarding
// StreamingDecisions to mirage-memory-oasis::StreamingFabric, which
// is the ONLY authorised executor of stream lifecycles.  MKR must
// NEVER call StreamingFabric::prefetch_horizon() directly from inside
// this module — that would make MKR a streaming owner, violating the
// federated architecture contract.
//
// ---------------------------------------------------------------
// OASIS OWNS:
// ---------------------------------------------------------------
//   * prefetch_horizon() execution
//   * residency promotion lifecycle
//   * mmap page mapping
//   * stream completion signals
//   * loaded/queued/evicted state
//
// MKR OWNS (eligibility only):
//   * probability thresholds (STREAM_PREFETCH_THRESHOLD, STREAM_RESIDENT_THRESHOLD)
//   * StreamingDecision generation (scan output)
//   * STREAM_COMPLETION_HEAT (feedback amount only)
//
// TODO(V3-OASIS-CANONICAL): StreamingFabric's prefetch_horizon() is
// currently camera-position-driven.  A future pass will add a
// field-index-based request path so MKR StreamingDecisions can drive
// OASIS directly without camera coordinate conversion.
//
// ---------------------------------------------------------------
// WHAT THIS IS NOT
// ---------------------------------------------------------------
//   * NOT a replacement for mirage-memory-oasis (OASIS is canonical).
//   * NOT a job queue — it makes boolean decisions, not work items.
//   * NOT camera-aware — camera velocity is an upstream concern.
//   * NOT the residency authority — that is OASIS/ResidencyTracker.
//
// TODO(V3-CEK): Once CEK is implemented, streaming requests will be
// generated from CEK emission events rather than direct probability
// threshold scans.  The StreamingCoordinator will become a CEK
// plugin rather than a direct ActivationField reader.
// ===================================================================

use crate::activation::field::ActivationField;

// =====================================================================
// CONSTANTS
// =====================================================================

/// execution_probability threshold to trigger predictive streaming (prefetch).
///
/// Cells above this value signal that streaming should begin.
/// Lower than EMIT_GATE (0.05) to ensure streaming begins before execution.
pub const STREAM_PREFETCH_THRESHOLD: f32 = 0.03;

/// execution_probability threshold to trigger hot-path residency promotion.
///
/// Cells above this value should be in VRAM and actively simulated.
/// Matches BRIDGE_RESIDENT_THRESHOLD in renderer_bridge.
pub const STREAM_RESIDENT_THRESHOLD: f32 = 0.35;

/// Heat injection amount when a streaming operation completes.
///
/// Completing a stream raises the cell's heat signal, which in turn
/// raises activation and execution_probability on the next tick.
/// This closes the streaming ↔ field feedback loop.
pub const STREAM_COMPLETION_HEAT: f32 = 0.25;

/// Maximum number of stream requests to generate per tick.
///
/// Prevents the streaming layer from being flooded when many cells
/// simultaneously cross the prefetch threshold.
pub const MAX_STREAM_REQUESTS_PER_TICK: usize = 32;

// =====================================================================
// STREAMING DECISION
// =====================================================================

/// A streaming decision record produced by `StreamingCoordinator::scan()`.
///
/// Tells the caller exactly which field cells need streaming action and
/// what kind of action is required.
#[derive(Debug, Clone, Copy)]
pub struct StreamingDecision {
    /// Flat field cell index (== chunk index for 1:1 grids).
    pub cell_index: usize,
    /// Action required for this cell.
    pub action: StreamAction,
    /// Raw probability at scan time (caller can use for priority sorting).
    pub probability: f32,
}

/// Streaming action type derived from execution_probability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamAction {
    /// Begin async prefetch — probability is above STREAM_PREFETCH_THRESHOLD
    /// but below STREAM_RESIDENT_THRESHOLD.
    Prefetch,
    /// Promote to resident VRAM — probability is at or above STREAM_RESIDENT_THRESHOLD.
    PromoteResident,
}

// =====================================================================
// STREAMING COORDINATOR
// =====================================================================

/// Activation-driven streaming gate.
///
/// Scans `execution_probability` each tick and produces a bounded list
/// of `StreamingDecision`s for cells that need OASIS streaming action.
///
/// # Ownership
/// `StreamingCoordinator` is stateless — all data comes from the field
/// reference passed to `scan()`.  It owns only a pre-allocated scratch
/// buffer to avoid per-tick heap allocation.
///
/// # How to Use
/// ```rust
/// // Inside a hypothetical game loop (not MKRWorld::tick itself):
/// let decisions = coordinator.scan(world.activation_field());
/// for decision in decisions {
///     match decision.action {
///         StreamAction::Prefetch => {
///             oasis_fabric.request_stream(decision.cell_index as u32);
///         }
///         StreamAction::PromoteResident => {
///             residency_tracker.request_load(decision.cell_index as u32);
///         }
///     }
/// }
/// ```
///
/// On stream completion, inject heat back into the field:
/// ```rust
/// world.inject_heat_at_chunk(x, y, STREAM_COMPLETION_HEAT);
/// ```
pub struct StreamingCoordinator {
    /// Reusable scratch buffer — avoids per-tick heap allocation.
    scratch: Vec<StreamingDecision>,
}

impl StreamingCoordinator {
    pub fn new() -> Self {
        Self {
            scratch: Vec::with_capacity(MAX_STREAM_REQUESTS_PER_TICK),
        }
    }

    /// Scan the activation field and return streaming decisions.
    ///
    /// Returns cells in descending probability order, bounded to
    /// `MAX_STREAM_REQUESTS_PER_TICK`.
    ///
    /// # Decision Logic (branchless-structured)
    /// For each cell:
    ///   * probability >= STREAM_RESIDENT_THRESHOLD → PromoteResident
    ///   * probability >= STREAM_PREFETCH_THRESHOLD → Prefetch
    ///   * probability <  STREAM_PREFETCH_THRESHOLD → skip
    ///
    /// # Returns
    /// Immutable slice valid until next call to `scan()`.
    pub fn scan<'a>(&'a mut self, field: &ActivationField) -> &'a [StreamingDecision] {
        self.scratch.clear();

        for (idx, cell) in field.cells.iter().enumerate() {
            let p = cell.execution_probability;

            if p >= STREAM_PREFETCH_THRESHOLD {
                let action = if p >= STREAM_RESIDENT_THRESHOLD {
                    StreamAction::PromoteResident
                } else {
                    StreamAction::Prefetch
                };
                self.scratch.push(StreamingDecision {
                    cell_index:  idx,
                    action,
                    probability: p,
                });
            }
        }

        // Budget cap: keep highest-probability decisions.
        let budget = MAX_STREAM_REQUESTS_PER_TICK.min(self.scratch.len());
        if self.scratch.len() > budget {
            self.scratch.select_nth_unstable_by(budget - 1, |a, b| {
                b.probability
                    .partial_cmp(&a.probability)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.scratch.truncate(budget);
        }

        &self.scratch[..budget]
    }

    /// Check if a single cell should initiate prefetch.
    ///
    /// Use this for per-cell queries without a full field scan.
    #[inline]
    pub fn should_prefetch(&self, probability: f32) -> bool {
        probability >= STREAM_PREFETCH_THRESHOLD
    }

    /// Check if a single cell should be promoted to resident VRAM.
    #[inline]
    pub fn should_promote_resident(&self, probability: f32) -> bool {
        probability >= STREAM_RESIDENT_THRESHOLD
    }

    /// Compute the heat injection amount for a completed stream.
    ///
    /// Scales the base `STREAM_COMPLETION_HEAT` by the cell's probability
    /// at completion time — higher probability cells get slightly more heat.
    #[inline]
    pub fn completion_heat(&self, probability_at_request: f32) -> f32 {
        // Linear scale: cells that were more likely to execute when
        // streaming was requested get proportionally more heat.
        STREAM_COMPLETION_HEAT * (0.5 + 0.5 * probability_at_request)
    }
}

impl Default for StreamingCoordinator {
    fn default() -> Self { Self::new() }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;

    fn field_with_prob(w: usize, h: usize, prob: f32) -> ActivationField {
        let mut f = ActivationField::new(w, h);
        for cell in &mut f.cells {
            cell.execution_probability = prob;
        }
        f
    }

    #[test]
    fn dormant_field_produces_no_decisions() {
        let field = field_with_prob(4, 4, 0.0);
        let mut coord = StreamingCoordinator::new();
        assert_eq!(coord.scan(&field).len(), 0);
    }

    #[test]
    fn cells_above_prefetch_threshold_get_prefetch_action() {
        let field = field_with_prob(4, 4, STREAM_PREFETCH_THRESHOLD + 0.01);
        let mut coord = StreamingCoordinator::new();
        let decisions = coord.scan(&field);
        assert!(!decisions.is_empty());
        for d in decisions {
            assert_eq!(d.action, StreamAction::Prefetch,
                "cell {} should be Prefetch, not {:?}", d.cell_index, d.action);
        }
    }

    #[test]
    fn cells_above_resident_threshold_get_promote_action() {
        let field = field_with_prob(4, 4, STREAM_RESIDENT_THRESHOLD + 0.01);
        let mut coord = StreamingCoordinator::new();
        let decisions = coord.scan(&field);
        assert!(!decisions.is_empty());
        for d in decisions {
            assert_eq!(d.action, StreamAction::PromoteResident,
                "cell {} should be PromoteResident, not {:?}", d.cell_index, d.action);
        }
    }

    #[test]
    fn decisions_bounded_by_budget() {
        // 32×32 = 1024 cells, all above threshold
        let field = field_with_prob(32, 32, 1.0);
        let mut coord = StreamingCoordinator::new();
        let decisions = coord.scan(&field);
        assert!(decisions.len() <= MAX_STREAM_REQUESTS_PER_TICK,
            "decisions {} exceeded budget {}", decisions.len(), MAX_STREAM_REQUESTS_PER_TICK);
    }

    #[test]
    fn completion_heat_scales_with_probability() {
        let coord = StreamingCoordinator::new();
        let low = coord.completion_heat(0.0);
        let high = coord.completion_heat(1.0);
        assert!(high > low, "higher probability should give more heat");
        assert!(high <= 1.0, "completion heat must not exceed 1.0: {}", high);
    }
}
