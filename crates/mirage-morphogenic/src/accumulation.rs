pub struct CanonicalAccumulatorF32 {
    pub ordered_values: Vec<f32>,
    pub accumulation_epoch: u64,
    pub deterministic_sum: f32,
    pub sequence_indices: Vec<u64>,
}

impl CanonicalAccumulatorF32 {
    pub fn new(epoch: u64) -> Self {
        Self {
            ordered_values: Vec::new(),
            accumulation_epoch: epoch,
            deterministic_sum: 0.0,
            sequence_indices: Vec::new(),
        }
    }

    pub fn push(&mut self, sequence_index: u64, val: f32) {
        self.ordered_values.push(val);
        self.sequence_indices.push(sequence_index);
        self.deterministic_sum = self.stable_sum();
    }

    fn sorted_indices(&self) -> Vec<usize> {
        let mut idxs: Vec<usize> = (0..self.ordered_values.len()).collect();
        idxs.sort_by(|&a, &b| {
            self.sequence_indices[a].cmp(&self.sequence_indices[b])
        });
        idxs
    }

    pub fn stable_sum(&self) -> f32 {
        let idxs = self.sorted_indices();
        let mut sum = 0.0;
        let mut c = 0.0;
        for &i in &idxs {
            let y = self.ordered_values[i] - c;
            let t = sum + y;
            c = (t - sum) - y;
            sum = t;
        }
        sum
    }

    pub fn stable_mean(&self) -> f32 {
        let len = self.ordered_values.len();
        if len == 0 {
            0.0
        } else {
            self.stable_sum() / len as f32
        }
    }

    pub fn stable_variance(&self) -> f32 {
        let len = self.ordered_values.len();
        if len == 0 {
            return 0.0;
        }
        let mean = self.stable_mean();
        let idxs = self.sorted_indices();
        let mut sum_sq_diff = 0.0;
        let mut c = 0.0;
        for &i in &idxs {
            let diff = self.ordered_values[i] - mean;
            let y = (diff * diff) - c;
            let t = sum_sq_diff + y;
            c = (t - sum_sq_diff) - y;
            sum_sq_diff = t;
        }
        sum_sq_diff / len as f32
    }

    pub fn clear(&mut self) {
        self.ordered_values.clear();
        self.sequence_indices.clear();
        self.deterministic_sum = 0.0;
    }
}

pub struct CanonicalAccumulatorF64 {
    pub ordered_values: Vec<f64>,
    pub accumulation_epoch: u64,
    pub deterministic_sum: f64,
    pub sequence_indices: Vec<u64>,
}

impl CanonicalAccumulatorF64 {
    pub fn new(epoch: u64) -> Self {
        Self {
            ordered_values: Vec::new(),
            accumulation_epoch: epoch,
            deterministic_sum: 0.0,
            sequence_indices: Vec::new(),
        }
    }

    pub fn push(&mut self, sequence_index: u64, val: f64) {
        self.ordered_values.push(val);
        self.sequence_indices.push(sequence_index);
        self.deterministic_sum = self.stable_sum();
    }

    fn sorted_indices(&self) -> Vec<usize> {
        let mut idxs: Vec<usize> = (0..self.ordered_values.len()).collect();
        idxs.sort_by(|&a, &b| {
            self.sequence_indices[a].cmp(&self.sequence_indices[b])
        });
        idxs
    }

    pub fn stable_sum(&self) -> f64 {
        let idxs = self.sorted_indices();
        let mut sum = 0.0;
        let mut c = 0.0;
        for &i in &idxs {
            let y = self.ordered_values[i] - c;
            let t = sum + y;
            c = (t - sum) - y;
            sum = t;
        }
        sum
    }

    pub fn stable_mean(&self) -> f64 {
        let len = self.ordered_values.len();
        if len == 0 {
            0.0
        } else {
            self.stable_sum() / len as f64
        }
    }

    pub fn stable_variance(&self) -> f64 {
        let len = self.ordered_values.len();
        if len == 0 {
            return 0.0;
        }
        let mean = self.stable_mean();
        let idxs = self.sorted_indices();
        let mut sum_sq_diff = 0.0;
        let mut c = 0.0;
        for &i in &idxs {
            let diff = self.ordered_values[i] - mean;
            let y = (diff * diff) - c;
            let t = sum_sq_diff + y;
            c = (t - sum_sq_diff) - y;
            sum_sq_diff = t;
        }
        sum_sq_diff / len as f64
    }

    pub fn clear(&mut self) {
        self.ordered_values.clear();
        self.sequence_indices.clear();
        self.deterministic_sum = 0.0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_accumulation() {
        let mut acc1 = CanonicalAccumulatorF32::new(1);
        let mut acc2 = CanonicalAccumulatorF32::new(1);

        acc1.push(2, 10.5);
        acc1.push(1, 20.25);

        acc2.push(1, 20.25);
        acc2.push(2, 10.5);

        // Sorted order is identical regardless of push order
        assert_eq!(acc1.stable_sum(), acc2.stable_sum());
        assert_eq!(acc1.stable_sum(), 30.75);
    }

    #[test]
    fn stable_mean_equivalence() {
        let mut acc = CanonicalAccumulatorF32::new(1);
        acc.push(10, 1.0);
        acc.push(20, 2.0);
        acc.push(30, 3.0);
        assert_eq!(acc.stable_mean(), 2.0);
    }

    #[test]
    fn stable_variance_equivalence() {
        let mut acc = CanonicalAccumulatorF32::new(1);
        acc.push(10, 2.0);
        acc.push(20, 4.0);
        acc.push(30, 4.0);
        acc.push(40, 4.0);
        acc.push(50, 6.0);
        // values: 2, 4, 4, 4, 6. mean = 4. variance = (4+0+0+0+4)/5 = 1.6
        assert!((acc.stable_variance() - 1.6).abs() < 1e-6);
    }

    #[test]
    fn insertion_order_preserved() {
        let mut acc = CanonicalAccumulatorF32::new(1);
        acc.push(1, 10.0);
        acc.push(1, 20.0);
        
        let idxs = acc.sorted_indices();
        // Since sequence indices are equal (1), stable sort preserves insertion order: 10.0 then 20.0
        assert_eq!(idxs, vec![0, 1]);
    }

    #[test]
    fn replay_safe_accumulation() {
        let mut acc = CanonicalAccumulatorF64::new(42);
        acc.push(100, 1.2345);
        acc.push(50, 6.789);
        assert_eq!(acc.accumulation_epoch, 42);
        assert_eq!(acc.stable_sum(), 1.2345 + 6.789);
    }
}
