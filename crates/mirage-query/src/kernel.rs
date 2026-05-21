// ===================================================================
// mirage-query/src/kernel.rs
// PURPOSE: SolverKernel — Abstraction for field mutation passes
//
// PARITY INVARIANT
// ---------------------------------------------------------------
// All weight constants are copied verbatim from field.rs and
// sparse.rs to guarantee bit-identical results.
//
// activation = heat×0.55 + pressure×0.35 + (1−entropy)×0.10  [sparse.rs:271]
// exec_prob  = a × a × (3 − 2 × a)                            [smoothstep]
// heat decay = heat × HEAT_DECAY (0.97)                        [field.rs:97]
// entropy grows at +0.003/tick when activation < 0.1           [field.rs:104]
// entropy decays at −0.015×activation/tick when act ≥ 0.1     [field.rs:107]
// ===================================================================

/// Stable identifier for each first-class solver kernel.
///
/// The TraceFusionCompiler records sequences of these IDs as trace
/// signatures. Identical sequences across frames indicate a hot
/// execution path suitable for fusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KernelId {
    Decay,
    Diffuse,
    PropagatePressure,
    RecomputeActivation,
    RecomputeExecutionProbability,
    Custom(u32),
}

/// A named, composable field mutation pass over SoA columns.
///
/// The `apply_selected` method receives all five mutable SoA columns
/// and a shared selection mask. It only processes cells where
/// `selected[i] == true`, preserving correct filter semantics.
pub trait SolverKernel: Send + Sync {
    fn id(&self) -> KernelId;

    /// Execute the kernel over selected cells.
    ///
    /// # Safety
    /// All slices must have equal length ≥ `selected.len()`.
    fn apply_selected(
        &self,
        heat:       &mut [f32],
        pressure:   &mut [f32],
        entropy:    &mut [f32],
        activation: &mut [f32],
        exec_prob:  &mut [f32],
        selected:   &[bool],
    );
}

// -----------------------------------------------------------------------
// Field constants — must stay in sync with field.rs / sparse.rs
// -----------------------------------------------------------------------
pub const HEAT_DECAY:      f32 = 0.97;
pub const ENTROPY_GROWTH:  f32 = 0.003;
pub const ENTROPY_DECAY:   f32 = 0.015;

// Activation blend weights from sparse.rs line 271
const W_HEAT:    f32 = 0.55;
const W_PRESS:   f32 = 0.35;
const W_INV_ENT: f32 = 0.10;

// -----------------------------------------------------------------------
// Built-in kernels
// -----------------------------------------------------------------------

/// Kernel: Exponential heat decay and entropy dynamics.
/// Mirrors `ActivationField::decay()` for selected cells.
pub struct DecayKernel;
impl SolverKernel for DecayKernel {
    fn id(&self) -> KernelId { KernelId::Decay }

    fn apply_selected(
        &self,
        heat:       &mut [f32],
        _pressure:  &mut [f32],
        entropy:    &mut [f32],
        activation: &mut [f32],
        _exec_prob: &mut [f32],
        selected:   &[bool],
    ) {
        let n = heat.len().min(entropy.len()).min(activation.len()).min(selected.len());
        for i in 0..n {
            if !selected[i] { continue; }
            heat[i] *= HEAT_DECAY;
            if activation[i] < 0.1 {
                entropy[i] = (entropy[i] + ENTROPY_GROWTH).clamp(0.0, 1.0);
            } else {
                entropy[i] = (entropy[i] - ENTROPY_DECAY * activation[i]).clamp(0.0, 1.0);
            }
        }
    }
}

/// Kernel: Recompute activation from heat, pressure, entropy.
/// Uses weights from `sparse.rs` line 271: heat×0.55 + pressure×0.35 + (1−entropy)×0.10.
pub struct RecomputeActivationKernel;
impl SolverKernel for RecomputeActivationKernel {
    fn id(&self) -> KernelId { KernelId::RecomputeActivation }

