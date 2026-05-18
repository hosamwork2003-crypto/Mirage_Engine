// ===================================================================
// mirage-mkr-core/src/activation/field.rs
// PURPOSE: ActivationCell + ActivationField — Core Field Primitives
//
// DESIGN PHILOSOPHY:
// The activation field is a continuous scalar field over the chunk
// space. Every cell stores five f32 values that together describe
// the instantaneous "activation pressure" of that region.
//
// There are NO enum states here.  Values are continuous.
// The only gating is at the fiber-emission boundary (future work).
//
// MEMORY LAYOUT:
// ActivationCell is #[repr(C)], 5 × f32 = 20 bytes.
// Cells are stored in a dense Vec<ActivationCell> — linear, L1-friendly.
// Width × Height determines grid shape; flat index = y * width + x.
//
// SIMD NOTES:
// Each ActivationCell is 20 bytes — pack four cells into 80 bytes (5
// __m256 lanes of 8 f32 each covers 40 cells in a single pass).
// The solver works on contiguous slices, so auto-vectorization fires.
//
// TODO(V3-CEK): inject_heat() and inject_pressure() will eventually
// receive their source values from CEK field outputs, not from ad-hoc
// callers.  Keep the signatures stable.
// ===================================================================

/// A single activation cell in the execution field.
///
/// Five continuous scalars describe the full activation state of a
/// chunk position.  No enum arms.  No discrete transitions.
///
/// # Field Semantics
///
/// | Field                    | Range   | Meaning                                      |
/// |--------------------------|---------|----------------------------------------------|
/// | `heat`                   | 0 – 1   | Accumulated thermal energy                   |
/// | `pressure`               | 0 – 1   | Execution demand from neighbours / topology  |
/// | `entropy`                | 0 – 1   | Disorder/uncertainty; high = stale/chaotic   |
/// | `activation`             | 0 – 1   | Weighted combination; primary drive signal   |
/// | `execution_probability`  | 0 – 1   | Emission gate signal for future fiber launch |
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActivationCell {
    /// Thermal energy accumulated in this cell.
    /// Sources: topology events, explicit injection, diffusion from hot neighbours.
    pub heat: f32,

    /// Execution demand pressure propagated from neighbours or the topology graph.
    /// Represents how strongly adjacent cells "want" this cell to be active.
    pub pressure: f32,

    /// Entropy — how uncertain or stale the current state is.
    /// Grows when a cell is under-utilised; decays when the cell is activated.
    pub entropy: f32,

    /// Activation signal — the weighted combination of heat, pressure, and
    /// inverse entropy.  Computed by `ActivationField::recompute_activation()`.
    pub activation: f32,

    /// Continuous execution probability, derived from activation by the solver.
    /// A future fiber-emission gate reads this value as a probability weight.
    pub execution_probability: f32,
}

impl Default for ActivationCell {
    #[inline]
    fn default() -> Self {
        Self {
            heat: 0.0,
            pressure: 0.0,
            // Start with mid entropy — unknown state, not guaranteed clean.
            entropy: 0.5,
            activation: 0.0,
            execution_probability: 0.0,
        }
    }
}

impl ActivationCell {
    /// Create a zeroed cell with a specific initial entropy.
    #[inline]
    pub fn with_entropy(entropy: f32) -> Self {
        Self {
            entropy: entropy.clamp(0.0, 1.0),
            ..Self::default()
        }
    }
}

// =====================================================================
// FIELD CONSTANTS
// =====================================================================

/// Per-frame exponential decay rate for heat (multiplicative).
/// 0.97 means heat halves in ≈ 23 frames at 60 Hz → ~0.38 s.
pub const HEAT_DECAY: f32 = 0.97;

/// Per-frame diffusion coefficient (fraction transferred to each neighbour).
/// 0.04 × 4 neighbours = 0.16 total out-flow — keeps field stable.
pub const DIFFUSION_ALPHA: f32 = 0.04;

