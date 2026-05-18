// ===================================================================
// mirage-mkr-core/src/activation/weights.rs
// PURPOSE: ExecutionWeights — Runtime-Tunable Activation Coefficients
//
// DESIGN:
// ExecutionWeights captures the runtime-configurable blend of signals
// that feed into the activation computation.  Each weight is a
// continuous scalar in [0.0, 1.0] that scales one class of input.
//
// This is NOT a scheduler priority table.
// This is NOT an ECS component weight.
// It is a tunable linear combination kernel for the activation field.
//
// CURRENT SOURCES (V3, pre-CEK):
// - thermal_weight:  heat contribution from the thermal subsystem.
// - topology_weight: contribution from TopologyGraph edge influence.
// - entropy_weight:  how strongly entropy suppresses activation.
// - residency_weight: bonus activation for chunks currently resident
//   in VRAM (compatibility bridge to old ThermalSystem).
//
// TODO(V3-CEK): When CEK is integrated, these weights will be driven
// by CEK field outputs rather than manually tuned constants.  The
// `compute_activation` / `compute_probability` method signatures must
// stay stable.
// ===================================================================

/// Runtime-tunable weights that control the activation field blend.
///
/// All fields are normalised scalars in `[0.0, 1.0]`.
/// The activation formula is:
///
/// ```text
/// activation = clamp(
///     heat   × thermal_weight  +
///     topo   × topology_weight +
///     (1-e)  × (1 - entropy_weight × entropy) +
///     resid  × residency_weight,
///     0, 1
/// )
/// ```
///
/// where `entropy_weight` controls how strongly entropy penalises
/// activation (set to 0.0 to disable entropy influence entirely).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExecutionWeights {
    /// How much raw heat contributes to activation.
    /// Default: 0.50 — heat is the primary activation driver.
    pub thermal_weight: f32,

    /// How much topology-graph edge pressure contributes.
    /// Default: 0.25 — neighbours amplify activation.
    pub topology_weight: f32,

    /// How strongly entropy suppresses the activation signal.
    /// 0.0 = entropy has no effect; 1.0 = full entropy penalty.
    /// Default: 0.15.
    pub entropy_weight: f32,

    /// Bonus activation for currently VRAM-resident chunks.
    /// Compatibility bridge to the old ThermalSystem residency concept.
    /// Default: 0.10.
    pub residency_weight: f32,
}

impl Default for ExecutionWeights {
    fn default() -> Self {
        Self {
            thermal_weight:   0.50,
            topology_weight:  0.25,
            entropy_weight:   0.15,
            residency_weight: 0.10,
        }
    }
}

impl ExecutionWeights {
    // ------------------------------------------------------------------
    // Construction helpers
    // ------------------------------------------------------------------

    /// Create weights with custom values.  All inputs are clamped to
    /// `[0.0, 1.0]` to prevent runaway activation.
    pub fn new(thermal: f32, topology: f32, entropy: f32, residency: f32) -> Self {
        Self {
            thermal_weight:   thermal.clamp(0.0, 1.0),
            topology_weight:  topology.clamp(0.0, 1.0),
            entropy_weight:   entropy.clamp(0.0, 1.0),
            residency_weight: residency.clamp(0.0, 1.0),
        }
    }

    /// Weights biased toward thermal dominance — useful for initial
    /// bring-up before CEK is integrated.
    pub fn thermal_dominant() -> Self {
        Self {
            thermal_weight:   0.70,
            topology_weight:  0.15,
            entropy_weight:   0.10,
            residency_weight: 0.05,
        }
    }

    // ------------------------------------------------------------------
    // Activation computation
    // ------------------------------------------------------------------

    /// Compute a continuous activation scalar from raw field signals.
    ///
    /// # Parameters
    /// * `heat`      — cell heat (0..1).
    /// * `topo_pull` — topology graph influence on this cell (0..1).
    /// * `entropy`   — cell entropy (0..1, high = uncertain/idle).
    /// * `is_resident` — whether the backing chunk is currently VRAM-resident.
    ///   Passed as f32 (1.0 = yes, 0.0 = no) to stay branchless.
    ///
    /// # Returns
    /// Continuous activation in `[0.0, 1.0]`.
    #[inline]
    pub fn compute_activation(
        &self,
        heat: f32,
        topo_pull: f32,
        entropy: f32,
        is_resident: f32,
    ) -> f32 {
        let entropy_penalty = self.entropy_weight * entropy;
        let raw = heat      * self.thermal_weight
                + topo_pull * self.topology_weight
                + (1.0 - entropy_penalty)
                + is_resident * self.residency_weight;
        // Normalise by sum of all weights + 1.0 (from entropy complement term)
        let normaliser = self.thermal_weight
                       + self.topology_weight
                       + 1.0
                       + self.residency_weight;
        (raw / normaliser).clamp(0.0, 1.0)
    }

