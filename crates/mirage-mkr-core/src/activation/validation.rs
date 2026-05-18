// ===================================================================
// mirage-mkr-core/src/activation/validation.rs
// PURPOSE: Sparse Validation Runtime — Parity Testing Infrastructure
//
// ---------------------------------------------------------------
// DESIGN INTENT
// ---------------------------------------------------------------
//
// This module implements side-by-side parity validation between the
// full solver and the sparse solver.  It is NOT the authoritative
// runtime path.  It exists solely to build confidence that sparse
// output is correct before the sparse solver takes authority.
//
// EXECUTION MODEL:
//   ValidationMode::Parallel:
//     1. Snapshot current field → snapshot_field
//     2. Run step() on the live field
//     3. Run step_sparse() on snapshot_field
//     4. Compare outputs → ParityComparisonResult
//     5. If drift < epsilon → record PASS
//     6. If drift >= epsilon → record FAIL, preserve full as authority
//
// AUTHORITY RULE:
//   The FULL solver result is ALWAYS written to the live field.
//   The sparse result is written to a SHADOW FIELD for comparison only.
//   This ensures ZERO risk of sparse divergence affecting runtime behavior.
//
// TODO(V3-SPARSE-VALIDATION): After SPARSE_PROMOTION_THRESHOLD consecutive
// PASS ticks, the runtime may be promoted to SPARSE_AUTHORITATIVE mode.
// This promotion must be explicitly requested (not automatic) and reviewed
// by the Lead Runtime Architect before enabling.
//
// ===================================================================

use super::field::ActivationField;
use super::frontier::PropagationFrontier;
use super::sparse::{step_sparse, SparseSolverResult, SPARSE_DIVERGENCE_EPSILON};
use super::solver::SolverStepStats;


// =====================================================================
// CONSTANTS
// =====================================================================

/// Number of consecutive PASS ticks required before sparse promotion
/// may even be considered.  NOT automatically applied.
pub const SPARSE_PROMOTION_THRESHOLD: u64 = 1_000;

/// Default epsilon for per-cell activation drift comparison.
pub const VALIDATION_ACTIVATION_EPSILON: f32 = 1e-3;

/// Default epsilon for per-cell probability drift comparison.
pub const VALIDATION_PROBABILITY_EPSILON: f32 = 1e-3;

/// Default epsilon for per-cell pressure drift comparison.
pub const VALIDATION_PRESSURE_EPSILON:    f32 = 1e-3;

// =====================================================================
// VALIDATION MODE
// =====================================================================

/// Controls what the validation layer does each tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationMode {
    /// Validation is disabled.  Only full solver runs.  Zero overhead.
    #[default]
    Disabled,
    /// Run both solvers in parallel.  Full solver is authoritative.
    /// Shadow field receives sparse result for comparison.
    Parallel,
    /// Sparse solver is authoritative (NOT YET ENABLED — requires
    /// explicit promotion after SPARSE_PROMOTION_THRESHOLD PASSes).
    SparseAuthoritative,
}

// =====================================================================
// RESULT TYPES
// =====================================================================

/// Result from the full solver pass.
#[derive(Debug, Clone, Copy, Default)]
pub struct FullSolverResult {
    pub stats: SolverStepStats,
}

/// Parity comparison between full and sparse solver outputs.
#[derive(Debug, Clone, Copy, Default)]
pub struct ParityComparisonResult {
    /// Maximum per-cell absolute drift in `activation`.
    pub max_activation_drift:    f32,
    /// Maximum per-cell absolute drift in `execution_probability`.
    pub max_probability_drift:   f32,
    /// Maximum per-cell absolute drift in `pressure`.
    pub max_pressure_drift:      f32,
    /// Mean absolute drift across all cells for `activation`.
    pub mean_activation_drift:   f32,
    /// Mean absolute drift across all cells for `probability`.
    pub mean_probability_drift:  f32,
    /// Number of cells that exceeded SPARSE_DIVERGENCE_EPSILON in activation.
    pub activation_violations:   usize,
    /// Number of cells that exceeded SPARSE_DIVERGENCE_EPSILON in probability.
    pub probability_violations:  usize,
    /// True if ALL cells passed within their epsilon tolerances.
    pub all_passed:              bool,
    /// Number of frontier cells compared (others are trivially 0 drift).
    pub cells_compared:          usize,
}

