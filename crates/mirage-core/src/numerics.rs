#[derive(Clone, Debug, PartialEq)]
pub struct CanonicalFloatPolicy {
    pub epsilon: f32,
    pub clamp_min: f32,
    pub clamp_max: f32,
    pub normalization_epsilon: f32,
    pub deterministic_rounding_precision: i32,
}

impl Default for CanonicalFloatPolicy {
    fn default() -> Self {
        Self {
            epsilon: 1e-6,
            clamp_min: -1000.0,
            clamp_max: 1000.0,
            normalization_epsilon: 1e-5,
            deterministic_rounding_precision: 4,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FloatNormalizationMode {
    ClampOnly,
    CanonicalNormalize,
    StrictFiniteOnly,
}

pub fn round_to_precision_f32(val: f32, precision: i32) -> f32 {
    let factor = 10.0f32.powi(precision);
    (val * factor).round() / factor
}

pub fn round_to_precision_f64(val: f64, precision: i32) -> f64 {
    let factor = 10.0f64.powi(precision);
    (val * factor).round() / factor
}

pub fn canonicalize_f32(
    val: f32,
    policy: &CanonicalFloatPolicy,
    mode: FloatNormalizationMode,
) -> Result<f32, &'static str> {
    if val.is_nan() {
        return Err("NaN float detected");
    }
    if val.is_infinite() {
        return Err("Infinite float detected");
    }
    
    let rounded = round_to_precision_f32(val, policy.deterministic_rounding_precision);
    
    let result = match mode {
        FloatNormalizationMode::ClampOnly => {
            rounded.clamp(policy.clamp_min, policy.clamp_max)
        }
        FloatNormalizationMode::CanonicalNormalize => {
            if rounded.abs() < policy.normalization_epsilon {
                0.0
            } else {
                rounded.clamp(policy.clamp_min, policy.clamp_max)
            }
        }
        FloatNormalizationMode::StrictFiniteOnly => {
            rounded
        }
    };
    
    Ok(result)
}

pub fn canonicalize_f64(
    val: f64,
    policy: &CanonicalFloatPolicy,
    mode: FloatNormalizationMode,
) -> Result<f64, &'static str> {
    if val.is_nan() {
        return Err("NaN float detected");
    }
    if val.is_infinite() {
        return Err("Infinite float detected");
    }
    
    let rounded = round_to_precision_f64(val, policy.deterministic_rounding_precision);
    
    let result = match mode {
        FloatNormalizationMode::ClampOnly => {
            rounded.clamp(policy.clamp_min as f64, policy.clamp_max as f64)
        }
        FloatNormalizationMode::CanonicalNormalize => {
            if rounded.abs() < policy.normalization_epsilon as f64 {
                0.0
            } else {
                rounded.clamp(policy.clamp_min as f64, policy.clamp_max as f64)
            }
        }
        FloatNormalizationMode::StrictFiniteOnly => {
            rounded
        }
    };
    
    Ok(result)
}

pub fn stable_add_f32(a: f32, b: f32, policy: &CanonicalFloatPolicy) -> Result<f32, &'static str> {
    if a.is_nan() || b.is_nan() {
        return Err("NaN float in addition");
    }
    if a.is_infinite() || b.is_infinite() {
        return Err("Infinite float in addition");
    }
    let sum = a + b;
    canonicalize_f32(sum, policy, FloatNormalizationMode::StrictFiniteOnly)
}

pub fn stable_add_f64(a: f64, b: f64, policy: &CanonicalFloatPolicy) -> Result<f64, &'static str> {
    if a.is_nan() || b.is_nan() {
        return Err("NaN float in addition");
    }
    if a.is_infinite() || b.is_infinite() {
        return Err("Infinite float in addition");
    }
    let sum = a + b;
    canonicalize_f64(sum, policy, FloatNormalizationMode::StrictFiniteOnly)
}

pub fn stable_mul_f32(a: f32, b: f32, policy: &CanonicalFloatPolicy) -> Result<f32, &'static str> {
    if a.is_nan() || b.is_nan() {
        return Err("NaN float in multiplication");
    }
    if a.is_infinite() || b.is_infinite() {
        return Err("Infinite float in multiplication");
    }
    let prod = a * b;
    canonicalize_f32(prod, policy, FloatNormalizationMode::StrictFiniteOnly)
}

pub fn stable_mul_f64(a: f64, b: f64, policy: &CanonicalFloatPolicy) -> Result<f64, &'static str> {
    if a.is_nan() || b.is_nan() {
        return Err("NaN float in multiplication");
    }
    if a.is_infinite() || b.is_infinite() {
        return Err("Infinite float in multiplication");
    }
    let prod = a * b;
    canonicalize_f64(prod, policy, FloatNormalizationMode::StrictFiniteOnly)
}

