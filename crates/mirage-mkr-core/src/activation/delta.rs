// ===================================================================
// mirage-mkr-core/src/activation/delta.rs  (V3 — Differential Runtime Pass)
// PURPOSE: Field Delta Tracking — Differential Runtime Foundation
//
// ---------------------------------------------------------------
// DIFFERENTIAL PRINCIPLE
// ---------------------------------------------------------------
//
// The current ActivationSolver recomputes EVERY cell every frame.
// This pass introduces the infrastructure required to transition
// to sparse, delta-driven propagation.
//
// COMPONENTS:
//   PreviousFieldSnapshot  — one-frame-behind copy of key scalars
//   FieldDeltaMask         — bit-packed vec marking changed cells
//   CellChangeFlags        — per-cell change classification
//   FieldDeltaTracker      — owns both, drives the comparison
//
// DESIGN CONSTRAINTS:
//   * No HashMap / BTreeMap — contiguous Vec only
//   * No heap allocation on hot path — pre-allocated at construction
//   * Bit-packing: one u64 covers 64 cells → 256 cells = 4 u64s
//   * Epsilon gating: float jitter below threshold is ignored
//
// TODO(V3-DIFFERENTIAL): Once FieldDeltaTracker is integrated into
// MKRWorld::tick(), replace ActivationSolver::step() with a sparse
// step that skips cells whose FieldDeltaMask bit is 0.
//
// TODO(V3-DIFFERENTIAL): Add a second delta mask (prev_delta_mask)
// to enable two-frame change detection for hysteresis-safe emission.
// ===================================================================

use super::field::ActivationField;

// =====================================================================
// EPSILON THRESHOLDS
// =====================================================================

/// Minimum change in `activation` to mark a cell as changed.
///
/// Below this, floating-point jitter is ignored and the cell is
/// treated as stable.  Value chosen as ~2× f32 machine epsilon
/// at the 0.5 midpoint of the activation range.
pub const ACTIVATION_EPSILON: f32 = 1e-4;

/// Minimum change in `execution_probability` to flag a probability shift.
pub const PROBABILITY_EPSILON: f32 = 1e-4;

/// Minimum change in `pressure` to flag a pressure shift.
pub const PRESSURE_EPSILON: f32 = 5e-2;

// =====================================================================
// CELL CHANGE FLAGS
// =====================================================================

/// Bit-field: which components of a cell changed since last frame.
///
/// Multiple flags may be set simultaneously.  Zero means the cell
/// is stable (no change above any epsilon threshold).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellChangeFlags(pub u8);

impl CellChangeFlags {
    /// Cell activation value changed beyond `ACTIVATION_EPSILON`.
    pub const ACTIVATION_CHANGED:   u8 = 0b0000_0001;
    /// Cell pressure changed beyond `PRESSURE_EPSILON`.
    pub const PRESSURE_CHANGED:     u8 = 0b0000_0010;
    /// Cell execution_probability changed beyond `PROBABILITY_EPSILON`.
    pub const PROBABILITY_CHANGED:  u8 = 0b0000_0100;
    /// Cell crossed the emission gate threshold (rising edge).
    pub const EMISSION_GATE_RISEN:  u8 = 0b0000_1000;
    /// Cell fell below the emission gate threshold (falling edge).
    pub const EMISSION_GATE_FALLEN: u8 = 0b0001_0000;

    /// True if any change flag is set.
    #[inline(always)]
    pub fn is_changed(self) -> bool { self.0 != 0 }

    /// True if the activation component changed.
    #[inline(always)]
    pub fn activation_changed(self) -> bool { self.0 & Self::ACTIVATION_CHANGED != 0 }

    /// True if the probability component changed.
    #[inline(always)]
    pub fn probability_changed(self) -> bool { self.0 & Self::PROBABILITY_CHANGED != 0 }

    /// True if the pressure component changed.
    #[inline(always)]
    pub fn pressure_changed(self) -> bool { self.0 & Self::PRESSURE_CHANGED != 0 }

    /// True if this cell newly became emission-eligible this frame.
    #[inline(always)]
    pub fn emission_gate_risen(self) -> bool { self.0 & Self::EMISSION_GATE_RISEN != 0 }

