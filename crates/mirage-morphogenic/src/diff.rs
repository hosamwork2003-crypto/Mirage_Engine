#[derive(Debug, Clone, PartialEq)]
pub struct ContinuityDiff {
    pub index: usize,
    pub previous: f32,
    pub current: f32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContinuityDiffSequence {
    pub diffs: Vec<ContinuityDiff>,
}

impl ContinuityDiffSequence {
    pub fn from_snapshots(previous: &crate::continuity::ContinuitySnapshot, current: &crate::continuity::ContinuitySnapshot) -> Self {
        if previous.continuity.len() != current.continuity.len() {
            panic!("snapshot length mismatch in diff generation: {} vs {}", previous.continuity.len(), current.continuity.len());
        }
        let mut diffs = Vec::new();
        for i in 0..previous.continuity.len() {
            let p = previous.continuity[i];
            let c = current.continuity[i];
            if (p - c).abs() > f32::EPSILON {
                diffs.push(ContinuityDiff { index: i, previous: p, current: c });
            }
        }
        Self { diffs }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::continuity::ContinuitySnapshot;

    #[test]
    fn continuity_diff_generation_equivalence() {
        let a = ContinuitySnapshot::new(1, vec![0.0, 0.5, 1.0]);
        let b = ContinuitySnapshot::new(2, vec![0.0, 0.6, 0.9]);
        let seq = ContinuityDiffSequence::from_snapshots(&a, &b);
        assert_eq!(seq.diffs.len(), 2);
        assert_eq!(seq.diffs[0].index, 1);
        assert!((seq.diffs[0].previous - 0.5).abs() < 1e-6);
    }
}