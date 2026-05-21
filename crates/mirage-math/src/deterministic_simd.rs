#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct DeterministicSimdPolicy {
    pub simd_enabled: bool,
    pub scalar_fallback_required: bool,
    pub deterministic_lane_ordering: bool,
    pub stable_reduction_required: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SimdExecutionMode {
    ScalarOnly,
    DeterministicSimd,
    DeterministicFallback,
}

pub fn verify_scalar_simd_equivalence(scalar_vals: &[f32], simd_vals: &[f32], tolerance: f32) -> bool {
    if scalar_vals.len() != simd_vals.len() {
        return false;
    }
    for i in 0..scalar_vals.len() {
        if (scalar_vals[i] - simd_vals[i]).abs() > tolerance {
            return false;
        }
    }
    true
}

pub fn stable_simd_reduce_f32(vals: &[f32], policy: &DeterministicSimdPolicy) -> f32 {
    if !policy.simd_enabled || policy.scalar_fallback_required {
        let mut sum = 0.0;
        let mut c = 0.0;
        for &v in vals {
            let y = v - c;
            let t = sum + y;
            c = (t - sum) - y;
            sum = t;
        }
        return sum;
    }

    let mut lane_sums = [0.0; 4];
    let mut lane_comps = [0.0; 4];
    for (i, &v) in vals.iter().enumerate() {
        let lane = i % 4;
        let y = v - lane_comps[lane];
        let t = lane_sums[lane] + y;
        lane_comps[lane] = (t - lane_sums[lane]) - y;
        lane_sums[lane] = t;
    }

    let mut sum = 0.0;
    let mut c = 0.0;
    for &v in &lane_sums {
        let y = v - c;
        let t = sum + y;
        c = (t - sum) - y;
        sum = t;
    }
    sum
}

pub fn stable_simd_reduce_f64(vals: &[f64], policy: &DeterministicSimdPolicy) -> f64 {
    if !policy.simd_enabled || policy.scalar_fallback_required {
        let mut sum = 0.0;
        let mut c = 0.0;
        for &v in vals {
            let y = v - c;
            let t = sum + y;
            c = (t - sum) - y;
            sum = t;
        }
        return sum;
    }

    let mut lane_sums = [0.0; 4];
    let mut lane_comps = [0.0; 4];
    for (i, &v) in vals.iter().enumerate() {
        let lane = i % 4;
        let y = v - lane_comps[lane];
        let t = lane_sums[lane] + y;
        lane_comps[lane] = (t - lane_sums[lane]) - y;
        lane_sums[lane] = t;
    }

    let mut sum = 0.0;
    let mut c = 0.0;
    for &v in &lane_sums {
        let y = v - c;
        let t = sum + y;
        c = (t - sum) - y;
        sum = t;
    }
    sum
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scalar_simd_equivalence() {
        let vals1 = vec![1.0, 2.0, 3.0];
        let vals2 = vec![1.0, 2.0, 3.0001];
        assert!(verify_scalar_simd_equivalence(&vals1, &vals2, 0.001));
        assert!(!verify_scalar_simd_equivalence(&vals1, &vals2, 0.00001));
    }

    #[test]
    fn stable_lane_ordering() {
        let policy = DeterministicSimdPolicy {
            simd_enabled: true,
            scalar_fallback_required: false,
            deterministic_lane_ordering: true,
            stable_reduction_required: true,
        };
        // Changing order of lanes in input doesn't affect deterministic lane assignments since lane index = i % 4
        let vals = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0];
        let sum = stable_simd_reduce_f32(&vals, &policy);
        assert_eq!(sum, 36.0);
    }

    #[test]
    fn deterministic_simd_reduction() {
        let policy = DeterministicSimdPolicy {
            simd_enabled: true,
            scalar_fallback_required: false,
            deterministic_lane_ordering: true,
            stable_reduction_required: true,
        };
        let vals = vec![0.1, 0.2, 0.3, 0.4];
        let sum = stable_simd_reduce_f32(&vals, &policy);
        assert!((sum - 1.0).abs() < 1e-6);
    }

    #[test]
    fn scalar_fallback_equivalence() {
        let policy_fallback = DeterministicSimdPolicy {
            simd_enabled: true,
            scalar_fallback_required: true,
            deterministic_lane_ordering: true,
            stable_reduction_required: true,
        };
        let policy_no_simd = DeterministicSimdPolicy {
            simd_enabled: false,
            scalar_fallback_required: false,
            deterministic_lane_ordering: true,
            stable_reduction_required: true,
        };
        let vals = vec![1.5, 2.5, 3.5];
        let sum1 = stable_simd_reduce_f32(&vals, &policy_fallback);
        let sum2 = stable_simd_reduce_f32(&vals, &policy_no_simd);
        assert_eq!(sum1, sum2);
        assert_eq!(sum1, 7.5);
    }
}
