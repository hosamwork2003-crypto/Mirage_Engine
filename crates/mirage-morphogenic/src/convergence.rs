use crate::emergence::StructuralEmergenceState;

#[derive(Debug, Clone, PartialEq)]
pub struct EmergenceThresholdDescriptor {
    pub emergence_threshold: f32,
    pub resonance_threshold: f32,
    pub stabilization_threshold: f32,
}

impl EmergenceThresholdDescriptor {
    pub fn evaluate(&self, state: &StructuralEmergenceState) -> bool {
        state.emergence_score >= self.emergence_threshold &&
        state.resonance_factor >= self.resonance_threshold &&
        state.stabilization_factor >= self.stabilization_threshold
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructuralConvergenceState {
    pub continuity_pressure: f32,
    pub reinforcement_pressure: f32,
    pub resonance_pressure: f32,
    pub stabilization_pressure: f32,
}

impl StructuralConvergenceState {
    pub fn compute_convergence(continuity_pressure: f32, reinforcement_pressure: f32, resonance_pressure: f32, stabilization_pressure: f32) -> Self {
        let mut s = Self { continuity_pressure, reinforcement_pressure, resonance_pressure, stabilization_pressure };
        s.normalize();
        s
    }

    pub fn normalize(&mut self) {
        self.continuity_pressure = self.continuity_pressure.clamp(0.0, 1.0);
        self.reinforcement_pressure = self.reinforcement_pressure.clamp(0.0, 1.0);
        self.resonance_pressure = self.resonance_pressure.clamp(0.0, 1.0);
        self.stabilization_pressure = self.stabilization_pressure.clamp(0.0, 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emergence::StructuralEmergenceState;

    #[test]
    fn threshold_evaluation() {
        let state = StructuralEmergenceState::new(0.6, 0.7, 0.8, 1.0);
        let desc = EmergenceThresholdDescriptor { emergence_threshold: 0.5, resonance_threshold: 0.6, stabilization_threshold: 0.7 };
        assert!(desc.evaluate(&state));
    }

    #[test]
    fn convergence_compute_normalizes() {
        let s = StructuralConvergenceState::compute_convergence(1.5, -0.2, 0.5, 0.6);
        assert!(s.continuity_pressure <= 1.0 && s.continuity_pressure >= 0.0);
    }
}