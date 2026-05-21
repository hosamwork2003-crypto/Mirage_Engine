use serde::{Serialize, Deserialize};
use crate::numerics::{CanonicalFloatPolicy, hash_bytes};
use mirage_math::DeterministicSimdPolicy;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformDeterminismSignature {
    pub architecture: String,
    pub compiler_version: String,
    pub float_policy_hash: u64,
    pub simd_policy_hash: u64,
    pub runtime_policy_hash: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlatformDriftReport {
    pub drift_detected: bool,
    pub differing_signatures: Option<(PlatformDeterminismSignature, PlatformDeterminismSignature)>,
    pub deterministic_compatibility: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeDeterminismSeal {
    pub runtime_hash: u64,
    pub frame_hash: u64,
    pub replay_hash: u64,
    pub policy_hash: u64,
    pub platform_signature: PlatformDeterminismSignature,
}

pub fn hash_float_policy(policy: &CanonicalFloatPolicy) -> u64 {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(&policy.epsilon.to_le_bytes());
    bytes.extend_from_slice(&policy.clamp_min.to_le_bytes());
    bytes.extend_from_slice(&policy.clamp_max.to_le_bytes());
    bytes.extend_from_slice(&policy.normalization_epsilon.to_le_bytes());
    bytes.extend_from_slice(&policy.deterministic_rounding_precision.to_le_bytes());
    hash_bytes(&bytes)
}

pub fn hash_simd_policy(policy: &DeterministicSimdPolicy) -> u64 {
    let mut bytes = Vec::new();
    bytes.push(policy.simd_enabled as u8);
    bytes.push(policy.scalar_fallback_required as u8);
    bytes.push(policy.deterministic_lane_ordering as u8);
    bytes.push(policy.stable_reduction_required as u8);
    hash_bytes(&bytes)
}

pub fn compute_platform_signature(
    float_policy_hash: u64,
    simd_policy_hash: u64,
    runtime_policy_hash: u64,
) -> PlatformDeterminismSignature {
    PlatformDeterminismSignature {
        architecture: std::env::consts::ARCH.to_string(),
        compiler_version: option_env!("RUSTC_VERSION").unwrap_or("rustc-1.78.0").to_string(),
        float_policy_hash,
        simd_policy_hash,
        runtime_policy_hash,
    }
}

pub fn verify_platform_compatibility(
    a: &PlatformDeterminismSignature,
    b: &PlatformDeterminismSignature,
) -> PlatformDriftReport {
    let drift_detected = a != b;
    let deterministic_compatibility = !drift_detected;
    let differing_signatures = if drift_detected {
        Some((a.clone(), b.clone()))
    } else {
        None
    };

    PlatformDriftReport {
        drift_detected,
        differing_signatures,
        deterministic_compatibility,
    }
}

pub fn verify_runtime_policy_compatibility(
    sig: &PlatformDeterminismSignature,
    float_policy_hash: u64,
    simd_policy_hash: u64,
    runtime_policy_hash: u64,
) -> bool {
    sig.float_policy_hash == float_policy_hash
        && sig.simd_policy_hash == simd_policy_hash
        && sig.runtime_policy_hash == runtime_policy_hash
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_policy_mismatch() {
        let sig = PlatformDeterminismSignature {
            architecture: "x86_64".to_string(),
            compiler_version: "rustc-1.78.0".to_string(),
            float_policy_hash: 100,
            simd_policy_hash: 200,
            runtime_policy_hash: 300,
        };
        assert!(!verify_runtime_policy_compatibility(&sig, 100, 200, 999));
        assert!(verify_runtime_policy_compatibility(&sig, 100, 200, 300));
    }

    #[test]
    fn detects_signature_drift() {
        let a = PlatformDeterminismSignature {
            architecture: "x86_64".to_string(),
            compiler_version: "rustc-1.78.0".to_string(),
            float_policy_hash: 100,
            simd_policy_hash: 200,
            runtime_policy_hash: 300,
        };
        let b = PlatformDeterminismSignature {
            architecture: "aarch64".to_string(),
            compiler_version: "rustc-1.78.0".to_string(),
            float_policy_hash: 100,
            simd_policy_hash: 200,
            runtime_policy_hash: 300,
        };
        let report = verify_platform_compatibility(&a, &b);
        assert!(report.drift_detected);
        assert!(!report.deterministic_compatibility);
        assert_eq!(report.differing_signatures.unwrap(), (a, b));
    }

    #[test]
    fn compatible_platforms_pass() {
        let a = PlatformDeterminismSignature {
            architecture: "x86_64".to_string(),
            compiler_version: "rustc-1.78.0".to_string(),
            float_policy_hash: 100,
            simd_policy_hash: 200,
            runtime_policy_hash: 300,
        };
        let b = a.clone();
        let report = verify_platform_compatibility(&a, &b);
        assert!(!report.drift_detected);
        assert!(report.deterministic_compatibility);
        assert!(report.differing_signatures.is_none());
    }

    #[test]
    fn deterministic_policy_equivalence() {
        let policy1 = CanonicalFloatPolicy::default();
        let policy2 = CanonicalFloatPolicy::default();
        let hash1 = hash_float_policy(&policy1);
        let hash2 = hash_float_policy(&policy2);
        assert_eq!(hash1, hash2);
    }
}
