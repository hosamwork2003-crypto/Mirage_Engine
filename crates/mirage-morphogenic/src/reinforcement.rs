#[derive(Debug, Clone, PartialEq)]
pub struct ReinforcementMemoryCell {
    pub accumulated_weight: f32,
    pub reinforcement_count: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReinforcementMemoryField {
    cells: Vec<ReinforcementMemoryCell>,
}

impl ReinforcementMemoryField {
    pub fn new(len: usize) -> Self {
        Self { cells: vec![ReinforcementMemoryCell { accumulated_weight: 0.0, reinforcement_count: 0 }; len] }
    }

    pub fn len(&self) -> usize { self.cells.len() }

    pub fn get(&self, idx: usize) -> Option<&ReinforcementMemoryCell> { self.cells.get(idx) }

    pub fn accumulate(&mut self, idx: usize, weight: f32) {
        if idx >= self.cells.len() { return; }
        let w = weight.clamp(0.0, 1.0);
        let cell = &mut self.cells[idx];
        cell.accumulated_weight = (cell.accumulated_weight + w).min(1.0);
        cell.reinforcement_count = cell.reinforcement_count.saturating_add(1);
    }

    pub fn deterministic_decay(&mut self, factor: f32) {
        let f = factor.clamp(0.0, 1.0);
        for cell in &mut self.cells {
            cell.accumulated_weight = (cell.accumulated_weight * f).clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reinforcement_accumulate_and_cap() {
        let mut f = ReinforcementMemoryField::new(1);
        f.accumulate(0, 0.6);
        f.accumulate(0, 0.6);
        let c = f.get(0).unwrap();
        assert!((c.accumulated_weight - 1.0).abs() < 1e-6);
        assert_eq!(c.reinforcement_count, 2);
    }

    #[test]
    fn reinforcement_decay_deterministic() {
        let mut f = ReinforcementMemoryField::new(2);
        f.accumulate(0, 0.5);
        f.accumulate(1, 0.8);
        f.deterministic_decay(0.5);
        assert!((f.get(0).unwrap().accumulated_weight - 0.25).abs() < 1e-6);
        assert!((f.get(1).unwrap().accumulated_weight - 0.4).abs() < 1e-6);
    }
}