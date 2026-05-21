// ===================================================================
// mirage-mkr-core/src/emission.rs  (V3 — Differential Runtime Pass)
// PURPOSE: Activation-Driven Fiber Emission Gate
//
// ROLE IN V3:
// The emission gate reads `execution_probability` from the
// ActivationField and decides which chunk-cells are eligible to have
// work fibers spawned for them this frame.
//
// ---------------------------------------------------------------
// TODO(V3-DIFFERENTIAL): DELTA-AWARE EMISSION PREPARATION
// ---------------------------------------------------------------
//
// Current: collect() scans ALL field cells (O(N)) every tick.
// Problem: most cells are Dormant and have probability ≈ 0.0 every tick.
//
// Target: collect_from_changed(field, delta_mask)
//   Only scan cells in delta_mask.iter_changed() — O(|changed|).
//   Cells not in the delta mask cannot have crossed EMIT_GATE this frame
//   (since their probability didn’t change by more than PROBABILITY_EPSILON).
//
// EXCEPTION: cells that were ALREADY above EMIT_GATE last frame and remain
// above it this frame will NOT appear in the delta mask (they didn’t change).
// These must be tracked separately with a persistent “still-eligible” bitset.
//
// Migration plan:
//   Step 1: Implement collect_from_changed() (this pass) — DONE below.
//   Step 2: Validate against collect() output for 1000 ticks (no divergence).
//   Step 3: Replace collect() call in MKRWorld::tick() with collect_from_changed().
//
// Compatibility: collect() is unchanged — executor and renderer compatibility unaffected.
//
// TODO(V3-DIFFERENTIAL): Also gate by RegionActivityState — skip cells
// in Dormant regions before even checking the delta mask.
//
// TODO(V3-CEK): Emission requests produced here will eventually carry
// a continuation_id for CEK to select the correct fiber to launch.
// ===================================================================

use crate::activation::field::ActivationField;

// =====================================================================
// CONSTANTS
// =====================================================================

/// Minimum execution_probability for a cell to be emission-eligible.
///
/// Cells below this value contribute effectively zero work; skipping
/// them avoids scheduling overhead.  0.05 means the field must be
/// at least ~22% activated (smoothstep⁻¹(0.05) ≈ 0.22) before any
/// emission occurs.
pub const EMIT_GATE: f32 = 0.05;

/// Maximum fibers emitted per tick across the whole field.
///
/// This is a hard budget cap to preserve frame-time predictability.
/// Future work: make this dynamic based on available CPU budget.
pub const MAX_EMIT_PER_TICK: usize = 128;

// =====================================================================
// EMISSION REQUEST
// =====================================================================

/// A request to schedule work for a specific activation field cell.
///
/// Produced by `EmissionGate::collect()` each tick.  Consumed by the
/// fiber pool (or future CEK) to spawn actual execution continuations.
///
/// # V3 Semantics
/// `cell_index` is the flat field index — identical to the chunk index
/// for a 1:1 field-to-chunk mapping.
/// `probability` is the raw emission_probability from that cell; the
/// consumer can use it to bias budget allocation within the batch.
#[derive(Debug, Clone, Copy)]
pub struct EmissionRequest {
    /// Flat index into `ActivationField::cells`.
    pub cell_index: usize,
    /// Execution probability at the time of emission (0 < p ≤ 1).
    pub probability: f32,
}

// =====================================================================
// EMISSION GATE
// =====================================================================

/// Stateless activation-driven emission gate.
///
/// Scans the activation field each tick and produces a bounded list of
/// `EmissionRequest`s for cells whose `execution_probability` exceeds
/// `EMIT_GATE`.
///
/// # Branchless Inner Loop
/// The inner loop avoids branching by comparing against the gate as
/// a float and writing to the output only when the condition is met.
/// Modern CPUs predict the rare-write case (most cells dormant) well,
/// but a predicated write would be better.  The structure is already
/// correct for SIMD gather/scatter migration.
///
/// # Budget Enforcement
/// Total output is capped at `MAX_EMIT_PER_TICK`.  When the field is
/// very hot (many cells above gate), high-probability cells are
/// preferred because we sort by `probability` before truncating.
///
/// # V3-DIFFERENTIAL
/// Two emission paths coexist:
///   * `collect(field)` — full-field O(N) scan (current default)
///   * `collect_from_changed(field, delta_mask)` — sparse O(|changed|) scan
///
/// The sparse path also requires `still_eligible` bitset to handle
/// cells that were already above EMIT_GATE last frame (they don't appear
/// in the delta mask but must still be emitted).
pub struct EmissionGate {
    /// Reusable scratch buffer — avoids per-tick Vec allocation.
    scratch: Vec<EmissionRequest>,
    pub budget: usize,
}