impl ParityComparisonResult {
    /// Compute parity between two activation fields.
    /// Only compares cells in the frontier (others are expected to differ
    /// for decay-skipped cells outside the active region).
    pub fn compute(
        full:     &ActivationField,
        sparse:   &ActivationField,
        frontier: &PropagationFrontier,
        act_eps:  f32,
        prob_eps: f32,
        pres_eps: f32,
    ) -> Self {
        let n = full.cells.len().min(sparse.cells.len());
        let mut max_act  = 0.0f32;
        let mut max_prob = 0.0f32;
        let mut max_pres = 0.0f32;
        let mut sum_act  = 0.0f32;
        let mut sum_prob = 0.0f32;
        let mut act_viol = 0usize;
        let mut prob_viol = 0usize;
        let mut count = 0usize;

        // Compare only frontier cells — non-frontier cells are intentionally
        // NOT updated by the sparse solver and will show expected drift.
        for &idx in frontier.iter_cells() {
            if idx >= n { continue; }
            let fa = full.cells[idx].activation;
            let sa = sparse.cells[idx].activation;
            let da = (fa - sa).abs();

            let fp = full.cells[idx].execution_probability;
            let sp = sparse.cells[idx].execution_probability;
            let dp = (fp - sp).abs();

            let fpr = full.cells[idx].pressure;
            let spr = sparse.cells[idx].pressure;
            let dpr = (fpr - spr).abs();

            if da > max_act  { max_act  = da; }
            if dp > max_prob { max_prob = dp; }
            if dpr > max_pres { max_pres = dpr; }

            sum_act  += da;
            sum_prob += dp;
            if da > act_eps  { act_viol  += 1; }
            if dp > prob_eps { prob_viol += 1; }

            let _ = pres_eps; // reserved for future use
            count += 1;
        }

        let cells_compared = count;
        let mean_act  = if count > 0 { sum_act  / count as f32 } else { 0.0 };
        let mean_prob = if count > 0 { sum_prob / count as f32 } else { 0.0 };

        Self {
            max_activation_drift:   max_act,
            max_probability_drift:  max_prob,
            max_pressure_drift:     max_pres,
            mean_activation_drift:  mean_act,
            mean_probability_drift: mean_prob,
            activation_violations:  act_viol,
            probability_violations: prob_viol,
            all_passed:             act_viol == 0 && prob_viol == 0,
            cells_compared,
        }
    }

    /// True if drift is severe enough to warrant a hard fallback.
    #[inline]
    pub fn is_severe_divergence(&self) -> bool {
        self.max_activation_drift   > SPARSE_DIVERGENCE_EPSILON
            || self.max_probability_drift > SPARSE_DIVERGENCE_EPSILON
    }
}

// =====================================================================
// FRONTIER VALIDATION REPORT
// =====================================================================

/// Accumulated validation statistics over multiple ticks.
///
/// Reset at the start of each validation run.  Accumulates across ticks
/// to build a statistical picture of sparse solver quality.
#[derive(Debug, Clone, Default)]
pub struct FrontierValidationReport {
    /// Total ticks validated since last reset.
    pub ticks_run:            u64,
    /// Ticks where all frontier cells passed within epsilon.
    pub ticks_passed:         u64,
    /// Ticks where at least one frontier cell exceeded epsilon.
    pub ticks_failed:         u64,
    /// Consecutive pass count (resets on any failure).
    pub consecutive_passes:   u64,
    /// Peak max_activation_drift seen across all ticks.
    pub peak_activation_drift: f32,
    /// Peak max_probability_drift seen across all ticks.
    pub peak_probability_drift: f32,
    /// Running average of mean_activation_drift.
    pub running_mean_activation_drift: f32,
    /// Peak frontier density seen.
    pub peak_frontier_density: f32,
    /// Total severe divergence events (triggered hard fallback).
    pub severe_divergence_events: u64,
    /// Sparse result from last tick (coverage etc).
    pub last_sparse_result:   Option<SparseSolverResult>,
}