    /// True if this cell was emission-eligible last frame but is not now.
    #[inline(always)]
    pub fn emission_gate_fallen(self) -> bool { self.0 & Self::EMISSION_GATE_FALLEN != 0 }
}

// =====================================================================
// PREVIOUS FIELD SNAPSHOT
// =====================================================================

/// One-frame-behind snapshot of the activation field's key scalars.
///
/// Only the three most-useful scalars are snapshotted to minimise
/// memory.  `heat` and `entropy` are excluded — they change every
/// frame by design (decay is continuous), making them poor delta signals.
///
/// # Memory
/// 3 × f32 × N cells = 12 bytes/cell.  For a 256-cell field: 3 KB.
/// For a 15625-cell field: ~183 KB — fits comfortably in L3 cache.
#[derive(Debug)]
pub struct PreviousFieldSnapshot {
    /// Previous-frame activation values, indexed by cell.
    pub prev_activation: Vec<f32>,
    /// Previous-frame pressure values.
    pub prev_pressure: Vec<f32>,
    /// Previous-frame execution_probability values.
    pub prev_probability: Vec<f32>,
}

impl PreviousFieldSnapshot {
    /// Allocate a zero-initialised snapshot for `num_cells` cells.
    pub fn new(num_cells: usize) -> Self {
        Self {
            prev_activation: vec![0.0; num_cells],
            prev_pressure:   vec![0.0; num_cells],
            prev_probability: vec![0.0; num_cells],
        }
    }

    /// Copy the current field state into the snapshot.
    ///
    /// Call this AFTER the solver step and AFTER delta computation,
    /// so the snapshot is always one frame behind.
    pub fn capture(&mut self, field: &ActivationField) {
        debug_assert_eq!(self.prev_activation.len(), field.cells.len());
        for (i, cell) in field.cells.iter().enumerate() {
            self.prev_activation[i]  = cell.activation;
            self.prev_pressure[i]    = cell.pressure;
            self.prev_probability[i] = cell.execution_probability;
        }
    }
}

// =====================================================================
// FIELD DELTA MASK
// =====================================================================

/// Bit-packed mask: one bit per cell indicating whether that cell changed.
///
/// Uses `u64` words so that 64 cells are covered per word.
/// Checking whether ANY cell in a 64-cell block changed is a single
/// `u64 != 0` test — O(1) and SIMD-friendly.
///
/// # Indexing
/// Cell `i` lives in word `i / 64`, bit `i % 64`.
pub struct FieldDeltaMask {
    /// Packed bits: bit `i % 64` of word `i / 64` is 1 if cell `i` changed.
    words: Vec<u64>,
    /// Total number of cells covered.
    num_cells: usize,
    /// Total number of cells marked changed in this frame.
    pub changed_count: usize,
}

impl FieldDeltaMask {

    pub fn mark_changed(&mut self, index: usize) {
    let word = index / 64;

    if word >= self.words.len() {
        return;
    }

    let bit = index % 64;
    let mask = 1u64 << bit;

    if (self.words[word] & mask) == 0 {
        self.words[word] |= mask;
        self.changed_count += 1;
    }
}

        /// Mark a cell as changed.
    #[inline]
    pub fn mark(&mut self, idx: usize) {
        let word = idx / 64;
        let bit  = idx % 64;

        if word >= self.words.len() {
            return;
        }

        let mask = 1u64 << bit;

        if (self.words[word] & mask) == 0 {
            self.words[word] |= mask;
            self.changed_count += 1;
        }
    }

    /// Allocate a zeroed mask for `num_cells` cells.
    pub fn new(num_cells: usize) -> Self {
        let num_words = (num_cells + 63) / 64;
        Self {
            words: vec![0u64; num_words],
            num_cells,
            changed_count: 0,
        }
    }

    /// Clear all bits (start of frame reset).
    #[inline]
    pub fn clear(&mut self) {
        self.changed_count = 0;
        for w in &mut self.words { *w = 0; }
    }

    /// Mark cell `idx` as changed.
    #[inline(always)]
    pub fn set(&mut self, idx: usize) {
        let word = idx / 64;
        let bit  = idx % 64;
        let was_zero = self.words[word] & (1u64 << bit) == 0;
        self.words[word] |= 1u64 << bit;
        if was_zero { self.changed_count += 1; }
    }