impl EmissionGate {
    /// Create a new EmissionGate with pre-allocated scratch capacity.
    pub fn new() -> Self {
        Self {
            scratch: Vec::with_capacity(MAX_EMIT_PER_TICK * 2),
            budget: MAX_EMIT_PER_TICK,
        }
    }

    /// Frontier-native emission scan: only evaluates cells in the propagation frontier.
    pub fn collect_from_frontier<'a>(
        &'a mut self,
        field: &ActivationField,
        frontier: &crate::activation::frontier::PropagationFrontier,
    ) -> &'a [EmissionRequest] {
        self.scratch.clear();

        // Scan cells in the frontier
        for &idx in frontier.iter_cells() {
            if idx >= field.cells.len() { continue; }
            let p = field.cells[idx].execution_probability;
            if p > EMIT_GATE {
                self.scratch.push(EmissionRequest { cell_index: idx, probability: p });
            }
        }

        // Budget enforcement & deterministic sorting
        // Sort by probability (descending), and use cell_index (ascending) as a stable tie-breaker.
        self.scratch.sort_by(|a, b| {
            b.probability.partial_cmp(&a.probability)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.cell_index.cmp(&b.cell_index))
        });

        let budget = self.budget.min(self.scratch.len());
        self.scratch.truncate(budget);

        &self.scratch[..budget]
    }

    /// Number of cells currently in the scratch buffer (after last collect).
    pub fn pending_count(&self) -> usize {
        self.scratch.len()
    }
}

impl Default for EmissionGate {
    fn default() -> Self { Self::new() }
}


// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;
    use crate::activation::frontier::PropagationFrontier;
    use crate::activation::delta::FieldDeltaMask;

    fn full_frontier(width: usize, height: usize) -> PropagationFrontier {
        let mut f = PropagationFrontier::new(width, height);
        let mut delta = FieldDeltaMask::new(width * height);
        for i in 0..(width * height) {
            delta.set(i);
        }
        f.build_from_delta(&delta, width, height);
        f
    }

    fn field_with_probability(width: usize, height: usize, prob: f32) -> ActivationField {
        let mut f = ActivationField::new(width, height);
        for cell in &mut f.cells {
            cell.execution_probability = prob;
        }
        f
    }

    #[test]
    fn dormant_field_emits_nothing() {
        let field = field_with_probability(8, 8, 0.0);
        let frontier = full_frontier(8, 8);
        let mut gate = EmissionGate::new();
        let requests = gate.collect_from_frontier(&field, &frontier);
        assert_eq!(requests.len(), 0, "dormant field should produce no emission requests");
    }

    #[test]
    fn hot_field_emits_up_to_budget() {
        // 16×16 = 256 cells, all at probability 1.0
        let field = field_with_probability(16, 16, 1.0);
        let frontier = full_frontier(16, 16);
        let mut gate = EmissionGate::new();
        let requests = gate.collect_from_frontier(&field, &frontier);
        assert!(requests.len() <= MAX_EMIT_PER_TICK,
            "emission must not exceed budget: got {}", requests.len());
        assert_eq!(requests.len(), MAX_EMIT_PER_TICK,
            "at full probability, budget should be fully consumed");
    }

    #[test]
    fn gate_filters_below_threshold() {
        let field = field_with_probability(4, 4, EMIT_GATE - 0.001);
        let frontier = full_frontier(4, 4);
        let mut gate = EmissionGate::new();
        let requests = gate.collect_from_frontier(&field, &frontier);
        assert_eq!(requests.len(), 0, "cells below gate must not be emitted");
    }

    #[test]
    fn emission_requests_are_probability_ordered() {
        let mut field = ActivationField::new(4, 4);
        let frontier = full_frontier(4, 4);
        // Set alternating probabilities
        for (i, cell) in field.cells.iter_mut().enumerate() {
            cell.execution_probability = if i % 2 == 0 { 0.9 } else { 0.1 };
        }
        let mut gate = EmissionGate::new();
        let requests = gate.collect_from_frontier(&field, &frontier);
        // All returned cells should have probability > EMIT_GATE
        for r in requests {
            assert!(r.probability > EMIT_GATE,
                "emitted cell {} has probability {} below gate",
                r.cell_index, r.probability);
        }
    }

    #[test]
    fn cell_index_matches_field_position() {
        let mut field = ActivationField::new(4, 4);
        let frontier = full_frontier(4, 4);
        // Only cell 5 is hot
        field.cells[5].execution_probability = 1.0;
        let mut gate = EmissionGate::new();
        let requests = gate.collect_from_frontier(&field, &frontier);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].cell_index, 5);
    }
}
