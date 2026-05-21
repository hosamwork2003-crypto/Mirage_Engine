// ===================================================================
// mirage-mkr-core/src/activation/frontier.rs  (V3 — Differential Runtime Pass)
// PURPOSE: Sparse Propagation Frontier — Changed-Region Seeds
//
// ---------------------------------------------------------------
// SPARSE PROPAGATION PRINCIPLE
// ---------------------------------------------------------------
//
// Current: ActivationSolver::propagate_pressure() iterates ALL cells.
// Target:  Only cells whose neighbours changed need pressure re-eval.
//
// A "frontier" is the set of cells that MUST be re-evaluated because
// either they changed or their neighbours changed.
//
// FRONTIER CONSTRUCTION:
//   1. Start from FieldDeltaMask changed cells (seeds).
//   2. Expand by one grid step (4-neighbour) → frontier.
//   3. Only frontier cells participate in the next propagation pass.
//
// This reduces propagation cost from O(N) to O(|frontier|) per tick.
// For a sparse field (few active regions), |frontier| << N.
//
// COEXISTENCE CONTRACT:
//   The frontier DOES NOT replace the full solver pass yet.
//   It runs alongside it and validates sparse correctness.
//
// TODO(V3-DIFFERENTIAL): Once frontier validation passes 1000 ticks
// without divergence from the full solver, enable DIFFERENTIAL_MODE
// which skips the full solver and only runs frontier propagation.
//
// TODO(V3-DIFFERENTIAL): Frontier expansion must be bounded.
// If density() > FRONTIER_FULL_FALLBACK_THRESHOLD, fall back to
// full-field propagation (no benefit from sparse at that point).
// ===================================================================

use super::delta::FieldDeltaMask;

// =====================================================================
// CONSTANTS
// =====================================================================

/// If the frontier covers more than this fraction of the field,
/// fall back to full propagation (sparse has no benefit).
pub const FRONTIER_FULL_FALLBACK_THRESHOLD: f32 = 0.40;

/// Maximum cells in the frontier before triggering full-field fallback.
/// Prevents degenerate O(N²) frontier expansion.
pub const FRONTIER_MAX_CELLS: usize = 4096;

// =====================================================================
// PROPAGATION FRONTIER
// =====================================================================

/// Sparse propagation frontier: the set of cells that require
/// re-evaluation in the current tick.
///
/// # Memory Layout
/// Two flat bit-masks (current frontier, scratch for expansion) plus
/// a compact index list for iteration.  All pre-allocated.
///
/// # Usage
/// ```rust
/// // After delta tracker runs:
/// frontier.build_from_delta(delta_tracker.mask(), field_width, field_height);
/// if frontier.should_use_sparse() {
///     // Run sparse propagation on frontier.iter_cells() only
/// } else {
///     // Fall back to full propagation
/// }
/// ```
#[derive(Debug, Clone)]
pub struct PropagationFrontier {
    /// Bit-mask: one bit per cell, 1 = cell is in the frontier.
    bits: Vec<u64>,
    /// Compact index list of frontier cells (for O(|frontier|) iteration).
    cells: Vec<usize>,
    /// Total field cell count.
    num_cells: usize,
    /// Field width (for neighbour computation).
    field_width: usize,
    /// Field height.
    field_height: usize,
}

impl PropagationFrontier {
    /// Allocate a frontier for a `width × height` field.
    pub fn new(width: usize, height: usize) -> Self {
        let num_cells = width * height;
        let num_words = (num_cells + 63) / 64;
        Self {
            bits: vec![0u64; num_words],
            cells: Vec::with_capacity(FRONTIER_MAX_CELLS),
            num_cells,
            field_width: width,
            field_height: height,
        }
    }

    /// Clear the frontier.
    #[inline]
    fn clear(&mut self) {
        self.cells.clear();
        for w in &mut self.bits { *w = 0; }
    }

    /// Set bit for cell `idx` and record it in the compact list.
    #[inline]
    fn set_cell(&mut self, idx: usize) {
        if idx >= self.num_cells { return; }
        let word = idx / 64;
        let bit  = idx % 64;
        if self.bits[word] & (1u64 << bit) == 0 {
            self.bits[word] |= 1u64 << bit;
            if self.cells.len() < FRONTIER_MAX_CELLS {
                self.cells.push(idx);
            }
        }
    }

    /// Build the frontier from a delta mask, expanding by one 4-neighbour step.
    ///
    /// The frontier includes every changed cell AND its immediate neighbours,
    /// because neighbours of changed cells must re-evaluate their pressure.
    ///
    /// Returns `true` if the frontier is usable (sparse).
    /// Returns `false` if a full-field fallback is recommended.
    pub fn build_from_delta(
        &mut self,
        delta:  &FieldDeltaMask,
        width:  usize,
        height: usize,
    ) -> bool {
        self.clear();
        self.field_width  = width;
        self.field_height = height;

        // Seed phase: include all changed cells
        for idx in delta.iter_changed() {
            let x = idx % width;
            let y = idx / width;

            // Include self
            self.set_cell(idx);

            // Include 4-neighbours (Neumann, boundary-clamped)
            if y > 0           { self.set_cell((y - 1) * width + x); }
            if y + 1 < height  { self.set_cell((y + 1) * width + x); }
            if x > 0           { self.set_cell(y * width + (x - 1)); }
            if x + 1 < width   { self.set_cell(y * width + (x + 1)); }
        }

        // If frontier is too large, recommend full fallback
        !self.should_fallback_to_full()
    }

