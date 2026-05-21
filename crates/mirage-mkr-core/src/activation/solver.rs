// ===================================================================
// mirage-mkr-core/src/activation/solver.rs  (V3 — Differential Runtime Pass)
// PURPOSE: ActivationSolver — Field Propagation Engine
//
// ROLE IN V3:
// The solver is the stateless operator that drives the activation
// field forward one timestep.  MKRWorld::tick() calls it every frame.
//
// The solver is intentionally stateless (except for scratch buffers
// that avoid allocation).  This makes it:
// 1. Thread-safe by construction (no interior state).
// 2. Easy to migrate to a GPU compute shader.
// 3. Testable in isolation from the full MKRWorld.
//
// EXECUTION ORDER (per tick):
//   1. decay()                           — heat & pressure exponential decay
//   2. diffuse()                         — 4-neighbour heat diffusion
//   3. propagate_pressure()              — topology-driven pressure spread
//   4. recompute_activation()            — weighted blend of signals
//   5. recompute_execution_probability() — smoothstep soft gate
//
// ---------------------------------------------------------------
// TODO(V3-DIFFERENTIAL): FULL-FIELD RECOMPUTE ANALYSIS
// ---------------------------------------------------------------
//
// ALL five passes currently iterate over the ENTIRE field (O(N) each).
// Target differential migration order:
//
//   decay()             — MUST remain full-field (exponential decay is
//                         continuous; cells don't decay to a threshold).
//                         Exception: cells with heat < 1e-6 can be skipped.
//
//   diffuse()           — Can become frontier-local: only cells in the
//                         PropagationFrontier need diffusion updates.
//                         Non-frontier cells have zero heat gradient.
//
//   propagate_pressure  — First candidate for sparse migration.
//                         Only changed topology nodes or frontier cells
//                         need pressure re-propagation.  Migrate after
//                         frontier validation passes 1000 stable ticks.
//
//   recompute_activation         — Only changed cells need recompute.
//                                  Use FieldDeltaMask to filter.
//
//   recompute_execution_probability — Only changed activation cells.
//                                     Use PROBABILITY_CHANGED flag.
//
// Introduce `fn step_sparse(&mut field, topo_influence, frontier)`
// that runs all five passes on frontier cells only.
//
// GPU MIGRATION PATH:
// Each method maps cleanly to a compute shader dispatch:
//   decay                  → element-wise multiply pass
//   diffuse                → stencil convolution pass
//   propagate_pressure     → scatter/gather from edge list
//   recompute_activation   → element-wise FMA pass
//   recompute_probability  → element-wise polynomial pass
//
// TODO(V3-CEK): `propagate_pressure` will receive its edge weights
// from CEK field outputs rather than a flat influence scalar.
// ===================================================================

use super::field::ActivationField;
use super::frontier::PropagationFrontier;

/// Statistics produced by a single solver step.
///
/// Useful for diagnostics, profiling, and future emission-budget
/// estimation without requiring a full field scan after the step.
#[derive(Debug, Clone, Copy, Default)]
pub struct SolverStepStats {
    /// Mean activation across the entire field after this step.
    pub mean_activation: f32,
    /// Mean execution probability across the field after this step.
    pub mean_execution_probability: f32,
    /// Number of cells whose execution_probability > 0.5.
    /// A rough proxy for how many chunks are "hot enough to emit" next frame.
    pub high_probability_count: usize,
    /// Which step number this stats record describes.
    pub step: u64,
}

/// Stateless activation field solver.
///
/// Owns only pre-allocated scratch buffers to avoid per-frame heap
/// allocation.  All field mutation happens through the `&mut ActivationField`
/// argument.
///
/// # Thread Safety
/// `ActivationSolver` itself is `Send`.  The field is mutably borrowed
/// for the duration of each `step()` call and then released.
pub struct ActivationSolver {
    /// Scratch buffer for the diffusion pass (avoids per-frame alloc).
    diffusion_scratch: Vec<f32>,

    /// Scratch buffer for the pressure propagation pass.
    pressure_scratch: Vec<f32>,

    /// Cumulative step counter (monotonically increasing).
    step_count: u64,
}

impl ActivationSolver {
    /// Create a new solver.  No allocation occurs until the first
    /// `step()` call when scratch buffers are sized to the field.
    pub fn new() -> Self {
        Self {
            diffusion_scratch: Vec::new(),
            pressure_scratch: Vec::new(),
            step_count: 0,
        }
    }

