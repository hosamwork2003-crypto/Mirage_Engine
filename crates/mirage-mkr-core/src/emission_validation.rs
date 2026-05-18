// ===================================================================
// mirage-mkr-core/src/emission_validation.rs
// PURPOSE: Differential Emission Shadow Validation — Pass 02
//
// AUTHORITY:
//   emission_gate.collect()              — AUTHORITATIVE (unchanged)
//   shadow_gate.collect_from_changed()   — SHADOW (validation only)
//
// This module provides:
//   1. EmissionParityReport     — per-tick comparison of both paths
//   2. DifferentialEmissionValidationReport — accumulated statistics
//   3. DifferentialEmissionMode — mode control (ShadowValidation only now)
//   4. EmissionShadowValidator  — owns shadow gate + runs validation
//
// CORRECTNESS REQUIREMENT — ORDERING:
//   Both paths apply budget enforcement via select_nth_unstable_by()
//   (partial descending probability sort + truncate).
//   select_nth_unstable_by does NOT guarantee stable ordering within
//   equal-probability groups.  For parity comparison we sort BOTH outputs
//   by (cell_index ASC) before comparing identity.  Ordering validation
//   compares probability-descending order after sorting by probability.
//
// DIVERGENCE SOURCES:
//   1. collect() scans in cell_index order (0..N). collect_from_changed()
//      scans delta_mask cells first, then still_eligible bits. These two
//      traversal orders produce identical SETS of cells but may differ in
//      internal scratch order before budget truncation.
//   2. Budget truncation: both use select_nth_unstable_by with the same
//      comparator. The partition is correct but the ordering within the
//      kept/discarded halves is unspecified. We compare SETS, not order.
//   3. still_eligible drift: if the shadow gate's bitset diverges from
//      the authoritative gate's bitset, carryover eligibility diverges.
//      Tracked separately in `stale_eligible_mismatches`.
//
// NEXT PASS (Pass 03) PREPARATION:
//   Phase 3 renderer parity is the natural successor.
//   EmissionShadowValidator.last_report and validation_report are
//   designed to be read by Pass 03 infrastructure.
//   The DifferentialEmissionMode::DifferentialAuthoritative arm is
//   defined but NOT enabled — the gate is already in place for Pass 03.
// ===================================================================

use crate::emission::{EmissionGate, EmissionRequest, EMIT_GATE, MAX_EMIT_PER_TICK};
use crate::activation::field::ActivationField;
use crate::activation::delta::FieldDeltaMask;

// =====================================================================
// MODE CONTROL
// =====================================================================

/// Controls differential emission execution mode.
///
/// Current pass: `ShadowValidation` only.
/// `DifferentialAuthoritative` is defined for Pass 03 but must NOT be
/// enabled until `ShadowValidation` achieves PASS_PROMOTION_THRESHOLD
/// consecutive clean ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DifferentialEmissionMode {
    /// Shadow validation disabled.  Only authoritative collect() runs.
    #[default]
    Disabled,
    /// CURRENT PASS: shadow collect_from_changed() runs alongside collect().
    /// Authoritative output is ALWAYS from collect().
    ShadowValidation,
    /// NOT YET ENABLED.  Reserved for Pass 03 authority promotion.
    /// Enabling this without 1000 consecutive clean validation ticks
    /// is a protocol violation.
    DifferentialAuthoritative,
}

/// Minimum consecutive passing ticks before DifferentialAuthoritative
/// may even be considered.  NOT automatically applied.
pub const PASS_PROMOTION_THRESHOLD: u64 = 1_000;

// =====================================================================
// PER-TICK PARITY REPORT
// =====================================================================

/// Comparison of one tick's authoritative and differential emission outputs.
///
/// Cells are compared by identity (cell_index set membership), not order.
/// Ordering is validated separately via `order_mismatches`.
#[derive(Debug, Clone, Default)]
pub struct EmissionParityReport {
    /// Number of requests from authoritative collect().
    pub authoritative_count: usize,
    /// Number of requests from shadow collect_from_changed().
    pub differential_count:  usize,

    /// Cell indices present in authoritative but absent in differential.
    pub missing_from_differential: usize,
    /// Cell indices present in differential but absent in authoritative.
    pub extra_in_differential:     usize,

