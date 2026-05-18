// ===================================================================
// mirage-mkr-core/src/activation/sparse.rs
// PURPOSE: Sparse Activation Solver — Frontier-Local Propagation
//
// ---------------------------------------------------------------
// DESIGN PHILOSOPHY
// ---------------------------------------------------------------
//
// The full solver (ActivationSolver::step) recomputes every field cell
// every tick.  This module provides frontier-local equivalents of each
// solver pass that operate ONLY on cells in the PropagationFrontier.
//
// PASS ANALYSIS — which passes are frontier-safe:
//
//   PASS            FRONTIER-LOCAL?   HALO NEEDED?  NOTES
//   ─────────────── ──────────────── ───────────── ──────────────────
//   decay()         NO               N/A           Continuous, all cells
//                                                  decay every frame.
//                                                  Exception: skip if
//                                                  heat < DECAY_SKIP_THRESH.
//   diffuse()       YES              READ-ONLY     Neighbours are READ
//                                                  from previous-frame
//                                                  values (no write outside
//                                                  frontier). Safe.
//   propagate_pressure() YES         READ-ONLY     Same as diffuse: reads
//                                                  neighbours but only
//                                                  writes frontier cells.
//   recompute_activation() YES       NONE          Pure local formula.
//   recompute_probability() YES      NONE          Pure local formula.
//
// DECAY DECISION:
//   Heat decay is multiplicative and continuous.  Even a perfectly
//   stable field has every cell decaying by HEAT_DECAY every tick.
//   However, cells with heat < DECAY_SKIP_THRESH (1e-5) contribute
//   effectively zero to all subsequent passes.  Skipping them is safe.
//   This is the only sparse-safe optimisation for decay.
//
// DIFFUSE HALO ANALYSIS:
//   The stencil for cell (x,y) reads (x±1, y) and (x, y±1).
//   In sparse mode, we only WRITE frontier cells.
//   We READ neighbours from the current (pre-diffuse) field state.
//   This is correct for an explicit finite-difference scheme —
//   each cell reads the unmodified previous values of its neighbours.
//   No write-outside-frontier occurs.
//
// DIVERGENCE RISK:
//   If a non-frontier cell has significant heat gradient against a
//   frontier cell, the frontier cell's new_heat will be computed
//   correctly (it reads the neighbour's actual heat).  However, the
//   non-frontier neighbour will NOT receive the reciprocal diffusion.
//   This causes asymmetric diffusion at frontier boundaries.
//   Mitigation: halo expansion includes all 4-neighbours of changed
//   cells — so the immediate boundary is always in the frontier.
//   Second-order boundary effects remain until the next tick.
//
// TODO(V3-SPARSE-VALIDATION): After 1000 ticks, compare per-cell
// drift between step() and step_sparse() outputs.  If max drift
// > SPARSE_DIVERGENCE_EPSILON (1e-3), trigger full-field fallback.
//
// ===================================================================

use super::field::{ActivationField, HEAT_DECAY, DIFFUSION_ALPHA,
                   ENTROPY_GROWTH, ENTROPY_DECAY, PRESSURE_STABILISATION};
use super::frontier::PropagationFrontier;

// =====================================================================
// CONSTANTS
// =====================================================================

/// Minimum heat value below which decay can be skipped.
/// Cells below this contribute < 1e-5 to all downstream passes.
pub const DECAY_SKIP_THRESH: f32 = 1e-5;

/// Maximum absolute drift (per-cell, per-field-scalar) between sparse
/// and full solver outputs before a full-field fallback is triggered.
pub const SPARSE_DIVERGENCE_EPSILON: f32 = 1e-3;

/// Maximum average drift across all field cells before hard fallback.
pub const SPARSE_MEAN_DRIFT_EPSILON: f32 = 1e-4;

// =====================================================================
// SPARSE STEP RESULT
// =====================================================================