    /// Return the number of solver steps executed so far.
    #[inline]
    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    // ------------------------------------------------------------------
    // Full step
    // ------------------------------------------------------------------

    /// Execute a complete activation field step.
    ///
    /// This is the primary hot-path entry point called by `MKRWorld::tick()`.
    /// It drives the field through all propagation phases and returns
    /// diagnostic statistics without requiring a separate scan.
    ///
    /// # Parameters
    /// * `field`          — mutable activation field (owned by MKRWorld).
    /// * `topo_influence` — flat slice of per-cell topology influence
    ///   scalars in `[0.0, 1.0]`.  Length must equal `field.len()`.
    ///   Pass an empty slice `&[]` to disable topology pressure; the
    ///   solver will use zero influence for all cells.
    pub fn step(
        &mut self,
        field: &mut ActivationField,
        topo_influence: &[f32],
    ) -> SolverStepStats {
        // Phase 1: decay heat and pressure, grow/decay entropy.
        field.decay();

        // Phase 2: diffuse heat across neighbours.
        field.diffuse(&mut self.diffusion_scratch);

        // Phase 3: propagate topology-driven pressure.
        self.propagate_pressure(field, topo_influence);

        // Phase 4: recompute activation from blended signals.
        field.recompute_activation();

        // Phase 5: recompute execution probability (smoothstep).
        field.recompute_execution_probability();

        self.step_count = self.step_count.wrapping_add(1);

        SolverStepStats {
            mean_activation:            field.mean_activation(),
            mean_execution_probability: field.mean_execution_probability(),
            high_probability_count:     field.count_above_probability(0.5),
            step:                       self.step_count,
        }
    }

    /// Execute a sparse activation field step over frontier cells only.
    pub fn step_sparse(
        &mut self,
        field: &mut ActivationField,
        frontier: &PropagationFrontier,
        topo_influence: &[f32],
    ) -> SolverStepStats {
        let _result = super::sparse::step_sparse(
            field,
            frontier,
            topo_influence,
            &mut self.diffusion_scratch,
            &mut self.pressure_scratch,
        );

        self.step_count = self.step_count.wrapping_add(1);

        SolverStepStats {
            mean_activation:            field.mean_activation(),
            mean_execution_probability: field.mean_execution_probability(),
            high_probability_count:     field.count_above_probability(0.5),
            step:                       self.step_count,
        }
    }

    // ------------------------------------------------------------------
    // Pressure propagation
    // ------------------------------------------------------------------