/// Per-frame entropy growth when activation is near zero.
pub const ENTROPY_GROWTH: f32 = 0.003;

/// Per-frame entropy decay when activation is high.
pub const ENTROPY_DECAY: f32 = 0.015;

/// Pressure stabilisation factor per step (how fast pressure equalises).
pub const PRESSURE_STABILISATION: f32 = 0.08;

// =====================================================================
// ACTIVATION FIELD
// =====================================================================

/// Two-dimensional continuous activation field over chunk space.
///
/// The field stores `width × height` [`ActivationCell`]s in a flat,
/// row-major `Vec`.  All operations are over contiguous slices to
/// maximise cache efficiency and enable SIMD auto-vectorisation.
///
/// # Coordinate Convention
/// `cells[y * width + x]` — row-major, origin at top-left.
///
/// # V3 Integration
/// `MKRWorld` owns a single `ActivationField` and passes it to the
/// `ActivationSolver` each tick.  The solver is stateless; it operates
/// only on the field's data.
pub struct ActivationField {
    /// Dense, row-major cell storage.
    pub cells: Vec<ActivationCell>,
    /// Grid width (number of cells per row).
    pub width: usize,
    /// Grid height (number of rows).
    pub height: usize,
}

impl ActivationField {
    // ------------------------------------------------------------------
    // Construction
    // ------------------------------------------------------------------

    /// Create a new activation field with `width × height` cells.
    ///
    /// All cells start at their [`Default`] values: heat = 0, pressure = 0,
    /// entropy = 0.5, activation = 0, execution_probability = 0.
    pub fn new(width: usize, height: usize) -> Self {
        let capacity = width * height;
        Self {
            cells: vec![ActivationCell::default(); capacity],
            width,
            height,
        }
    }

    /// Total number of cells.
    #[inline]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// True if the field has zero cells.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Linear index from 2-D coordinates.  Returns `None` if out-of-bounds.
    #[inline]
    pub fn index_of(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.width && y < self.height {
            Some(y * self.width + x)
        } else {
            None
        }
    }

    // ------------------------------------------------------------------
    // Heat injection
    // ------------------------------------------------------------------

    /// Inject heat at a specific linear cell index.
    ///
    /// Heat is clamped to `[0.0, 1.0]`.  Caller is responsible for
    /// converting chunk coordinates to field indices via [`index_of`].
    ///
    /// # TODO(V3-CEK)
    /// `amount` will eventually be a CEK-computed field value, not a
    /// scalar injected by ad-hoc callers.
    #[inline]
    pub fn inject_heat(&mut self, index: usize, amount: f32) {
        if let Some(cell) = self.cells.get_mut(index) {
            cell.heat = (cell.heat + amount).min(1.0);
        }
    }

    /// Inject heat at 2-D coordinates (convenience wrapper).
    #[inline]
    pub fn inject_heat_at(&mut self, x: usize, y: usize, amount: f32) {
        if let Some(idx) = self.index_of(x, y) {
            self.inject_heat(idx, amount);
        }
    }

    // ------------------------------------------------------------------
    // Pressure injection
    // ------------------------------------------------------------------

    /// Inject execution demand pressure at a specific linear cell index.
    ///
    /// Pressure represents neighbour-driven demand.  It is not a
    /// threshold; it participates continuously in activation computation.
    ///
    /// # TODO(V3-CEK)
    /// Pressure will eventually be sourced from topology graph edge
    /// weights, not from manual injection.
    #[inline]
    pub fn inject_pressure(&mut self, index: usize, amount: f32) {
        if let Some(cell) = self.cells.get_mut(index) {
            cell.pressure = (cell.pressure + amount).min(1.0);
        }
    }

    /// Inject pressure at 2-D coordinates (convenience wrapper).
    #[inline]
    pub fn inject_pressure_at(&mut self, x: usize, y: usize, amount: f32) {
        if let Some(idx) = self.index_of(x, y) {
            self.inject_pressure(idx, amount);
        }
    }