    /// Returns true if cell `idx` changed this frame.
    #[inline(always)]
    pub fn is_changed(&self, idx: usize) -> bool {
        let word = idx / 64;
        let bit  = idx % 64;
        self.words[word] & (1u64 << bit) != 0
    }

    /// Returns true if NO cell changed this frame.
    #[inline]
    pub fn is_empty(&self) -> bool { self.changed_count == 0 }

    /// Iterate over indices of all changed cells.
    ///
    /// Uses `trailing_zeros` (BSF/TZCNT instruction) on each 64-bit word
    /// for branchless sparse iteration — O(changed_count + num_words/64).
    pub fn iter_changed(&self) -> ChangedCellIter<'_> {
        ChangedCellIter { mask: self, word_idx: 0, word: self.words.first().copied().unwrap_or(0) }
    }

    /// Fraction of cells that changed this frame (0.0 = fully sparse, 1.0 = full field).
    #[inline]
    pub fn density(&self) -> f32 {
        if self.num_cells == 0 { return 0.0; }
        self.changed_count as f32 / self.num_cells as f32
    }
}

// =====================================================================
// CHANGED CELL ITERATOR
// =====================================================================

/// Sparse iterator over changed cell indices using bit-scan.
pub struct ChangedCellIter<'a> {
    mask:     &'a FieldDeltaMask,
    word_idx: usize,
    word:     u64,
}

impl<'a> Iterator for ChangedCellIter<'a> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<usize> {
        // Advance past zero words
        while self.word == 0 {
            self.word_idx += 1;
            if self.word_idx >= self.mask.words.len() {
                return None;
            }
            self.word = self.mask.words[self.word_idx];
        }
        // Extract lowest set bit
        let bit = self.word.trailing_zeros() as usize;
        self.word &= self.word - 1; // clear lowest set bit
        let cell_idx = self.word_idx * 64 + bit;
        if cell_idx < self.mask.num_cells { Some(cell_idx) } else { None }
    }
}

// =====================================================================
// FIELD DELTA TRACKER
// =====================================================================

/// Owns the snapshot and mask; drives the per-frame delta computation.
///
/// # Integration
/// Call `compute(field)` once per tick after the solver step.
/// The resulting mask and per-cell flags are valid until the next call.
///
/// ```rust
/// // Inside MKRWorld::tick() (after activation_solver.step()):
/// self.delta_tracker.compute(&self.activation_field);
/// // ... then pass delta_tracker.mask() to downstream sparse systems
/// ```
///
/// TODO(V3-DIFFERENTIAL): Wire into MKRWorld::tick() between Phase 1
/// (solver step) and Phase 2 (emission gate).  Emission gate should
/// only scan cells in delta_tracker.mask().iter_changed().
pub struct FieldDeltaTracker {
    snapshot: PreviousFieldSnapshot,
    mask:     FieldDeltaMask,
    /// Per-cell change flags (same length as field).
    pub cell_flags: Vec<CellChangeFlags>,

    /// Emission gate threshold (copied from EMIT_GATE for self-containment).
    emit_gate: f32,
}

impl FieldDeltaTracker {
    /// Create a tracker for a field of `num_cells` cells.
    pub fn new(num_cells: usize, emit_gate: f32) -> Self {
        Self {
            snapshot:   PreviousFieldSnapshot::new(num_cells),
            mask:       FieldDeltaMask::new(num_cells),
            cell_flags: vec![CellChangeFlags::default(); num_cells],
            emit_gate,
        }
    }