    /// Propagate topology-driven execution demand pressure across the field.
    ///
    /// For each cell, its pressure is additively blended with the
    /// topology influence signal for that cell.  A subsequent 4-neighbour
    /// averaging step smooths the pressure surface.
    ///
    /// This method is intentionally simple in V3 — it will be replaced
    /// by a CEK-driven graph walk once the topology influence interface
    /// is stable.
    ///
    /// # TODO(V3-CEK)
    /// Replace the flat `topo_influence` slice with a structured
    /// topology-edge traversal that accumulates pressure from directed
    /// graph relationships.
    ///
    /// # TODO(V3-TOPOLOGY)
    /// The TopologyGraph must expose an `influence_scalars()` method
    /// that returns a `&[f32]` aligned to field cell indices.
fn propagate_pressure(&mut self, field: &mut ActivationField, topo_influence: &[f32]) {
    let n = field.len();
    let w = field.width;
    let h = field.height;

    if self.pressure_scratch.len() != n {
        self.pressure_scratch.resize(n, 0.0);
    }

    // حد أدنى لقطع الضوضاء العائمة ومنع انتشار الـ tails الأزلية
    const NOISE_FLOOR: f32 = 1e-4;

    // Step A: inject topology influence into pressure.
    for (i, cell) in field.cells.iter().enumerate() {
        let infl = if i < topo_influence.len() { topo_influence[i] } else { 0.0 };
        let mut p = cell.pressure + infl * 0.3;
        
        // تطبيق الـ Noise Floor فوراً أثناء الحقن
        if p < NOISE_FLOOR { p = 0.0; }
        self.pressure_scratch[i] = p.min(1.0);
    }

    // Step B: 4-neighbour pressure average
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let center = self.pressure_scratch[idx];

            // إذا كان المركز وجيرانه أصفاراً، تخطي الحساب لضمان عدم حدوث float jitter
            if center == 0.0 {
                let north = if y > 0 { self.pressure_scratch[(y - 1) * w + x] } else { 0.0 };
                let south = if y + 1 < h { self.pressure_scratch[(y + 1) * w + x] } else { 0.0 };
                let west  = if x > 0 { self.pressure_scratch[y * w + (x - 1)] } else { 0.0 };
                let east  = if x + 1 < w { self.pressure_scratch[y * w + (x + 1)] } else { 0.0 };
                
                if north == 0.0 && south == 0.0 && west == 0.0 && east == 0.0 {
                    field.cells[idx].pressure = 0.0;
                    continue;
                }
            }

            let north = if y > 0 { self.pressure_scratch[(y - 1) * w + x] } else { center };
            let south = if y + 1 < h { self.pressure_scratch[(y + 1) * w + x] } else { center };
            let west  = if x > 0 { self.pressure_scratch[y * w + (x - 1)] } else { center };
            let east  = if x + 1 < w { self.pressure_scratch[y * w + (x + 1)] } else { center };

            let mut final_pressure = center * 0.5 + (north + south + west + east) * 0.125;
            if final_pressure < NOISE_FLOOR { final_pressure = 0.0; }
            
            field.cells[idx].pressure = final_pressure.clamp(0.0, 1.0);
        }
    }
}
    // ------------------------------------------------------------------
    // Targeted injection helpers (convenience wrappers for MKRWorld)
    // ------------------------------------------------------------------

    /// Inject a localised heat burst at a cell index.
    ///
    /// Intended for external events (collision, player action, streaming
    /// completion) that need to raise activation immediately without
    /// waiting for the next diffusion pass.
    ///
    /// # TODO(V3-CEK)
    /// CEK will emit these events as field signal packets rather than
    /// direct index injections.
    #[inline]
    pub fn inject_heat_burst(&self, field: &mut ActivationField, index: usize, amount: f32) {
        field.inject_heat(index, amount);
    }

    /// Inject a localised pressure event at a cell index.
    ///
    /// Analogous to `inject_heat_burst` but for pressure signals.
    #[inline]
    pub fn inject_pressure_event(
        &self,
        field: &mut ActivationField,
        index: usize,
        amount: f32,
    ) {
        field.inject_pressure(index, amount);
    }
}

impl Default for ActivationSolver {
    fn default() -> Self {
        Self::new()
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;

    #[test]
    fn solver_step_increments_counter() {
        let mut solver = ActivationSolver::new();
        let mut field = ActivationField::new(4, 4);
        solver.step(&mut field, &[]);
        assert_eq!(solver.step_count(), 1);
        solver.step(&mut field, &[]);
        assert_eq!(solver.step_count(), 2);
    }

    #[test]
    fn stats_are_bounded() {
        let mut solver = ActivationSolver::new();
        let mut field = ActivationField::new(8, 8);
        // Inject some heat to produce non-trivial stats.
        field.inject_heat(0, 0.8);
        field.inject_heat(15, 0.5);
        let stats = solver.step(&mut field, &[]);
        assert!(stats.mean_activation >= 0.0 && stats.mean_activation <= 1.0);
        assert!(
            stats.mean_execution_probability >= 0.0
                && stats.mean_execution_probability <= 1.0
        );
    }

    #[test]
    fn heat_decays_over_steps() {
        let mut solver = ActivationSolver::new();
        let mut field = ActivationField::new(4, 4);
        field.inject_heat(0, 1.0);
        let s1 = solver.step(&mut field, &[]);
        let s2 = solver.step(&mut field, &[]);
        // Mean activation must fall as heat decays.
        assert!(
            s2.mean_activation <= s1.mean_activation + 0.05,
            "activation should trend downward: {} vs {}",
            s2.mean_activation,
            s1.mean_activation
        );
    }

    #[test]
    fn topo_influence_raises_pressure() {
        let mut solver = ActivationSolver::new();
        let mut field = ActivationField::new(4, 4);
        let n = field.len();
        let influence: Vec<f32> = vec![1.0; n]; // Max topology pull for all cells
        solver.step(&mut field, &influence);
        // All cells should have non-zero pressure after topology injection.
        let has_pressure = field.cells.iter().any(|c| c.pressure > 0.0);
        assert!(has_pressure, "topology influence should produce non-zero pressure");
    }
}