    /// Pairs that both paths emitted but at different probability rank positions
    /// (after sorting each output by descending probability then cell_index).
    pub order_mismatches: usize,

    /// True iff missing == 0, extra == 0, and order_mismatches == 0.
    pub all_passed: bool,
}

impl EmissionParityReport {
    /// Compare two emission slices.
    ///
    /// `auth` is the authoritative output (collect()).
    /// `diff` is the shadow output (collect_from_changed()).
    ///
    /// Both slices are sorted in-place to canonical order
    /// (descending probability, then ascending cell_index) for comparison.
    /// The caller should pass owned copies or slices already safe to sort.
    pub fn compare(auth: &[EmissionRequest], diff: &[EmissionRequest]) -> Self {
        // Build sorted cell-index sets for identity comparison.
        // Using a fixed-size inline sort to avoid allocation on the hot path.
        // MAX_EMIT_PER_TICK is 128 — small enough for O(N²) insertion sort.
        let mut auth_indices = [usize::MAX; MAX_EMIT_PER_TICK];
        let mut diff_indices = [usize::MAX; MAX_EMIT_PER_TICK];

        let auth_len = auth.len().min(MAX_EMIT_PER_TICK);
        let diff_len = diff.len().min(MAX_EMIT_PER_TICK);

        for i in 0..auth_len { auth_indices[i] = auth[i].cell_index; }
        for i in 0..diff_len { diff_indices[i] = diff[i].cell_index; }

        // Sort both index arrays (ascending cell_index)
        auth_indices[..auth_len].sort_unstable();
        diff_indices[..diff_len].sort_unstable();

        // Merge-count: missing and extra
        let mut ai = 0usize;
        let mut di = 0usize;
        let mut missing = 0usize;
        let mut extra   = 0usize;

        while ai < auth_len && di < diff_len {
            match auth_indices[ai].cmp(&diff_indices[di]) {
                std::cmp::Ordering::Equal => { ai += 1; di += 1; }
                std::cmp::Ordering::Less  => { missing += 1; ai += 1; }
                std::cmp::Ordering::Greater => { extra   += 1; di += 1; }
            }
        }
        missing += auth_len - ai;
        extra   += diff_len - di;

        // Order mismatch: compare probability-rank order for the common set.
        // Sort auth and diff by (descending prob, ascending cell_idx) and compare positions.
        let mut auth_ranked: [(f32, usize); MAX_EMIT_PER_TICK] = [(0.0, 0); MAX_EMIT_PER_TICK];
        let mut diff_ranked: [(f32, usize); MAX_EMIT_PER_TICK] = [(0.0, 0); MAX_EMIT_PER_TICK];
        for i in 0..auth_len { auth_ranked[i] = (auth[i].probability, auth[i].cell_index); }
        for i in 0..diff_len { diff_ranked[i] = (diff[i].probability, diff[i].cell_index); }
        auth_ranked[..auth_len].sort_unstable_by(|(pa, ia), (pb, ib)|
            pb.partial_cmp(pa).unwrap_or(std::cmp::Ordering::Equal)
                .then(ia.cmp(ib)));
        diff_ranked[..diff_len].sort_unstable_by(|(pa, ia), (pb, ib)|
            pb.partial_cmp(pa).unwrap_or(std::cmp::Ordering::Equal)
                .then(ia.cmp(ib)));

        // Count positions where the ranked cell_index differs
        let common = auth_len.min(diff_len);
        let mut order_mismatches = 0usize;
        for i in 0..common {
            if auth_ranked[i].1 != diff_ranked[i].1 { order_mismatches += 1; }
        }

        let all_passed = missing == 0 && extra == 0 && order_mismatches == 0;

        Self {
            authoritative_count: auth_len,
            differential_count:  diff_len,
            missing_from_differential: missing,
            extra_in_differential:     extra,
            order_mismatches,
            all_passed,
        }
    }
}

// =====================================================================
// ACCUMULATED VALIDATION REPORT
// =====================================================================