    /// Compute execution probability from an activation scalar.
    ///
    /// Applies a smoothstep curve so that:
    /// * Low activation → near-zero probability (soft gate)
    /// * Mid activation → smooth rise
    /// * High activation → saturates near 1.0
    ///
    /// This is the same formula as `ActivationField::recompute_execution_probability`
    /// but exposed here for use in per-chunk weight-aware emission decisions.
    ///
    /// Formula: `p = a² × (3 − 2a)`  (cubic Hermite interpolation).
    #[inline]
    pub fn compute_probability(&self, activation: f32) -> f32 {
        let a = activation.clamp(0.0, 1.0);
        a * a * (3.0 - 2.0 * a)
    }

    // ------------------------------------------------------------------
    // Runtime tuning
    // ------------------------------------------------------------------

    /// Linearly interpolate toward a target weight set at rate `t`.
    ///
    /// Useful for smooth runtime re-tuning without discontinuities.
    #[inline]
    pub fn lerp_toward(&self, target: &ExecutionWeights, t: f32) -> ExecutionWeights {
        let t = t.clamp(0.0, 1.0);
        ExecutionWeights {
            thermal_weight:   lerp(self.thermal_weight,   target.thermal_weight,   t),
            topology_weight:  lerp(self.topology_weight,  target.topology_weight,  t),
            entropy_weight:   lerp(self.entropy_weight,   target.entropy_weight,   t),
            residency_weight: lerp(self.residency_weight, target.residency_weight, t),
        }
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_weights_are_valid() {
        let w = ExecutionWeights::default();
        assert!(w.thermal_weight   >= 0.0 && w.thermal_weight   <= 1.0);
        assert!(w.topology_weight  >= 0.0 && w.topology_weight  <= 1.0);
        assert!(w.entropy_weight   >= 0.0 && w.entropy_weight   <= 1.0);
        assert!(w.residency_weight >= 0.0 && w.residency_weight <= 1.0);
    }

    #[test]
    fn compute_activation_bounded() {
        let w = ExecutionWeights::default();
        // Fully active: max heat, max topo, zero entropy, resident
        let a = w.compute_activation(1.0, 1.0, 0.0, 1.0);
        assert!(a >= 0.0 && a <= 1.0, "activation={}", a);

        // Fully dormant: no heat, no topo, max entropy, not resident
        let b = w.compute_activation(0.0, 0.0, 1.0, 0.0);
        assert!(b >= 0.0 && b <= 1.0, "activation={}", b);
    }

    #[test]
    fn higher_heat_raises_activation() {
        let w = ExecutionWeights::default();
        let low  = w.compute_activation(0.1, 0.0, 0.5, 0.0);
        let high = w.compute_activation(0.9, 0.0, 0.5, 0.0);
        assert!(high > low, "high_heat={} should be > low_heat={}", high, low);
    }

    #[test]
    fn compute_probability_smoothstep_endpoints() {
        let w = ExecutionWeights::default();
        assert!((w.compute_probability(0.0) - 0.0).abs() < 1e-6);
        assert!((w.compute_probability(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn probability_monotone() {
        let w = ExecutionWeights::default();
        let p0 = w.compute_probability(0.0);
        let p5 = w.compute_probability(0.5);
        let p1 = w.compute_probability(1.0);
        assert!(p0 <= p5 && p5 <= p1);
    }

    #[test]
    fn lerp_toward_interpolates() {
        let a = ExecutionWeights::default();
        let b = ExecutionWeights::thermal_dominant();
        let mid = a.lerp_toward(&b, 0.5);
        assert!((mid.thermal_weight - (a.thermal_weight + b.thermal_weight) / 2.0).abs() < 1e-6);
    }
}