    // ------------------------------------------------------------------
    // Decay
    // ------------------------------------------------------------------

    /// Apply per-frame exponential decay to heat and pressure across the
    /// entire field.
    ///
    /// Entropy grows in cells with low activation (idle drift) and decays
    /// in cells with high activation (active clarity).
    ///
    /// This pass is fully branchless and vectorisable.
    pub fn decay(&mut self) {
        for cell in &mut self.cells {
            cell.heat *= HEAT_DECAY;
            cell.pressure *= 1.0 - PRESSURE_STABILISATION;

            // Entropy rises when idle, falls when active.
            // `cell.activation` is from the *previous* frame — intentionally
            // one step behind to avoid circular dependency within a single tick.
            let idle_weight = 1.0 - cell.activation;
            cell.entropy = (cell.entropy
                + ENTROPY_GROWTH * idle_weight
                - ENTROPY_DECAY * cell.activation)
                .clamp(0.0, 1.0);
        }
    }

    // ------------------------------------------------------------------
    // Diffusion
    // ------------------------------------------------------------------

    /// Diffuse heat across the grid (4-neighbour stencil, Neumann BC).
    ///
    /// Uses a read-then-write two-buffer approach to avoid in-place
    /// aliasing.  The `scratch` buffer is provided by the caller (solver)
    /// to avoid allocation on the hot path.
    ///
    /// # Algorithm
    /// For each cell (i,j):
    ///   new_heat = old_heat + α × (sum_of_neighbours − 4 × old_heat)
    ///
    /// Neumann boundary condition: missing neighbours are treated as
    /// equal to the cell itself, so the cell doesn't lose heat at edges.
    pub fn diffuse(&mut self, scratch: &mut Vec<f32>) {
        let n = self.cells.len();
        let w = self.width;
        let h = self.height;

        // Resize scratch buffer without zeroing — we will write every element.
        if scratch.len() != n {
            scratch.resize(n, 0.0);
        }

        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let center = self.cells[idx].heat;

                let north = if y > 0 { self.cells[(y - 1) * w + x].heat } else { center };
                let south = if y + 1 < h { self.cells[(y + 1) * w + x].heat } else { center };
                let west  = if x > 0 { self.cells[y * w + (x - 1)].heat } else { center };
                let east  = if x + 1 < w { self.cells[y * w + (x + 1)].heat } else { center };

                scratch[idx] = center + DIFFUSION_ALPHA * (north + south + west + east - 4.0 * center);
            }
        }

        // Write back
        for (cell, &new_heat) in self.cells.iter_mut().zip(scratch.iter()) {
            cell.heat = new_heat.clamp(0.0, 1.0);
        }
    }

    // ------------------------------------------------------------------
    // Activation recomputation
    // ------------------------------------------------------------------

    /// Recompute the `activation` scalar for every cell from the current
    /// heat, pressure, and entropy values.
    ///
    /// Formula (branchless, vectorisable):
    ///
    /// ```text
    /// activation = clamp(heat × 0.55 + pressure × 0.35 + (1 − entropy) × 0.10, 0, 1)
    /// ```
    ///
    /// Weights are intentionally unequal: heat is the dominant driver,
    /// pressure from neighbours amplifies it, entropy suppresses idle
    /// regions.
    ///
    /// # TODO(V3-CEK)
    /// Weights will eventually be output from `ExecutionWeights::compute_activation()`
    /// which will itself receive CEK-computed scalars.
    pub fn recompute_activation(&mut self) {
        for cell in &mut self.cells {
            cell.activation = (cell.heat * 0.55
                + cell.pressure * 0.35
                + (1.0 - cell.entropy) * 0.10)
                .clamp(0.0, 1.0);
        }
    }

    /// Recompute `execution_probability` from the current `activation`.
    ///
    /// Uses a smooth sigmoid-like curve so that:
    /// * Very low activation → near-zero probability (naturally gated)
    /// * Mid activation → linearly rising probability
    /// * High activation → saturates near 1.0 (fully eligible for emission)
    ///
    /// Formula:
    /// ```text
    /// p = activation² × (3 − 2 × activation)   [smoothstep]
    /// ```
    ///
    /// Smoothstep avoids hard cutoffs while still producing a near-zero
    /// probability for dormant cells.  Critically: this is a soft gate,
    /// not a state machine.  Fiber emission (future work) will sample
    /// this probability stochastically.
    pub fn recompute_execution_probability(&mut self) {
        for cell in &mut self.cells {
            let a = cell.activation;
            // Smoothstep S-curve: a² × (3 − 2a)
            cell.execution_probability = a * a * (3.0 - 2.0 * a);
        }
    }

    // ------------------------------------------------------------------
    // Diagnostic helpers
    // ------------------------------------------------------------------

    /// Return the mean activation across the entire field.
    pub fn mean_activation(&self) -> f32 {
        if self.cells.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.cells.iter().map(|c| c.activation).sum();
        sum / self.cells.len() as f32
    }

    /// Return the mean execution probability across the entire field.
    pub fn mean_execution_probability(&self) -> f32 {
        if self.cells.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.cells.iter().map(|c| c.execution_probability).sum();
        sum / self.cells.len() as f32
    }

    /// Count cells whose execution_probability exceeds a given threshold.
    ///
    /// Used for diagnostics and future fiber-emission budget estimation.
    /// NOTE: This is NOT a scheduling gate — the field operates continuously.
    pub fn count_above_probability(&self, threshold: f32) -> usize {
        self.cells
            .iter()
            .filter(|c| c.execution_probability > threshold)
            .count()
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_creation_and_size() {
        let field = ActivationField::new(16, 16);
        assert_eq!(field.len(), 256);
        assert_eq!(field.width, 16);
        assert_eq!(field.height, 16);
    }

    #[test]
    fn heat_injection_clamps() {
        let mut field = ActivationField::new(4, 4);
        field.inject_heat(0, 2.0); // Should clamp to 1.0
        assert_eq!(field.cells[0].heat, 1.0);
    }

    #[test]
    fn decay_reduces_heat() {
        let mut field = ActivationField::new(4, 4);
        field.inject_heat(0, 1.0);
        field.decay();
        assert!(field.cells[0].heat < 1.0);
        assert!(field.cells[0].heat > 0.0);
    }

    #[test]
    fn recompute_activation_is_bounded() {
        let mut field = ActivationField::new(4, 4);
        field.inject_heat(0, 1.0);
        field.inject_pressure(0, 1.0);
        field.cells[0].entropy = 0.0;
        field.recompute_activation();
        let a = field.cells[0].activation;
        assert!(a >= 0.0 && a <= 1.0, "activation out of range: {}", a);
    }

    #[test]
    fn execution_probability_smoothstep() {
        let mut field = ActivationField::new(1, 1);
        // At full activation, probability should be 1.0
        field.cells[0].activation = 1.0;
        field.recompute_execution_probability();
        assert!((field.cells[0].execution_probability - 1.0).abs() < 1e-6);

        // At zero activation, probability should be 0.0
        field.cells[0].activation = 0.0;
        field.recompute_execution_probability();
        assert!(field.cells[0].execution_probability.abs() < 1e-6);
    }

    #[test]
    fn diffuse_conserves_energy_approximately() {
        let mut field = ActivationField::new(8, 8);
        let mut scratch = Vec::new();
        // Inject heat at centre
        field.inject_heat(3 * 8 + 3, 1.0);
        let heat_before: f32 = field.cells.iter().map(|c| c.heat).sum();
        field.diffuse(&mut scratch);
        let heat_after: f32 = field.cells.iter().map(|c| c.heat).sum();
        // Diffusion conserves total energy (Neumann BC — no leak)
        assert!(
            (heat_after - heat_before).abs() < 0.01,
            "heat not conserved: before={}, after={}",
            heat_before,
            heat_after
        );
    }
}