/// Output from a single sparse solver step.
///
/// Carries the same diagnostic payload as `SolverStepStats` but also
/// records frontier coverage information for validation.
#[derive(Debug, Clone, Copy, Default)]
pub struct SparseSolverResult {
    /// Number of cells actually processed by the sparse passes.
    pub cells_processed: usize,
    /// Total field cells (for density computation).
    pub total_cells:     usize,
    /// Number of cells whose decay was skipped (heat < DECAY_SKIP_THRESH).
    pub decay_skipped:   usize,
    /// Whether a full-field decay pass was run (always true in V3 — see analysis above).
    pub full_decay_ran:  bool,
    /// Whether sparse mode was recommended by the frontier.
    pub used_sparse:     bool,
}

impl SparseSolverResult {
    /// Fraction of field processed by sparse passes [0.0, 1.0].
    #[inline]
    pub fn coverage_density(&self) -> f32 {
        if self.total_cells == 0 { return 0.0; }
        self.cells_processed as f32 / self.total_cells as f32
    }
}

// =====================================================================
// SPARSE SOLVER PASSES
// =====================================================================

/// Execute a selective decay pass.
///
/// Always runs over the full field for correctness (see analysis above),
/// but skips cells with heat and pressure below `DECAY_SKIP_THRESH`.
/// Returns the count of cells that were skipped.
///
/// TODO(V3-SPARSE-VALIDATION): Track skipped cell count and compare
/// against expected decay drift to validate skip threshold correctness.
pub fn decay_selective(field: &mut ActivationField) -> usize {
    let mut skipped = 0usize;
    for cell in &mut field.cells {
        if cell.heat < DECAY_SKIP_THRESH && cell.pressure < DECAY_SKIP_THRESH {
            // Entropy still needs to update for idle drift correctness.
            let idle_weight = 1.0 - cell.activation;
            cell.entropy = (cell.entropy
                + ENTROPY_GROWTH * idle_weight
                - ENTROPY_DECAY * cell.activation)
                .clamp(0.0, 1.0);
            skipped += 1;
            continue;
        }
        cell.heat     *= HEAT_DECAY;
        cell.pressure *= 1.0 - PRESSURE_STABILISATION;
        let idle_weight = 1.0 - cell.activation;
        cell.entropy = (cell.entropy
            + ENTROPY_GROWTH * idle_weight
            - ENTROPY_DECAY * cell.activation)
            .clamp(0.0, 1.0);
    }
    skipped
}

/// Frontier-local heat diffusion.
///
/// Only writes heat to cells in `frontier`.  Reads neighbours from the
/// pre-diffuse field state (explicit finite-difference — read-consistent).
///
/// The scratch buffer is pre-allocated by the solver; only frontier cells
/// are updated in it.  Non-frontier scratch entries remain stale but are
/// never read back for non-frontier cells.
///
/// # Halo Safety
/// Reads from non-frontier neighbours are safe: we only READ them, never
/// WRITE to them.  The value we read is the previous-tick heat, which is
/// correct for an explicit scheme.
///
/// # TODO(V3-SPARSE-VALIDATION): Asymmetric diffusion at frontier boundary —
/// frontier cell reads real neighbour heat, but non-frontier neighbour does
/// NOT receive the reciprocal update.  Asymmetry resolves in subsequent
/// ticks as the frontier expands.  Track max boundary asymmetry.
pub fn diffuse_frontier(
    field:    &mut ActivationField,
    frontier: &PropagationFrontier,
    scratch:  &mut Vec<f32>,
) {
    let n = field.cells.len();
    let w = field.width;
    let h = field.height;

    if scratch.len() < n { scratch.resize(n, 0.0); }

    // Compute new heat only for frontier cells.
    // Non-frontier scratch entries are not written (stale but unused).
    for &idx in frontier.iter_cells() {
        if idx >= n { continue; }
        let y = idx / w;
        let x = idx % w;
        let center = field.cells[idx].heat;

        let north = if y > 0     { field.cells[(y-1)*w + x].heat } else { center };
        let south = if y+1 < h   { field.cells[(y+1)*w + x].heat } else { center };
        let west  = if x > 0     { field.cells[y*w + (x-1)].heat } else { center };
        let east  = if x+1 < w   { field.cells[y*w + (x+1)].heat } else { center };

        scratch[idx] = center + DIFFUSION_ALPHA * (north + south + west + east - 4.0 * center);
    }

    // Write back only frontier cells.
    for &idx in frontier.iter_cells() {
        if idx < n {
            field.cells[idx].heat = scratch[idx].clamp(0.0, 1.0);
        }
    }
}

