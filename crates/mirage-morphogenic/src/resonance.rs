#[derive(Debug, Clone, PartialEq)]
pub struct EmergenceResonanceSnapshot {
    pub epoch: u64,
    pub resonance: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmergenceResonanceField {
    resonance: Vec<f32>,
}

impl EmergenceResonanceField {
    pub fn new(len: usize) -> Self { Self { resonance: vec![0.0f32; len] } }
    pub fn len(&self) -> usize { self.resonance.len() }
    pub fn get(&self, idx: usize) -> Option<f32> { self.resonance.get(idx).copied() }
    pub fn set(&mut self, idx: usize, value: f32) {
        if idx < self.resonance.len() { self.resonance[idx] = value.clamp(0.0, 1.0); }
    }
    pub fn snapshot(&self, epoch: u64) -> EmergenceResonanceSnapshot { EmergenceResonanceSnapshot { epoch, resonance: self.resonance.clone() } }
    pub fn apply_snapshot(&mut self, snap: &EmergenceResonanceSnapshot) {
        if snap.resonance.len() != self.resonance.len() { panic!("snapshot length mismatch"); }
        self.resonance = snap.resonance.clone();
    }
    pub fn smoothing_pass(&mut self) {
        let n = self.resonance.len();
        if n == 0 { return; }
        let mut smoothed = self.resonance.clone();
        for i in 0..n {
            let mut sum = self.resonance[i];
            let mut count = 1.0f32;
            if i > 0 { sum += self.resonance[i - 1]; count += 1.0; }
            if i + 1 < n { sum += self.resonance[i + 1]; count += 1.0; }
            smoothed[i] = (sum / count).clamp(0.0, 1.0);
        }
        self.resonance = smoothed;
    }
    pub fn stable_index_iter_clone(&self) -> Vec<(usize, f32)> { self.resonance.iter().enumerate().map(|(i, &v)| (i, v)).collect() }
}

// Emergence provenance for resonance propagation
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EmergenceProvenance {
    pub originating_tick: u64,
    pub continuity_epoch: u64,
    pub resonance_sequence_index: u64,
    pub topology_generation: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResonancePropagationDescriptor {
    pub sequence_index: u64,
    pub target_index: usize,
    pub weight: f32,
    pub provenance: EmergenceProvenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ResonancePropagationSequence {
    pub descriptors: Vec<ResonancePropagationDescriptor>,
}

impl ResonancePropagationSequence {
    pub fn new() -> Self { Self { descriptors: Vec::new() } }
    pub fn stable_sort(&mut self) {
        self.descriptors.sort_by(|a, b| a.sequence_index.cmp(&b.sequence_index).then_with(|| a.target_index.cmp(&b.target_index)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resonance_field_smoothing() {
        let mut f = EmergenceResonanceField::new(3);
        f.set(0, 1.0);
        f.set(1, 0.0);
        f.set(2, 0.0);
        f.smoothing_pass();
        assert!((f.get(0).unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn resonance_sequence_sorting() {
        let mut s = ResonancePropagationSequence::new();
        s.descriptors.push(ResonancePropagationDescriptor { sequence_index: 2, target_index: 1, weight: 0.5, provenance: EmergenceProvenance { originating_tick:0, continuity_epoch:0, resonance_sequence_index:0, topology_generation:0 } });
        s.descriptors.push(ResonancePropagationDescriptor { sequence_index: 1, target_index: 2, weight: 0.5, provenance: EmergenceProvenance { originating_tick:0, continuity_epoch:0, resonance_sequence_index:0, topology_generation:0 } });
        s.stable_sort();
        assert_eq!(s.descriptors[0].sequence_index, 1);
    }
}