pub fn stable_normalize_f32(val: f32, policy: &CanonicalFloatPolicy) -> Result<f32, &'static str> {
    if val.is_nan() {
        return Err("NaN float in normalization");
    }
    if val.is_infinite() {
        return Err("Infinite float in normalization");
    }
    
    let rounded = round_to_precision_f32(val, policy.deterministic_rounding_precision);
    if rounded.abs() < policy.normalization_epsilon {
        Ok(0.0)
    } else if rounded > 0.0 {
        Ok(1.0)
    } else {
        Ok(-1.0)
    }
}

pub fn stable_normalize_f64(val: f64, policy: &CanonicalFloatPolicy) -> Result<f64, &'static str> {
    if val.is_nan() {
        return Err("NaN float in normalization");
    }
    if val.is_infinite() {
        return Err("Infinite float in normalization");
    }
    
    let rounded = round_to_precision_f64(val, policy.deterministic_rounding_precision);
    if rounded.abs() < policy.normalization_epsilon as f64 {
        Ok(0.0)
    } else if rounded > 0.0 {
        Ok(1.0)
    } else {
        Ok(-1.0)
    }
}

// FNV-1a Hashing Functions
pub fn hash_bytes(bytes: &[u8]) -> u64 {
    const FNV_PRIME: u64 = 1099511628211;
    let mut hash: u64 = 14695981039346656037;
    for &byte in bytes {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }
    hash
}

pub fn hash_u64(mut val: u64, mut hash: u64) -> u64 {
    const FNV_PRIME: u64 = 1099511628211;
    for _ in 0..8 {
        let byte = (val & 0xFF) as u8;
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
        val >>= 8;
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_nan() {
        let policy = CanonicalFloatPolicy::default();
        assert!(canonicalize_f32(f32::NAN, &policy, FloatNormalizationMode::StrictFiniteOnly).is_err());
        assert!(canonicalize_f64(f64::NAN, &policy, FloatNormalizationMode::StrictFiniteOnly).is_err());
    }

    #[test]
    fn rejects_infinity() {
        let policy = CanonicalFloatPolicy::default();
        assert!(canonicalize_f32(f32::INFINITY, &policy, FloatNormalizationMode::StrictFiniteOnly).is_err());
        assert!(canonicalize_f64(f64::NEG_INFINITY, &policy, FloatNormalizationMode::StrictFiniteOnly).is_err());
    }

    #[test]
    fn deterministic_rounding() {
        let mut policy = CanonicalFloatPolicy::default();
        policy.deterministic_rounding_precision = 2;
        
        let val1 = 12.3456_f32;
        let res1 = canonicalize_f32(val1, &policy, FloatNormalizationMode::StrictFiniteOnly).unwrap();
        assert_eq!(res1, 12.35);

        let val2 = 12.3444_f32;
        let res2 = canonicalize_f32(val2, &policy, FloatNormalizationMode::StrictFiniteOnly).unwrap();
        assert_eq!(res2, 12.34);
    }

    #[test]
    fn stable_addition_order() {
        let policy = CanonicalFloatPolicy::default();
        // Float addition sequence checking
        let a = 0.1_f32;
        let b = 0.2_f32;
        let sum = stable_add_f32(a, b, &policy).unwrap();
        assert_eq!(sum, 0.3); // rounded to 4 decimals (0.3000)
    }

    #[test]
    fn stable_normalization() {
        let mut policy = CanonicalFloatPolicy::default();
        policy.normalization_epsilon = 0.01;
        policy.deterministic_rounding_precision = 3;

        assert_eq!(stable_normalize_f32(0.005, &policy).unwrap(), 0.0);
        assert_eq!(stable_normalize_f32(0.015, &policy).unwrap(), 1.0);
        assert_eq!(stable_normalize_f32(-0.015, &policy).unwrap(), -1.0);
    }

    #[test]
    fn canonical_float_equivalence() {
        let policy1 = CanonicalFloatPolicy {
            epsilon: 1e-6,
            clamp_min: 0.0,
            clamp_max: 1.0,
            normalization_epsilon: 1e-5,
            deterministic_rounding_precision: 2,
        };
        let val = 1.5_f32;
        // ClampOnly maps 1.5 to 1.0
        let res = canonicalize_f32(val, &policy1, FloatNormalizationMode::ClampOnly).unwrap();
        assert_eq!(res, 1.0);
    }
}