    fn apply_selected(
        &self,
        heat:       &mut [f32],
        pressure:   &mut [f32],
        entropy:    &mut [f32],
        activation: &mut [f32],
        _exec_prob: &mut [f32],
        selected:   &[bool],
    ) {
        let n = heat.len().min(pressure.len()).min(entropy.len())
            .min(activation.len()).min(selected.len());
        for i in 0..n {
            if !selected[i] { continue; }
            let raw = heat[i] * W_HEAT
                + pressure[i] * W_PRESS
                + (1.0 - entropy[i]) * W_INV_ENT;
            activation[i] = raw.clamp(0.0, 1.0);
        }
    }
}

/// Kernel: Smoothstep execution probability gate.
/// Mirrors `ActivationField::recompute_execution_probability()`.
pub struct RecomputeExecProbKernel;
impl SolverKernel for RecomputeExecProbKernel {
    fn id(&self) -> KernelId { KernelId::RecomputeExecutionProbability }

    fn apply_selected(
        &self,
        _heat:      &mut [f32],
        _pressure:  &mut [f32],
        _entropy:   &mut [f32],
        activation: &mut [f32],
        exec_prob:  &mut [f32],
        selected:   &[bool],
    ) {
        let n = activation.len().min(exec_prob.len()).min(selected.len());
        for i in 0..n {
            if !selected[i] { continue; }
            let t = activation[i]; // already clamped [0,1] by previous kernel
            exec_prob[i] = t * t * (3.0 - 2.0 * t);
        }
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    fn make_cols(n: usize) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<bool>) {
        (vec![0.0; n], vec![0.0; n], vec![0.5; n], vec![0.0; n], vec![0.0; n], vec![true; n])
    }

    #[test]
    fn decay_reduces_heat_by_factor() {
        let (mut h, mut p, mut e, mut a, mut x, sel) = make_cols(4);
        h.fill(1.0);
        DecayKernel.apply_selected(&mut h, &mut p, &mut e, &mut a, &mut x, &sel);
        assert!((h[0] - HEAT_DECAY).abs() < 1e-6);
    }

    #[test]
    fn activation_kernel_uses_correct_weights() {
        let (mut h, mut p, mut e, mut a, mut x, sel) = make_cols(1);
        h[0] = 1.0; p[0] = 1.0; e[0] = 0.0;
        RecomputeActivationKernel.apply_selected(&mut h, &mut p, &mut e, &mut a, &mut x, &sel);
        let expected = (W_HEAT + W_PRESS + W_INV_ENT).clamp(0.0, 1.0);
        assert!((a[0] - expected).abs() < 1e-6);
    }

    #[test]
    fn exec_prob_smoothstep_endpoints() {
        let (mut h, mut p, mut e, mut a, mut x, sel) = make_cols(2);
        a[0] = 0.0; a[1] = 1.0;
        RecomputeExecProbKernel.apply_selected(&mut h, &mut p, &mut e, &mut a, &mut x, &sel);
        assert!(x[0].abs() < 1e-6);
        assert!((x[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn kernel_respects_selection_mask() {
        let (mut h, mut p, mut e, mut a, mut x, mut sel) = make_cols(4);
        h.fill(1.0);
        sel[1] = false; // deselect cell 1
        sel[3] = false; // deselect cell 3
        DecayKernel.apply_selected(&mut h, &mut p, &mut e, &mut a, &mut x, &sel);
        // Selected cells should have decayed
        assert!((h[0] - HEAT_DECAY).abs() < 1e-6);
        assert!((h[2] - HEAT_DECAY).abs() < 1e-6);
        // Deselected cells must be unchanged
        assert!((h[1] - 1.0).abs() < 1e-6);
        assert!((h[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn kernel_id_is_stable() {
        assert_eq!(DecayKernel.id(),                KernelId::Decay);
        assert_eq!(RecomputeActivationKernel.id(),  KernelId::RecomputeActivation);
        assert_eq!(RecomputeExecProbKernel.id(),    KernelId::RecomputeExecutionProbability);
    }
}