/// Frontier-local pressure propagation.
///
/// Only frontier cells receive pressure updates.  Topology influence is
/// applied additively, then 4-neighbour averaging smooths discontinuities.
///
/// # Two-pass approach
/// Pass A: inject topology influence into frontier cells only.
/// Pass B: 4-neighbour averaging for frontier cells only (reads from
///         scratch written in Pass A, or old field values for non-frontier
///         neighbours — safe read-consistent behaviour).
///
/// # TODO(V3-SPARSE-VALIDATION): Non-frontier neighbours participate in
/// the averaging read but receive no write.  This may cause frontier
/// cells to "drain" pressure into non-frontier zones asymmetrically.
/// Track max pressure gradient at frontier boundary.
pub fn propagate_pressure_frontier(
    field:          &mut ActivationField,
    frontier:       &PropagationFrontier,
    topo_influence: &[f32],
    scratch:        &mut Vec<f32>,
) {
    let n = field.cells.len();
    let w = field.width;
    let h = field.height;

    if scratch.len() < n { scratch.resize(n, 0.0); }

    // Pass A: topology influence injection for frontier cells.
    // Non-frontier cells: copy their existing pressure unchanged.
    // This is necessary for Pass B to correctly average with non-frontier neighbours.
    for i in 0..n {
        scratch[i] = field.cells[i].pressure;
    }
    for &idx in frontier.iter_cells() {
        if idx >= n { continue; }
        let infl = if idx < topo_influence.len() { topo_influence[idx] } else { 0.0 };
        scratch[idx] = (field.cells[idx].pressure + infl * 0.3).min(1.0);
    }

    // Pass B: 4-neighbour pressure average for frontier cells only.
    for &idx in frontier.iter_cells() {
        if idx >= n { continue; }
        let y = idx / w;
        let x = idx % w;
        let center = scratch[idx];

        let north = if y > 0   { scratch[(y-1)*w + x] } else { center };
        let south = if y+1 < h { scratch[(y+1)*w + x] } else { center };
        let west  = if x > 0   { scratch[y*w + (x-1)] } else { center };
        let east  = if x+1 < w { scratch[y*w + (x+1)] } else { center };

        field.cells[idx].pressure =
            (center * 0.5 + (north + south + west + east) * 0.125).clamp(0.0, 1.0);
    }
}

/// Frontier-local activation recomputation.
///
/// Only frontier cells have their `activation` scalar updated.
/// The formula is identical to the full-field pass — pure element-wise.
///
/// # Correctness
/// `activation = heat × 0.55 + pressure × 0.35 + (1 − entropy) × 0.10`
/// This is a pure local formula — no neighbour dependency.  Safe.
pub fn recompute_activation_frontier(
    field:    &mut ActivationField,
    frontier: &PropagationFrontier,
) {
    let n = field.cells.len();
    for &idx in frontier.iter_cells() {
        if idx >= n { continue; }
        let cell = &mut field.cells[idx];
        cell.activation = (cell.heat * 0.55
            + cell.pressure * 0.35
            + (1.0 - cell.entropy) * 0.10)
            .clamp(0.0, 1.0);
    }
}