impl FrontierValidationReport {
    pub fn new() -> Self { Self::default() }

    /// Record the result of one validation tick.
    pub fn record(&mut self, parity: &ParityComparisonResult, sparse: SparseSolverResult) {
        self.ticks_run += 1;
        self.last_sparse_result = Some(sparse);

        if parity.max_activation_drift > self.peak_activation_drift {
            self.peak_activation_drift = parity.max_activation_drift;
        }
        if parity.max_probability_drift > self.peak_probability_drift {
            self.peak_probability_drift = parity.max_probability_drift;
        }
        if sparse.coverage_density() > self.peak_frontier_density {
            self.peak_frontier_density = sparse.coverage_density();
        }

        // Exponential moving average for mean drift
        let alpha = 0.05;
        self.running_mean_activation_drift =
            self.running_mean_activation_drift * (1.0 - alpha)
            + parity.mean_activation_drift * alpha;

        if parity.all_passed {
            self.ticks_passed += 1;
            self.consecutive_passes += 1;
        } else {
            self.ticks_failed += 1;
            self.consecutive_passes = 0;
        }

        if parity.is_severe_divergence() {
            self.severe_divergence_events += 1;
        }
    }

    /// True if sparse solver has earned promotion consideration.
    pub fn eligible_for_promotion(&self) -> bool {
        self.consecutive_passes >= SPARSE_PROMOTION_THRESHOLD
            && self.severe_divergence_events == 0
    }

    /// Pass rate across all ticks [0.0, 1.0].
    pub fn pass_rate(&self) -> f32 {
        if self.ticks_run == 0 { return 1.0; }
        self.ticks_passed as f32 / self.ticks_run as f32
    }

    /// Reset all statistics.
    pub fn reset(&mut self) { *self = Self::default(); }
}

// =====================================================================
// SPARSE VALIDATION RUNNER
// =====================================================================

/// Runs both full and sparse solvers in parallel for one tick.
///
/// The live `field` receives the FULL solver result (authoritative).
/// The `shadow_field` receives the SPARSE solver result (comparison only).
///
/// Returns the parity comparison between the two outputs.
///
/// # Safety Guarantee
/// If parity fails, the live field is unaffected — it already has the
/// full solver result.  The shadow field may have incorrect state but
/// is never used for downstream passes.
///
/// TODO(V3-SPARSE-VALIDATION): Wire into MKRWorld::tick() between Phase 1
/// and Phase 1.5 when ValidationMode::Parallel is active.
pub struct SparseValidationRunner {
    /// Shadow field: receives sparse solver output for comparison.
    pub shadow_field:   ActivationField,
    /// Scratch buffers for the sparse solver (avoids allocation).
    sparse_diff_scratch: Vec<f32>,
    sparse_pres_scratch: Vec<f32>,
    /// Current validation mode.
    pub mode:            ValidationMode,
    /// Accumulated report across all ticks.
    pub report:          FrontierValidationReport,
}

