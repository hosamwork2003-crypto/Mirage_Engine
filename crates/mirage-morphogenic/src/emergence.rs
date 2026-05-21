#[derive(Debug, Clone, PartialEq)]
pub struct StructuralEmergenceState {
    pub emergence_score: f32,
    pub resonance_factor: f32,
    pub stabilization_factor: f32,
    pub convergence_factor: f32,
}

impl StructuralEmergenceState {
    pub fn new(emergence_score: f32, resonance_factor: f32, stabilization_factor: f32, convergence_factor: f32) -> Self {
        let mut s = Self { emergence_score, resonance_factor, stabilization_factor, convergence_factor };
        s.normalize();
        s
    }

    /// Clamp all fields to [0.0, 1.0]
    pub fn normalize(&mut self) {
        self.emergence_score = self.emergence_score.clamp(0.0, 1.0);
        self.resonance_factor = self.resonance_factor.clamp(0.0, 1.0);
        self.stabilization_factor = self.stabilization_factor.clamp(0.0, 1.0);
        self.convergence_factor = self.convergence_factor.clamp(0.0, 1.0);
    }

    /// Apply deterministic resonance contribution and return a new state (immutable input)
    pub fn apply_resonance(&self, resonance: f32) -> Self {
        let mut s = self.clone();
        let r = resonance.clamp(0.0, 1.0);
        s.resonance_factor = (s.resonance_factor + r).clamp(0.0, 1.0);
        s.emergence_score = (s.emergence_score + r * s.resonance_factor).clamp(0.0, 1.0);
        s
    }

    /// Apply deterministic stabilization contribution and return a new state
    pub fn apply_stabilization(&self, factor: f32) -> Self {
        let mut s = self.clone();
        let f = factor.clamp(0.0, 1.0);
        s.stabilization_factor = (s.stabilization_factor + f).clamp(0.0, 1.0);
        s
    }

    /// Deterministic convergence metric for this emergence state (pure)
    pub fn convergence_state(&self) -> f32 {
        let val = self.emergence_score * self.resonance_factor * self.stabilization_factor * self.convergence_factor;
        val.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emergence_normalize_and_apply() {
        let s = StructuralEmergenceState::new(1.5, -0.2, 0.8, 0.9);
        assert!((s.emergence_score - 1.0).abs() < 1e-6);
        assert!((s.resonance_factor - 0.0).abs() < 1e-6);
        let s2 = s.apply_resonance(0.25);
        assert!((s2.resonance_factor - 0.25).abs() < 1e-6);
        assert!(s2.convergence_state() <= 1.0);
    }
}