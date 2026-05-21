#[derive(Debug, Clone, PartialEq)]
pub struct StructuralPersistenceField {
    persistence: Vec<f32>,
}

impl StructuralPersistenceField {
    pub fn new(len: usize) -> Self { Self { persistence: vec![0.0f32; len] } }

    pub fn len(&self) -> usize { self.persistence.len() }

    pub fn get(&self, idx: usize) -> Option<f32> { self.persistence.get(idx).copied() }

    pub fn set(&mut self, idx: usize, value: f32) {
        if idx < self.persistence.len() {
            self.persistence[idx] = value.clamp(0.0, 1.0);
        }
    }

    pub fn decay(&mut self, factor: f32) {
        let f = factor.clamp(0.0, 1.0);
        for v in &mut self.persistence {
            *v = (*v * f).clamp(0.0, 1.0);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn persistence_set_get_decay() {
        let mut p = StructuralPersistenceField::new(2);
        p.set(0, 1.0);
        assert!((p.get(0).unwrap() - 1.0).abs() < 1e-6);
        p.decay(0.5);
        assert!((p.get(0).unwrap() - 0.5).abs() < 1e-6);
    }
}