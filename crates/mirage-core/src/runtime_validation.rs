use crate::invariants::DeterministicInvariantViolation;

pub fn validate_runtime_phase_ordering(phases: &[u8]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    let canonical = [0, 1, 2, 3, 4, 5, 6, 7, 8];
    if phases.len() != canonical.len() {
        return Err(vec![DeterministicInvariantViolation {
            invariant_name: "invalid_phase_count",
            subsystem: "runtime_governance",
        }]);
    }
    for i in 0..phases.len() {
        if phases[i] != canonical[i] {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "invalid_phase_order",
                subsystem: "runtime_governance",
            }]);
        }
    }
    Ok(())
}

pub fn validate_runtime_frame_sequence(frame_sequences: &[u64], originating_ticks: &[u64]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    if frame_sequences.len() != originating_ticks.len() {
        return Err(vec![DeterministicInvariantViolation {
            invariant_name: "mismatched_sequence_lengths",
            subsystem: "runtime_governance",
        }]);
    }
    for i in 1..frame_sequences.len() {
        let prev_seq = frame_sequences[i - 1];
        let curr_seq = frame_sequences[i];
        let prev_tick = originating_ticks[i - 1];
        let curr_tick = originating_ticks[i];

        if prev_seq > curr_seq {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "frame_sequence_not_sorted",
                subsystem: "runtime_governance",
            }]);
        } else if prev_seq == curr_seq {
            if prev_tick >= curr_tick {
                return Err(vec![DeterministicInvariantViolation {
                    invariant_name: "duplicate_or_unsorted_tick",
                    subsystem: "runtime_governance",
                }]);
            }
        }
    }
    Ok(())
}

pub fn validate_runtime_epoch_progression(epochs: &[u64]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    for i in 1..epochs.len() {
        if epochs[i - 1] > epochs[i] {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "epoch_regression",
                subsystem: "runtime_governance",
            }]);
        }
    }
    Ok(())
}

pub fn validate_replay_frame_equivalence(frame_hashes_a: &[u64], frame_hashes_b: &[u64]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    if frame_hashes_a.len() != frame_hashes_b.len() {
        return Err(vec![DeterministicInvariantViolation {
            invariant_name: "replay_length_mismatch",
            subsystem: "runtime_governance",
        }]);
    }
    for i in 0..frame_hashes_a.len() {
        if frame_hashes_a[i] != frame_hashes_b[i] {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "replay_frame_mismatch",
                subsystem: "runtime_governance",
            }]);
        }
    }
    Ok(())
}

pub fn validate_canonical_tick_progression(ticks: &[u64]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    for i in 1..ticks.len() {
        if ticks[i - 1] >= ticks[i] {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "tick_regression_or_stagnation",
                subsystem: "runtime_governance",
            }]);
        }
    }
    Ok(())
}

pub fn validate_runtime_pipeline_integrity(sequence_indices: &[u64], phases: &[u8]) -> Result<(), Vec<DeterministicInvariantViolation>> {
    if sequence_indices.len() != phases.len() {
        return Err(vec![DeterministicInvariantViolation {
            invariant_name: "pipeline_mismatched_lengths",
            subsystem: "runtime_governance",
        }]);
    }
    for i in 1..sequence_indices.len() {
        let prev_idx = sequence_indices[i - 1];
        let curr_idx = sequence_indices[i];
        let prev_phase = phases[i - 1];
        let curr_phase = phases[i];

        if prev_idx > curr_idx {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "pipeline_sequence_not_sorted",
                subsystem: "runtime_governance",
            }]);
        } else if prev_idx == curr_idx {
            if prev_phase > curr_phase {
                return Err(vec![DeterministicInvariantViolation {
                    invariant_name: "pipeline_phase_not_sorted",
                    subsystem: "runtime_governance",
                }]);
            }
        }
    }
    Ok(())
}

pub fn validate_numeric_determinism(
    values: &[f32],
    policy: &crate::numerics::CanonicalFloatPolicy,
) -> Result<(), Vec<DeterministicInvariantViolation>> {
    for &val in values {
        if val.is_nan() || val.is_infinite() {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "non_finite_float_detected",
                subsystem: "numeric_determinism",
            }]);
        }
        if val < policy.clamp_min || val > policy.clamp_max {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "value_out_of_bounds",
                subsystem: "numeric_determinism",
            }]);
        }
        if let Ok(canonicalized) = crate::numerics::canonicalize_f32(val, policy, crate::numerics::FloatNormalizationMode::ClampOnly) {
            if (val - canonicalized).abs() > f32::EPSILON {
                return Err(vec![DeterministicInvariantViolation {
                    invariant_name: "non_canonical_float_value",
                    subsystem: "numeric_determinism",
                }]);
            }
        } else {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "canonicalization_failed",
                subsystem: "numeric_determinism",
            }]);
        }
    }
    Ok(())
}

pub fn validate_replay_exactness(
    replay_bytes_a: &[u8],
    replay_bytes_b: &[u8],
) -> Result<(), Vec<DeterministicInvariantViolation>> {
    if replay_bytes_a != replay_bytes_b {
        return Err(vec![DeterministicInvariantViolation {
            invariant_name: "replay_byte_mismatch",
            subsystem: "replay_exactness",
        }]);
    }
    Ok(())
}