    /// True if the frontier is small enough to benefit from sparse propagation.
    #[inline]
    pub fn should_use_sparse(&self) -> bool { !self.should_fallback_to_full() }

    #[inline]
    fn should_fallback_to_full(&self) -> bool {
        if self.num_cells == 0 { return false; }
        self.cells.len() >= FRONTIER_MAX_CELLS
            || (self.cells.len() as f32 / self.num_cells as f32)
                > FRONTIER_FULL_FALLBACK_THRESHOLD
    }

    /// Iterate over cell indices in the frontier.
    ///
    /// Used by sparse propagation passes to skip non-frontier cells.
    #[inline]
    pub fn iter_cells(&self) -> std::slice::Iter<'_, usize> {
        self.cells.iter()
    }

    /// Number of cells in the frontier.
    #[inline]
    pub fn frontier_size(&self) -> usize { self.cells.len() }

    /// Frontier density: frontier_size / total_cells.
    #[inline]
    pub fn density(&self) -> f32 {
        if self.num_cells == 0 { return 0.0; }
        self.cells.len() as f32 / self.num_cells as f32
    }

    /// True if the frontier is empty (no propagation needed this tick).
    #[inline]
    pub fn is_empty(&self) -> bool { self.cells.is_empty() }

    /// Check if a cell index is present in the frontier.
    #[inline]
    pub fn contains(&self, idx: usize) -> bool {
        if idx >= self.num_cells { return false; }
        let word = idx / 64;
        let bit  = idx % 64;
        (self.bits[word] & (1u64 << bit)) != 0
    }
}

// =====================================================================
// FRONTIER STATISTICS
// =====================================================================

/// Diagnostic statistics from the propagation frontier.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrontierStats {
    /// Number of cells in the frontier.
    pub frontier_cells: usize,
    /// Total field cells.
    pub total_cells: usize,
    /// Whether sparse mode was used (vs full-field fallback).
    pub used_sparse: bool,
    /// Frontier density [0, 1].
    pub density: f32,
}

impl FrontierStats {
    pub fn from_frontier(frontier: &PropagationFrontier, used_sparse: bool) -> Self {
        Self {
            frontier_cells: frontier.frontier_size(),
            total_cells:    frontier.num_cells,
            used_sparse,
            density:        frontier.density(),
        }
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::delta::FieldDeltaMask;

    fn make_delta(num_cells: usize, changed: &[usize]) -> FieldDeltaMask {
        let mut m = FieldDeltaMask::new(num_cells);
        for &c in changed { m.set(c); }
        m
    }

    #[test]
    fn empty_delta_produces_empty_frontier() {
        let delta = make_delta(16, &[]);
        let mut frontier = PropagationFrontier::new(4, 4);
        frontier.build_from_delta(&delta, 4, 4);
        assert!(frontier.is_empty());
    }

    #[test]
    fn single_changed_cell_expands_to_neighbours() {
        // Cell 5 in a 4×4 grid: coords (1, 1)
        // Neighbours: (0,1)=4, (2,1)=6, (1,0)=1, (1,2)=9
        let delta = make_delta(16, &[5]);
        let mut frontier = PropagationFrontier::new(4, 4);
        frontier.build_from_delta(&delta, 4, 4);

        let cells: Vec<usize> = frontier.iter_cells().copied().collect();
        assert!(cells.contains(&5), "changed cell must be in frontier");
        assert!(cells.contains(&4), "west neighbour must be in frontier");
        assert!(cells.contains(&6), "east neighbour must be in frontier");
        assert!(cells.contains(&1), "north neighbour must be in frontier");
        assert!(cells.contains(&9), "south neighbour must be in frontier");
        assert_eq!(cells.len(), 5, "corner cell has 4 neighbours + self");
    }

    #[test]
    fn corner_cell_clamps_boundary() {
        // Cell 0 in 4×4: top-left corner — only 2 neighbours
        let delta = make_delta(16, &[0]);
        let mut frontier = PropagationFrontier::new(4, 4);
        frontier.build_from_delta(&delta, 4, 4);
        let cells: Vec<usize> = frontier.iter_cells().copied().collect();
        assert_eq!(cells.len(), 3, "corner cell has 2 neighbours + self");
    }

    #[test]
    fn frontier_density_calculation() {
        // All 16 cells changed in 4×4 grid — frontier should cover all
        let delta = make_delta(16, &(0..16).collect::<Vec<_>>());
        let mut frontier = PropagationFrontier::new(4, 4);
        frontier.build_from_delta(&delta, 4, 4);
        let d = frontier.density();
        assert!(d > 0.99, "full delta should produce full frontier, got {}", d);
    }

    #[test]
    fn sparse_mode_recommended_for_small_frontier() {
        let delta = make_delta(256, &[100]); // 1 cell changed in 256
        let mut frontier = PropagationFrontier::new(16, 16);
        let is_sparse = frontier.build_from_delta(&delta, 16, 16);
        assert!(is_sparse, "tiny frontier should recommend sparse mode");
    }
}