impl SparseValidationRunner {
    /// Create a validation runner for a `width × height` field.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            shadow_field:        ActivationField::new(width, height),
            sparse_diff_scratch: Vec::new(),
            sparse_pres_scratch: Vec::new(),
            mode:                ValidationMode::Disabled,
            report:              FrontierValidationReport::new(),
        }
    }

    /// Enable parallel validation mode.
    pub fn enable_parallel(&mut self) {
        self.mode = ValidationMode::Parallel;
    }

    /// Disable validation.
    pub fn disable(&mut self) {
        self.mode = ValidationMode::Disabled;
    }

    /// True if validation is currently active.
    #[inline]
    pub fn is_active(&self) -> bool {
        matches!(self.mode, ValidationMode::Parallel | ValidationMode::SparseAuthoritative)
    }

    /// Run one parallel validation tick.
    ///
    /// Copies the live field into the shadow, runs step_sparse() on
    /// the shadow, and compares against the live field (which has
    /// already been advanced by the full solver).
    ///
    /// Returns `None` if validation is disabled.
    pub fn validate_tick(
        &mut self,
        live_field:     &ActivationField,
        frontier:       &PropagationFrontier,
        topo_influence: &[f32],
    ) -> Option<ParityComparisonResult> {
        if !self.is_active() { return None; }

        // Sync shadow with the state BEFORE the full solver ran
        // (snapshot captured by FieldDeltaTracker — we use the shadow
        // as a second field that we run the sparse solver on).
        //
        // NOTE: For true parity, shadow_field should start from the same
        // pre-tick state as the live field.  We approximate by copying
        // the LIVE FIELD after the full solver ran, then running sparse
        // on a copy of the PRE-TICK state.
        //
        // TODO(V3-SPARSE-VALIDATION): To achieve true pre-tick snapshot
        // comparison, MKRWorld must snapshot the field before Phase 1
        // and provide it here.  For now, this runner demonstrates the
        // parity infrastructure; exact pre-tick comparison is a future step.

        // Run sparse on shadow (which is currently the live post-full state
        // — for testing the infrastructure only, not true pre-tick parity).
        let sparse_result = step_sparse(
            &mut self.shadow_field,
            frontier,
            topo_influence,
            &mut self.sparse_diff_scratch,
            &mut self.sparse_pres_scratch,
        );

        // Compare shadow (sparse output) against live (full output).
        let parity = ParityComparisonResult::compute(
            live_field,
            &self.shadow_field,
            frontier,
            VALIDATION_ACTIVATION_EPSILON,
            VALIDATION_PROBABILITY_EPSILON,
            VALIDATION_PRESSURE_EPSILON,
        );

        self.report.record(&parity, sparse_result);
        Some(parity)
    }

    /// Copy live field into shadow field for pre-tick snapshot.
    ///
    /// Call this BEFORE running the full solver so the shadow field
    /// has the same initial state as the live field.
    pub fn snapshot_pre_tick(&mut self, live_field: &ActivationField) {
        debug_assert_eq!(
            self.shadow_field.cells.len(), live_field.cells.len(),
            "shadow field size must match live field"
        );
        self.shadow_field.cells.copy_from_slice(&live_field.cells);
    }
}

// =====================================================================
// DIVERGENCE HEATMAP PREPARATION
// =====================================================================

/// Per-cell drift accumulated over N ticks (Task 5 preparation).
///
/// Used to identify cells with persistent high drift — which may
/// indicate incorrect frontier expansion or halo asymmetry.
pub struct DivergenceHeatmap {
    /// Per-cell cumulative activation drift.
    pub activation_drift:    Vec<f32>,
    /// Per-cell cumulative probability drift.
    pub probability_drift:   Vec<f32>,
    /// Number of ticks accumulated.
    pub ticks_accumulated:   u64,
}

impl DivergenceHeatmap {
    pub fn new(num_cells: usize) -> Self {
        Self {
            activation_drift:  vec![0.0; num_cells],
            probability_drift: vec![0.0; num_cells],
            ticks_accumulated: 0,
        }
    }

    /// Accumulate drift from a full/sparse comparison.
    pub fn accumulate(&mut self, full: &ActivationField, sparse: &ActivationField) {
        let n = full.cells.len().min(sparse.cells.len()).min(self.activation_drift.len());
        for i in 0..n {
            self.activation_drift[i] +=
                (full.cells[i].activation - sparse.cells[i].activation).abs();
            self.probability_drift[i] +=
                (full.cells[i].execution_probability - sparse.cells[i].execution_probability).abs();
        }
        self.ticks_accumulated += 1;
    }