pub fn validate_platform_compatibility(
    a: &crate::platform_drift::PlatformDeterminismSignature,
    b: &crate::platform_drift::PlatformDeterminismSignature,
) -> Result<(), Vec<DeterministicInvariantViolation>> {
    let report = crate::platform_drift::verify_platform_compatibility(a, b);
    if report.drift_detected {
        return Err(vec![DeterministicInvariantViolation {
            invariant_name: "platform_signature_drift",
            subsystem: "platform_drift_governance",
        }]);
    }
    Ok(())
}

pub fn validate_stable_accumulation(
    sequence_indices: &[u64],
) -> Result<(), Vec<DeterministicInvariantViolation>> {
    for i in 1..sequence_indices.len() {
        if sequence_indices[i - 1] > sequence_indices[i] {
            return Err(vec![DeterministicInvariantViolation {
                invariant_name: "accumulation_order_violation",
                subsystem: "stable_accumulation",
            }]);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_invalid_phase_order() {
        let bad_phases = vec![0, 2, 1, 3, 4, 5, 6, 7, 8];
        assert!(validate_runtime_phase_ordering(&bad_phases).is_err());

        let short_phases = vec![0, 1, 2];
        assert!(validate_runtime_phase_ordering(&short_phases).is_err());

        let good_phases = vec![0, 1, 2, 3, 4, 5, 6, 7, 8];
        assert!(validate_runtime_phase_ordering(&good_phases).is_ok());
    }

    #[test]
    fn rejects_duplicate_frame_sequence() {
        let seqs = vec![1, 1, 2];
        let ticks = vec![100, 100, 200];
        assert!(validate_runtime_frame_sequence(&seqs, &ticks).is_err());

        let good_seqs = vec![1, 1, 2];
        let good_ticks = vec![100, 200, 300];
        assert!(validate_runtime_frame_sequence(&good_seqs, &good_ticks).is_ok());
    }

    #[test]
    fn rejects_epoch_regression() {
        let bad_epochs = vec![1, 2, 1];
        assert!(validate_runtime_epoch_progression(&bad_epochs).is_err());

        let good_epochs = vec![1, 2, 2, 3];
        assert!(validate_runtime_epoch_progression(&good_epochs).is_ok());
    }

    #[test]
    fn accepts_valid_runtime_pipeline() {
        let indices = vec![0, 1, 1, 2];
        let phases = vec![0, 0, 1, 0];
        assert!(validate_runtime_pipeline_integrity(&indices, &phases).is_ok());

        let bad_indices = vec![1, 0];
        let bad_phases = vec![0, 0];
        assert!(validate_runtime_pipeline_integrity(&bad_indices, &bad_phases).is_err());
    }

    #[test]
    fn replay_equivalence_validation() {
        let hashes_a = vec![10, 20, 30];
        let hashes_b = vec![10, 20, 30];
        assert!(validate_replay_frame_equivalence(&hashes_a, &hashes_b).is_ok());

        let hashes_c = vec![10, 25, 30];
        assert!(validate_replay_frame_equivalence(&hashes_a, &hashes_c).is_err());
    }

    #[test]
    fn test_validate_numeric_determinism() {
        use crate::numerics::CanonicalFloatPolicy;
        let policy = CanonicalFloatPolicy {
            epsilon: 1e-5,
            clamp_min: 0.0,
            clamp_max: 10.0,
            normalization_epsilon: 1e-5,
            deterministic_rounding_precision: 2,
        };

        // Canonical float
        let good_vals = vec![1.23, 4.56, 7.89];
        assert!(validate_numeric_determinism(&good_vals, &policy).is_ok());

        // Out of bounds
        let bad_vals = vec![1.23, 11.0, 7.89];
        assert!(validate_numeric_determinism(&bad_vals, &policy).is_err());

        // NaN
        let nan_vals = vec![1.23, f32::NAN, 7.89];
        assert!(validate_numeric_determinism(&nan_vals, &policy).is_err());

        // Non-canonical float (not rounded to 2 decimal places)
        let unrounded_vals = vec![1.234, 4.56, 7.89];
        assert!(validate_numeric_determinism(&unrounded_vals, &policy).is_err());
    }

    #[test]
    fn test_validate_replay_exactness() {
        let bytes_a = vec![1, 2, 3, 4];
        let bytes_b = vec![1, 2, 3, 4];
        let bytes_c = vec![1, 2, 3, 5];

        assert!(validate_replay_exactness(&bytes_a, &bytes_b).is_ok());
        assert!(validate_replay_exactness(&bytes_a, &bytes_c).is_err());
    }

    #[test]
    fn test_validate_platform_compatibility() {
        use crate::platform_drift::PlatformDeterminismSignature;
        let sig_a = PlatformDeterminismSignature {
            architecture: "x86_64".to_string(),
            compiler_version: "rustc-1.78.0".to_string(),
            float_policy_hash: 10,
            simd_policy_hash: 20,
            runtime_policy_hash: 30,
        };
        let sig_b = sig_a.clone();
        let sig_c = PlatformDeterminismSignature {
            architecture: "aarch64".to_string(),
            ..sig_a.clone()
        };

        assert!(validate_platform_compatibility(&sig_a, &sig_b).is_ok());
        assert!(validate_platform_compatibility(&sig_a, &sig_c).is_err());
    }

    #[test]
    fn test_validate_stable_accumulation() {
        let good_seqs = vec![0, 1, 1, 2, 3];
        let bad_seqs = vec![0, 1, 2, 1, 3];

        assert!(validate_stable_accumulation(&good_seqs).is_ok());
        assert!(validate_stable_accumulation(&bad_seqs).is_err());
    }
}