/// Frontier-local execution probability recomputation.
///
/// Only frontier cells have their `execution_probability` updated.
/// Formula: smoothstep S-curve `a² × (3 − 2a)`.
///
/// # Correctness
/// Pure local formula — no neighbour dependency.  Safe.
pub fn recompute_probability_frontier(
    field:    &mut ActivationField,
    frontier: &PropagationFrontier,
) {
    let n = field.cells.len();
    for &idx in frontier.iter_cells() {
        if idx >= n { continue; }
        let a = field.cells[idx].activation;
        field.cells[idx].execution_probability = a * a * (3.0 - 2.0 * a);
    }
}

// =====================================================================
// FULL SPARSE STEP
// =====================================================================

/// Execute a sparse activation field step over frontier cells only.
///
/// This is the primary entry point for the differential runtime.
/// It runs all five solver passes in frontier-local mode and returns
/// a `SparseSolverResult` describing coverage and skip statistics.
///
/// # Execution Order
/// 1. `decay_selective()`            — full field, skips near-zero cells
/// 2. `diffuse_frontier()`           — frontier only, read-halo safe
/// 3. `propagate_pressure_frontier()` — frontier only, read-halo safe
/// 4. `recompute_activation_frontier()` — frontier only, pure local
/// 5. `recompute_probability_frontier()` — frontier only, pure local
///
/// # Full Solver Availability
/// The full solver `ActivationSolver::step()` is NOT replaced by this
/// function.  Both coexist.  `step_sparse()` is called in VALIDATION
/// MODE alongside `step()` for parity comparison.
///
/// TODO(V3-SPARSE-VALIDATION): Once 1000-tick parity validation passes
/// (max drift < SPARSE_DIVERGENCE_EPSILON), promote step_sparse() to
/// the authoritative path and make step() the fallback.
pub fn step_sparse(
    field:          &mut ActivationField,
    frontier:       &PropagationFrontier,
    topo_influence: &[f32],
    diff_scratch:   &mut Vec<f32>,
    pres_scratch:   &mut Vec<f32>,
) -> SparseSolverResult {
    let total = field.cells.len();

    // If frontier is empty, nothing to do — field is stable.
    if frontier.is_empty() {
        return SparseSolverResult {
            cells_processed: 0,
            total_cells:     total,
            decay_skipped:   0,
            full_decay_ran:  true,
            used_sparse:     true,
        };
    }

    // Pass 1: selective decay (touches all cells for entropy correctness)
    let decay_skipped = decay_selective(field);

    // Passes 2–5: frontier-local only
    if frontier.should_use_sparse() {
        diffuse_frontier(field, frontier, diff_scratch);
        propagate_pressure_frontier(field, frontier, topo_influence, pres_scratch);
        recompute_activation_frontier(field, frontier);
        recompute_probability_frontier(field, frontier);

        SparseSolverResult {
            cells_processed: frontier.frontier_size(),
            total_cells:     total,
            decay_skipped,
            full_decay_ran:  true,
            used_sparse:     true,
        }
    } else {
        // Frontier too large — fall back to full passes for phases 2-5
        // but still use decay_selective result from phase 1.
        field.diffuse(diff_scratch);
        // Rebuild pressure scratch from full field
        if pres_scratch.len() != total { pres_scratch.resize(total, 0.0); }
        for (i, cell) in field.cells.iter().enumerate() {
            let infl = if i < topo_influence.len() { topo_influence[i] } else { 0.0 };
            pres_scratch[i] = (cell.pressure + infl * 0.3).min(1.0);
        }
        // Full pressure smoothing
        let w = field.width;
        let h = field.height;
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let c = pres_scratch[idx];
                let n = if y > 0     { pres_scratch[(y-1)*w+x] } else { c };
                let s = if y+1 < h   { pres_scratch[(y+1)*w+x] } else { c };
                let ww = if x > 0    { pres_scratch[y*w+(x-1)] } else { c };
                let e = if x+1 < w   { pres_scratch[y*w+(x+1)] } else { c };
                field.cells[idx].pressure = (c*0.5 + (n+s+ww+e)*0.125).clamp(0.0, 1.0);
            }
        }
        field.recompute_activation();
        field.recompute_execution_probability();

        SparseSolverResult {
            cells_processed: total,
            total_cells:     total,
            decay_skipped,
            full_decay_ran:  true,
            used_sparse:     false,
        }
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;
    use crate::activation::delta::{FieldDeltaTracker};
    use crate::activation::frontier::PropagationFrontier;

    fn make_frontier_from_cell(w: usize, h: usize, cell: usize) -> PropagationFrontier {
        let mut delta = crate::activation::delta::FieldDeltaMask::new(w * h);
        delta.set(cell);
        let mut frontier = PropagationFrontier::new(w, h);
        frontier.build_from_delta(&delta, w, h);
        frontier
    }

    #[test]
    fn sparse_step_empty_frontier_is_stable() {
        let mut field = ActivationField::new(4, 4);
        field.inject_heat(0, 0.5);
        let frontier = PropagationFrontier::new(4, 4); // empty
        let mut d = Vec::new();
        let mut p = Vec::new();
        let result = step_sparse(&mut field, &frontier, &[], &mut d, &mut p);
        assert_eq!(result.cells_processed, 0);
        assert!(result.used_sparse);
    }

    #[test]
    fn sparse_diffuse_frontier_cell_changes() {
        let mut field = ActivationField::new(4, 4);
        field.cells[5].heat = 0.8; // center of 4×4
        let frontier = make_frontier_from_cell(4, 4, 5);
        let mut scratch = Vec::new();
        let heat_before = field.cells[5].heat;
        diffuse_frontier(&mut field, &frontier, &mut scratch);
        // Cell 5 should have diffused heat to/from neighbours
        // Centre had 0.8, all neighbours 0.0: expected decrease
        assert!(field.cells[5].heat < heat_before,
            "centre cell heat should decrease when surrounded by cool neighbours");
    }

    #[test]
    fn sparse_recompute_activation_is_local() {
        let mut field = ActivationField::new(4, 4);
        field.cells[3].heat = 0.9;
        let frontier = make_frontier_from_cell(4, 4, 3);
        recompute_activation_frontier(&mut field, &frontier);
        // Cell 3 activation should be non-zero
        assert!(field.cells[3].activation > 0.0);
        // Cell 0 should be untouched
        assert_eq!(field.cells[0].activation, 0.0);
    }

    #[test]
    fn sparse_probability_is_bounded() {
        let mut field = ActivationField::new(4, 4);
        field.cells[0].activation = 1.0;
        let frontier = make_frontier_from_cell(4, 4, 0);
        recompute_probability_frontier(&mut field, &frontier);
        assert!(field.cells[0].execution_probability <= 1.0);
        assert!(field.cells[0].execution_probability > 0.0);
    }

    #[test]
    fn decay_selective_skips_cold_cells() {
        let mut field = ActivationField::new(4, 4);
        // Only cell 0 is hot
        field.cells[0].heat = 0.5;
        let skipped = decay_selective(&mut field);
        // 15 cold cells should be skipped for heat/pressure decay
        assert_eq!(skipped, 15);
        assert!(field.cells[0].heat < 0.5, "hot cell should have decayed");
    }

    #[test]
    fn step_sparse_produces_valid_result() {
        let mut field = ActivationField::new(8, 8);
        field.inject_heat(20, 0.7);
        let mut tracker = FieldDeltaTracker::new(64, 0.05);
        tracker.compute(&field);
        field.cells[20].heat = 0.9;
        let mask = tracker.compute(&field);
        let mut frontier = PropagationFrontier::new(8, 8);
        frontier.build_from_delta(mask, 8, 8);
        let mut d = Vec::new();
        let mut p = Vec::new();
        let result = step_sparse(&mut field, &frontier, &[], &mut d, &mut p);
        assert!(result.total_cells == 64);
        assert!(result.coverage_density() <= 1.0);
    }
}