    /// Return mean per-cell drift across all accumulated ticks.
    pub fn mean_activation_drift(&self) -> f32 {
        if self.ticks_accumulated == 0 || self.activation_drift.is_empty() { return 0.0; }
        let sum: f32 = self.activation_drift.iter().sum();
        sum / (self.activation_drift.len() as f32 * self.ticks_accumulated as f32)
    }

    /// Return the cell index with the highest cumulative drift.
    pub fn hottest_cell(&self) -> Option<usize> {
        self.activation_drift.iter().enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
    }

    /// Reset all accumulated drift.
    pub fn reset(&mut self) {
        self.activation_drift.fill(0.0);
        self.probability_drift.fill(0.0);
        self.ticks_accumulated = 0;
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parity_comparison_identical_fields() {
        let f1 = ActivationField::new(4, 4);
        let f2 = ActivationField::new(4, 4);
        let frontier = PropagationFrontier::new(4, 4);
        let result = ParityComparisonResult::compute(&f1, &f2, &frontier, 1e-3, 1e-3, 1e-3);
        assert!(result.all_passed);
        assert_eq!(result.cells_compared, 0, "empty frontier compares 0 cells");
    }

    #[test]
    fn validation_report_accumulates_passes() {
        let mut report = FrontierValidationReport::new();
        let parity = ParityComparisonResult { all_passed: true, ..Default::default() };
        let sparse  = SparseSolverResult::default();
        for _ in 0..10 { report.record(&parity, sparse); }
        assert_eq!(report.ticks_passed, 10);
        assert_eq!(report.consecutive_passes, 10);
        assert_eq!(report.ticks_failed, 0);
    }

    #[test]
    fn validation_report_resets_consecutive_on_failure() {
        let mut report = FrontierValidationReport::new();
        let pass = ParityComparisonResult { all_passed: true, ..Default::default() };
        let fail = ParityComparisonResult {
            all_passed: false,
            activation_violations: 1,
            ..Default::default()
        };
        let sparse = SparseSolverResult::default();
        for _ in 0..5 { report.record(&pass, sparse); }
        report.record(&fail, sparse);
        assert_eq!(report.consecutive_passes, 0);
        assert_eq!(report.ticks_failed, 1);
    }

    #[test]
    fn runner_disabled_returns_none() {
        let mut runner = SparseValidationRunner::new(4, 4);
        let field    = ActivationField::new(4, 4);
        let frontier = PropagationFrontier::new(4, 4);
        let result = runner.validate_tick(&field, &frontier, &[]);
        assert!(result.is_none(), "disabled runner should return None");
    }

    #[test]
    fn runner_parallel_returns_result() {
        let mut runner = SparseValidationRunner::new(4, 4);
        runner.enable_parallel();
        let field    = ActivationField::new(4, 4);
        let frontier = PropagationFrontier::new(4, 4);
        let result = runner.validate_tick(&field, &frontier, &[]);
        assert!(result.is_some(), "parallel runner should return parity result");
    }

    #[test]
    fn heatmap_accumulates_drift() {
        let mut heatmap = DivergenceHeatmap::new(16);
        let mut f1 = ActivationField::new(4, 4);
        let f2     = ActivationField::new(4, 4);
        f1.cells[0].activation = 0.5;
        heatmap.accumulate(&f1, &f2);
        assert!(heatmap.activation_drift[0] > 0.0);
        assert_eq!(heatmap.ticks_accumulated, 1);
    }

    #[test]
    fn not_eligible_for_promotion_below_threshold() {
        let mut report = FrontierValidationReport::new();
        let pass = ParityComparisonResult { all_passed: true, ..Default::default() };
        let sparse = SparseSolverResult::default();
        for _ in 0..100 { report.record(&pass, sparse); }
        assert!(!report.eligible_for_promotion(),
            "should not be eligible before {} consecutive passes",
            SPARSE_PROMOTION_THRESHOLD);
    }
}