/// Accumulated differential emission validation statistics across all ticks.
#[derive(Debug, Clone, Default)]
pub struct DifferentialEmissionValidationReport {
    /// Total ticks where shadow emission was evaluated.
    pub ticks_run:  u64,
    /// Ticks where authoritative and differential outputs matched exactly.
    pub ticks_passed: u64,
    /// Ticks where any mismatch was detected.
    pub ticks_failed: u64,
    /// Consecutive pass count — resets to 0 on any failure.
    pub consecutive_passes: u64,

    /// Peak `missing_from_differential` seen across all ticks.
    pub peak_missing: usize,
    /// Peak `extra_in_differential` seen across all ticks.
    pub peak_extra:   usize,
    /// Peak `order_mismatches` seen across all ticks.
    pub peak_order_mismatch: usize,

    /// Total missing-cell events summed across all ticks.
    pub total_missing_events: u64,
    /// Total extra-cell events summed across all ticks.
    pub total_extra_events:   u64,
}

impl DifferentialEmissionValidationReport {
    pub fn new() -> Self { Self::default() }

    /// Record one tick's parity result.
    pub fn record(&mut self, report: &EmissionParityReport) {
        self.ticks_run += 1;
        if report.all_passed {
            self.ticks_passed += 1;
            self.consecutive_passes += 1;
        } else {
            self.ticks_failed += 1;
            self.consecutive_passes = 0;
        }
        if report.missing_from_differential > self.peak_missing {
            self.peak_missing = report.missing_from_differential;
        }
        if report.extra_in_differential > self.peak_extra {
            self.peak_extra = report.extra_in_differential;
        }
        if report.order_mismatches > self.peak_order_mismatch {
            self.peak_order_mismatch = report.order_mismatches;
        }
        self.total_missing_events += report.missing_from_differential as u64;
        self.total_extra_events   += report.extra_in_differential as u64;
    }

    /// Pass rate [0.0, 1.0].
    pub fn pass_rate(&self) -> f32 {
        if self.ticks_run == 0 { return 1.0; }
        self.ticks_passed as f32 / self.ticks_run as f32
    }

    /// True if consecutive passes have reached the promotion threshold.
    /// Does NOT automatically enable DifferentialAuthoritative.
    pub fn eligible_for_promotion(&self) -> bool {
        self.consecutive_passes >= PASS_PROMOTION_THRESHOLD
            && self.ticks_failed == 0
    }

    /// Reset all accumulated state.
    pub fn reset(&mut self) { *self = Self::default(); }
}

// =====================================================================
// SHADOW EMISSION VALIDATOR
// =====================================================================

/// Owns the shadow EmissionGate and runs differential emission validation.
///
/// # Design
/// The shadow gate is a SEPARATE EmissionGate instance from the authoritative
/// gate.  It maintains its own `scratch` and `still_eligible` bitsets so that
/// both paths can evolve independently and be compared each tick.
///
/// # Authority
/// The authoritative gate (`MKRWorld::emission_gate`) always wins.
/// `last_emission` is always from `collect()`.
/// The shadow result is stored in `last_shadow_emission` for inspection only.
///
/// # Enabling
/// Call `enable_shadow()`.  Default is `Disabled`.
/// Check `validation_report.eligible_for_promotion()` for promotion readiness.
///
/// # Pass 03 Preparation
/// `last_report` is pub so the Pass 03 renderer validator can read emission
/// parity as a pre-condition for renderer differential validation.
pub struct EmissionShadowValidator {
    /// Shadow emission gate — separate from authoritative gate.
    shadow_gate:           EmissionGate,
    /// Last shadow emission output (cell indices + probabilities).
    pub last_shadow_emission: Vec<EmissionRequest>,
    /// Per-tick parity comparison result.
    pub last_report:       Option<EmissionParityReport>,
    /// Accumulated statistics.
    pub validation_report: DifferentialEmissionValidationReport,
    /// Current mode.
    pub mode:              DifferentialEmissionMode,
}

impl EmissionShadowValidator {
    pub fn new() -> Self {
        Self {
            shadow_gate:           EmissionGate::new(),
            last_shadow_emission:  Vec::new(),
            last_report:           None,
            validation_report:     DifferentialEmissionValidationReport::new(),
            mode:                  DifferentialEmissionMode::Disabled,
        }
    }

    /// Enable shadow validation mode.
    pub fn enable_shadow(&mut self) {
        self.mode = DifferentialEmissionMode::ShadowValidation;
    }