    /// Compute deltas between the current field and the previous snapshot.
    ///
    /// Fills `self.mask` and `self.cell_flags`.
    /// Captures the new snapshot at the end.
    ///
    /// # Returns
    /// Reference to the freshly-computed delta mask.
    pub fn compute<'a>(&'a mut self, field: &ActivationField) -> &'a FieldDeltaMask {
        self.mask.clear();
        let n = field.cells.len().min(self.snapshot.prev_activation.len());

        for i in 0..n {
            let cell = &field.cells[i];
            let mut flags = 0u8;

            // Activation delta
            let da = (cell.activation - self.snapshot.prev_activation[i]).abs();
            if da > ACTIVATION_EPSILON {
                flags |= CellChangeFlags::ACTIVATION_CHANGED;
            }

            // Pressure delta
            let dp = (cell.pressure - self.snapshot.prev_pressure[i]).abs();
            if dp > PRESSURE_EPSILON {
                flags |= CellChangeFlags::PRESSURE_CHANGED;
            }

            // Probability delta
            let dpr = (cell.execution_probability - self.snapshot.prev_probability[i]).abs();
            if dpr > PROBABILITY_EPSILON {
                flags |= CellChangeFlags::PROBABILITY_CHANGED;
            }

            // Emission gate edge detection
            let was_eligible = self.snapshot.prev_probability[i] > self.emit_gate;
            let now_eligible  = cell.execution_probability > self.emit_gate;
            if !was_eligible && now_eligible  { flags |= CellChangeFlags::EMISSION_GATE_RISEN; }
            if  was_eligible && !now_eligible { flags |= CellChangeFlags::EMISSION_GATE_FALLEN; }

            self.cell_flags[i] = CellChangeFlags(flags);
            if flags != 0 { self.mask.set(i); }
        }

        self.snapshot.capture(field);
        &self.mask
    }

    /// Reference to the computed delta mask (valid after `compute`).
    #[inline]
    pub fn mask(&self) -> &FieldDeltaMask { &self.mask }

    /// Per-cell flags (valid after `compute`).
    #[inline]
    pub fn flags(&self) -> &[CellChangeFlags] { &self.cell_flags }

    /// Number of changed cells in the most recent frame.
    #[inline]
    pub fn changed_count(&self) -> usize { self.mask.changed_count }

    /// Fraction of field that changed (0 = fully sparse, 1 = full recompute).
    #[inline]
    pub fn change_density(&self) -> f32 { self.mask.density() }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;

    fn make_tracker(n: usize) -> FieldDeltaTracker {
        FieldDeltaTracker::new(n, 0.05)
    }

    #[test]
    fn no_changes_on_stable_field() {
        let field = ActivationField::new(4, 4);
        let mut tracker = make_tracker(16);
        // First call: snapshot is all zeros, field is all zeros → no changes
        tracker.compute(&field);
        // Second call: nothing changed
        let mask = tracker.compute(&field);
        assert!(mask.is_empty(), "stable field should produce no changes");
    }

    #[test]
    fn heat_injection_triggers_delta() {
        let mut field = ActivationField::new(4, 4);
        let mut tracker = make_tracker(16);
        tracker.compute(&field); // baseline snapshot

        field.inject_heat(0, 0.5);
        field.recompute_activation();
        field.recompute_execution_probability();

        let mask = tracker.compute(&field);
        assert!(!mask.is_empty(), "heat injection should trigger delta");
        assert!(mask.is_changed(0), "injected cell should be flagged");
    }

    #[test]
    fn delta_mask_bit_packing() {
        let mut mask = FieldDeltaMask::new(128);
        mask.set(0);
        mask.set(63);
        mask.set(64);
        mask.set(127);
        assert_eq!(mask.changed_count, 4);
        assert!(mask.is_changed(0));
        assert!(mask.is_changed(63));
        assert!(mask.is_changed(64));
        assert!(mask.is_changed(127));
        assert!(!mask.is_changed(1));
    }

    #[test]
    fn changed_cell_iterator_correct() {
        let mut mask = FieldDeltaMask::new(256);
        mask.set(5);
        mask.set(70);
        mask.set(200);
        let changed: Vec<usize> = mask.iter_changed().collect();
        assert_eq!(changed, vec![5, 70, 200]);
    }

    #[test]
    fn emission_gate_edge_detection() {
        let mut field = ActivationField::new(4, 4);
        let mut tracker = make_tracker(16);
        tracker.compute(&field);

        // Push cell 0 above emission gate
        field.cells[0].execution_probability = 0.10; // above 0.05
        tracker.compute(&field);
        assert!(tracker.cell_flags[0].emission_gate_risen(),
            "cell should be RISEN when crossing above gate");

        // Push it back below
        field.cells[0].execution_probability = 0.01;
        tracker.compute(&field);
        assert!(tracker.cell_flags[0].emission_gate_fallen(),
            "cell should be FALLEN when crossing below gate");
    }

    #[test]
    fn density_computation() {
        let mut mask = FieldDeltaMask::new(100);
        for i in 0..25 { mask.set(i); }
        let d = mask.density();
        assert!((d - 0.25).abs() < 1e-5, "density should be 0.25: {}", d);
    }
}
