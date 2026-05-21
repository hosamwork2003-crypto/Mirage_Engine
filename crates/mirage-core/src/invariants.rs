#[derive(Debug, Clone)]
pub struct DeterministicInvariantViolation {
    pub invariant_name: &'static str,
    pub subsystem: &'static str,
}

/// Validate that the provided sequence of u64 indices is non-decreasing.
/// Returns Ok(()) if valid, otherwise Err with a single violation describing the subsystem.
pub fn validate_stable_sorting(indices: &[u64], subsystem: &'static str) -> Result<(), Vec<DeterministicInvariantViolation>> {
    for i in 1..indices.len() {
        if indices[i - 1] > indices[i] {
            return Err(vec![DeterministicInvariantViolation { invariant_name: "stable_sorting", subsystem }]);
        }
    }
    Ok(())
}

/// Validate snapshot equivalence (exact element-wise equality).
pub fn validate_snapshot_equivalence(prev: &[f32], next: &[f32], subsystem: &'static str) -> Result<(), Vec<DeterministicInvariantViolation>> {
    if prev.len() != next.len() {
        return Err(vec![DeterministicInvariantViolation { invariant_name: "snapshot_length_mismatch", subsystem }]);
    }
    for i in 0..prev.len() {
        if (prev[i] - next[i]).abs() > f32::EPSILON {
            return Err(vec![DeterministicInvariantViolation { invariant_name: "snapshot_value_mismatch", subsystem }]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validate_stable_sorting_ok() {
        let v = vec![0u64, 1, 1, 2, 3];
        assert!(validate_stable_sorting(&v, "test").is_ok());
    }

    #[test]
    fn validate_stable_sorting_fails() {
        let v = vec![0u64, 3, 2];
        assert!(validate_stable_sorting(&v, "test").is_err());
    }

    #[test]
    fn validate_snapshot_equivalence_ok() {
        let a = [0.0f32, 0.5];
        let b = [0.0f32, 0.5];
        assert!(validate_snapshot_equivalence(&a, &b, "test").is_ok());
    }

    #[test]
    fn validate_snapshot_equivalence_fail() {
        let a = [0.0f32, 0.5];
        let b = [0.0f32, 0.6];
        assert!(validate_snapshot_equivalence(&a, &b, "test").is_err());
    }
}