    /// Disable validation (zero overhead).
    pub fn disable(&mut self) {
        self.mode = DifferentialEmissionMode::Disabled;
    }

    /// True if shadow validation is currently active.
    #[inline]
    pub fn is_active(&self) -> bool {
        matches!(self.mode, DifferentialEmissionMode::ShadowValidation)
    }

    /// Run shadow emission and compare against the authoritative result.
    ///
    /// # Parameters
    /// * `field`          — activation field (same as authoritative path)
    /// * `delta_mask`     — field delta mask from delta_tracker.compute()
    /// * `authoritative`  — slice produced by authoritative collect() this tick
    ///
    /// # Returns
    /// `Some(EmissionParityReport)` when active, `None` when disabled.
    ///
    /// # Authority
    /// Returns nothing that affects the live emission output.
    /// `last_shadow_emission` is for inspection / diagnostics only.
    pub fn validate_tick(
        &mut self,
        field:         &ActivationField,
        delta_mask:    &FieldDeltaMask,
        authoritative: &[EmissionRequest],
    ) -> Option<EmissionParityReport> {
        if !self.is_active() { return None; }

        // Run shadow collect_from_changed() on the shadow gate.
        let shadow = self.shadow_gate.collect_from_changed(field, delta_mask);

        // Snapshot shadow output before the borrow expires.
        self.last_shadow_emission.clear();
        self.last_shadow_emission.extend_from_slice(shadow);

        // Compare.
        let report = EmissionParityReport::compare(authoritative, &self.last_shadow_emission);
        self.validation_report.record(&report);
        self.last_report = Some(report.clone());
        Some(report)
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;
    use crate::activation::delta::FieldDeltaMask;
    use crate::emission::EmissionGate;

    // ------------------------------------------------------------------
    // Helper: build a delta mask from prob snapshot vs current field
    // ------------------------------------------------------------------
    fn snap_probs(field: &ActivationField) -> Vec<f32> {
        field.cells.iter().map(|c| c.execution_probability).collect()
    }

    fn delta_from_snap(
        prev_probs: &[f32],
        after:      &ActivationField,
        epsilon:    f32,
    ) -> FieldDeltaMask {
        let n = after.cells.len();
        let mut mask = FieldDeltaMask::new(n);
        for i in 0..n {
            let prev = if i < prev_probs.len() { prev_probs[i] } else { 0.0 };
            let curr = after.cells[i].execution_probability;
            if (curr - prev).abs() > epsilon { mask.set(i); }
        }
        mask
    }

    // Convenience: delta from a zero-baseline (field newly set)
    fn delta_from_zero(after: &ActivationField, epsilon: f32) -> FieldDeltaMask {
        delta_from_snap(&vec![0.0f32; after.cells.len()], after, epsilon)
    }

    // ------------------------------------------------------------------
    // Test 1: cells newly crossing EMIT_GATE appear in both paths
    // ------------------------------------------------------------------
    #[test]
    fn newly_crossing_gate_appears_in_both() {
        let mut field = ActivationField::new(4, 4);
        // Cell 3 rises above gate this tick
        field.cells[3].execution_probability = EMIT_GATE + 0.1;

        let mask = delta_from_zero(&field, 0.001);

        let mut auth_gate   = EmissionGate::new();
        let mut shadow_val  = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth = auth_gate.collect(&field);
        let report = shadow_val.validate_tick(&field, &mask, auth).unwrap();

        assert!(report.all_passed,
            "newly-crossing cell must appear in both paths: {:?}", report);
        assert_eq!(report.authoritative_count, 1);
        assert_eq!(report.differential_count, 1);
    }

    // ------------------------------------------------------------------
    // Test 2: cells remaining above gate without changing (still_eligible)
    // ------------------------------------------------------------------
    #[test]
    fn still_eligible_cells_persist_in_differential() {
        let mut field = ActivationField::new(4, 4);
        // Cell 0 is hot and doesn't change between ticks
        field.cells[0].execution_probability = 0.9;

        // Tick 1: cell 0 newly hot → appears in delta
        let mask1 = delta_from_zero(&field, 0.001);

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth1 = auth_gate.collect(&field);
        let r1 = shadow_val.validate_tick(&field, &mask1, auth1).unwrap();
        assert!(r1.all_passed, "tick 1 must pass: {:?}", r1);

        // Tick 2: field identical — cell 0 NOT in delta but still eligible
        let empty_mask = FieldDeltaMask::new(16); // no bits set
        let auth2 = auth_gate.collect(&field); // still emits cell 0
        let r2 = shadow_val.validate_tick(&field, &empty_mask, auth2).unwrap();
        assert!(r2.all_passed,
            "still_eligible must carry cell 0 into differential tick 2: {:?}", r2);
    }

    // ------------------------------------------------------------------
    // Test 3: cells dropping below gate are cleared
    // ------------------------------------------------------------------
    #[test]
    fn cells_dropping_below_gate_are_cleared() {
        let mut field = ActivationField::new(4, 4);
        field.cells[2].execution_probability = 0.8;

        // Tick 1: cell 2 hot
        let mask1 = delta_from_zero(&field, 0.001);
        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();
        let a1 = auth_gate.collect(&field);
        shadow_val.validate_tick(&field, &mask1, a1);

        // Tick 2: cell 2 drops below gate
        let prev_probs = snap_probs(&field);
        field.cells[2].execution_probability = 0.001;
        let mask2 = delta_from_snap(&prev_probs, &field, 0.001);
        let a2 = auth_gate.collect(&field);
        let r2 = shadow_val.validate_tick(&field, &mask2, a2).unwrap();
        assert!(r2.all_passed,
            "cell dropping below gate must be absent in both: {:?}", r2);
        assert_eq!(r2.authoritative_count, 0, "no cells above gate");
        assert_eq!(r2.differential_count, 0);
    }

    // ------------------------------------------------------------------
    // Test 4: oscillating threshold cells
    // ------------------------------------------------------------------
    #[test]
    fn oscillating_threshold_cells() {
        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let high = EMIT_GATE + 0.05;
        let low  = EMIT_GATE - 0.01;

        let mut field     = ActivationField::new(4, 4);
        let mut prev_probs = snap_probs(&field); // all zero initially

        for tick in 0..10 {
            // Alternate cell 7 above/below gate
            let prob = if tick % 2 == 0 { high } else { low };
            field.cells[7].execution_probability = prob;
            let mask = delta_from_snap(&prev_probs, &field, 0.001);
            let auth = auth_gate.collect(&field);
            let r = shadow_val.validate_tick(&field, &mask, auth).unwrap();
            assert!(r.all_passed, "tick {tick} oscillation: {:?}", r);
            prev_probs = snap_probs(&field);
        }
    }

    // ------------------------------------------------------------------
    // Test 5: budget truncation parity
    // ------------------------------------------------------------------
    #[test]
    fn budget_truncation_parity() {
        // 256 cells all above gate — budget (128) must be enforced identically
        let mut field = ActivationField::new(16, 16);
        for cell in &mut field.cells { cell.execution_probability = 0.9; }

        let mask = delta_from_zero(&field, 0.001);

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth = auth_gate.collect(&field);
        let r = shadow_val.validate_tick(&field, &mask, auth).unwrap();

        assert_eq!(r.authoritative_count, MAX_EMIT_PER_TICK,
            "authoritative must hit budget");
        assert_eq!(r.differential_count, MAX_EMIT_PER_TICK,
            "differential must hit same budget");
        // Missing/extra may differ because select_nth_unstable_by has
        // unspecified ordering within equal-probability groups.
        // The SET of emitted cells may differ but counts must match.
        assert_eq!(r.authoritative_count, r.differential_count,
            "both paths must emit identical count");
    }

    // ------------------------------------------------------------------
    // Test 6: deterministic ordering parity (unique probabilities)
    // ------------------------------------------------------------------
    #[test]
    fn deterministic_ordering_parity_unique_probs() {
        let mut field = ActivationField::new(4, 4);
        // Assign unique probabilities > EMIT_GATE so ordering is deterministic
        for (i, cell) in field.cells.iter_mut().enumerate() {
            cell.execution_probability = EMIT_GATE + 0.01 * (i as f32 + 1.0);
        }

        let mask = delta_from_zero(&field, 0.001);

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth = auth_gate.collect(&field);
        let r = shadow_val.validate_tick(&field, &mask, auth).unwrap();

        assert!(r.all_passed,
            "unique probabilities must produce identical ranked order: {:?}", r);
        assert_eq!(r.order_mismatches, 0);
    }

    // ------------------------------------------------------------------
    // Test 7: zero changed cells with no still_eligible — both output empty
    // ------------------------------------------------------------------
    #[test]
    fn zero_changed_cells_no_eligible() {
        let field     = ActivationField::new(4, 4); // all prob = 0
        let empty_mask = FieldDeltaMask::new(16);

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth = auth_gate.collect(&field);
        assert_eq!(auth.len(), 0);
        let r = shadow_val.validate_tick(&field, &empty_mask, auth).unwrap();
        assert!(r.all_passed, "empty field, empty mask: {:?}", r);
        assert_eq!(r.authoritative_count, 0);
        assert_eq!(r.differential_count, 0);
    }

    // ------------------------------------------------------------------
    // Test 8: large sparse workload — only a few hot cells in a large field
    // ------------------------------------------------------------------
    #[test]
    fn large_sparse_workload_parity() {
        let mut field = ActivationField::new(16, 16); // 256 cells
        // Only 5 hot cells
        let hot_cells = [10, 50, 100, 150, 200];
        for &i in &hot_cells {
            field.cells[i].execution_probability = 0.9;
        }

        let mask = delta_from_zero(&field, 0.001);

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth = auth_gate.collect(&field);
        let r = shadow_val.validate_tick(&field, &mask, auth).unwrap();
        assert!(r.all_passed, "sparse large workload: {:?}", r);
        assert_eq!(r.authoritative_count, 5);
        assert_eq!(r.differential_count, 5);
    }

    // ------------------------------------------------------------------
    // Test 9: long-running eligibility persistence (50 ticks)
    // ------------------------------------------------------------------
    #[test]
    fn long_running_eligibility_persistence() {
        let mut field = ActivationField::new(4, 4);
        field.cells[1].execution_probability = 0.8;

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        // First tick: cell 1 appears in delta
        let mask1 = delta_from_zero(&field, 0.001);
        let a1 = auth_gate.collect(&field);
        shadow_val.validate_tick(&field, &mask1, a1);

        // Ticks 2..50: field unchanged — cell 1 stays eligible via still_eligible
        let empty_mask = FieldDeltaMask::new(16);
        for tick in 2..=50 {
            let auth = auth_gate.collect(&field);
            let r = shadow_val.validate_tick(&field, &empty_mask, auth).unwrap();
            assert!(r.all_passed,
                "tick {tick}: still_eligible must persist cell 1: {:?}", r);
        }

        assert_eq!(shadow_val.validation_report.ticks_passed, 50);
        assert_eq!(shadow_val.validation_report.ticks_failed, 0);
        assert_eq!(shadow_val.validation_report.consecutive_passes, 50);
    }

    // ------------------------------------------------------------------
    // Test 10: validation report accumulation and pass_rate
    // ------------------------------------------------------------------
    #[test]
    fn validation_report_accumulates() {
        let mut report = DifferentialEmissionValidationReport::new();
        let pass = EmissionParityReport { all_passed: true, ..Default::default() };
        let fail = EmissionParityReport {
            all_passed: false,
            missing_from_differential: 2,
            ..Default::default()
        };
        for _ in 0..10 { report.record(&pass); }
        assert_eq!(report.consecutive_passes, 10);
        report.record(&fail);
        assert_eq!(report.consecutive_passes, 0);
        assert_eq!(report.ticks_failed, 1);
        assert_eq!(report.peak_missing, 2);
        assert!((report.pass_rate() - 10.0 / 11.0).abs() < 1e-5);
    }

    // ------------------------------------------------------------------
    // Test 11: disabled validator returns None
    // ------------------------------------------------------------------
    #[test]
    fn disabled_validator_returns_none() {
        let field      = ActivationField::new(4, 4);
        let empty_mask = FieldDeltaMask::new(16);
        let mut validator = EmissionShadowValidator::new();
        // mode is Disabled by default
        let result = validator.validate_tick(&field, &empty_mask, &[]);
        assert!(result.is_none(), "disabled validator must return None");
    }
}
