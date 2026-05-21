// ===================================================================
// mirage-core/src/lib.rs
// PURPOSE: Core Runtime Primitives — Foundation Layer
//
// V6 OWNERSHIP DECLARATION:
// ---------------------------------------------------------------
// OWNS:
//   * ChunkState, ChunkThermals, ThermalSystem  — chunk lifecycle primitives
//   * RuntimeDirectory, Handle, AddressMapping   — handle/address primitives
//   * RuntimeAuthorityDomain, AuthorityBounded   — governance primitives
//   * DeterministicContract, DeterministicValidated — contract markers
//   * DeterministicInvariantViolation            — invariant error type
//   * RuntimeValidatable                         — validation trait
//   * MirageWorld                                — top-level world container
//   * validation validators & helpers            — V6 primitive-based governance validation
//   * contracts & guarantees                     — V6 runtime execution contracts
//
// V6.5 NUMERICAL DETERMINISM & GOVERNANCE RULES:
// ---------------------------------------------------------------
//   * CanonicalFloatPolicy: enforces strict rounding precision, clamping, and normalizes floats to prevent architecture drift.
//   * PlatformDriftReport: checks compiler version and CPU architecture to identify hardware-level variance.
//   * Pure deterministic rounding and clamping helpers that reject NaN and Infinity.
//   * Forbidden: async runtime, background threads, unordered iteration, and platform-specific SIMD fast-math.
//
// MUST NOT OWN:
//   * topology                   — owned by mirage-mts
//   * continuity / emergence     — owned by mirage-morphogenic
//   * orchestration / runtime    — owned by mirage-mkr-core
//
// DETERMINISTIC & REPLAY GUARANTEES:
//   * Pure validators with zero side-effects and zero non-deterministic allocations.
//   * Immutable execution contract guarantees.
//
// DEPENDENCY DIRECTION:
//   mirage-core is the foundation. Nothing in mirage-core may depend
//   on mirage-mts, mirage-morphogenic, or mirage-mkr-core.
//   Allowed deps: mirage-geometry, mirage-math, serde, uuid, bytemuck.
// ===================================================================

pub mod pool;
pub mod oasis;
pub mod runtime;
pub mod archetype;
pub mod continuation;

pub mod governance;
pub mod contracts;
pub mod invariants;
pub mod validation;
pub mod runtime_validation;
pub mod runtime_contracts;
pub mod numerics;
pub mod platform_drift;
pub mod spatial_validation;
pub mod world_contracts;



pub use runtime::{ChunkState, ChunkThermals, ThermalSystem};
pub use governance::{RuntimeAuthorityDomain, AuthorityBounded, AuthorityBoundaryViolation};
pub use contracts::{DeterministicContract, DeterministicValidated};
pub use invariants::DeterministicInvariantViolation;
pub use validation::{
    RuntimeValidatable,
    canonicalize_sequence_indices,
    validate_topology_descriptor_ordering,
    validate_stable_index_order,
    validate_replay_equivalence_frames,
    validate_replay_equivalence_f32,
    validate_resonance_replay_equivalence,
    validate_history_replay_equivalence,
    validate_continuity_equivalence,
    validate_emergence_bounds,
    validate_resonance_sequence,
    validate_convergence_state,
    validate_runtime_frame_ordering,
    validate_runtime_tick_monotonicity,
    canonicalize_runtime_frames,
};
pub use runtime_validation::{
    validate_runtime_phase_ordering,
    validate_runtime_frame_sequence,
    validate_runtime_epoch_progression,
    validate_replay_frame_equivalence,
    validate_canonical_tick_progression,
    validate_runtime_pipeline_integrity,
    validate_numeric_determinism,
    validate_replay_exactness,
    validate_platform_compatibility,
    validate_stable_accumulation,
};
pub use runtime_contracts::{
    ForbiddenRuntimeAuthority,
    DeterministicGuarantee,
    RuntimeExecutionContract,
};
pub use numerics::{
    CanonicalFloatPolicy, FloatNormalizationMode,
    canonicalize_f32, canonicalize_f64,
    stable_add_f32, stable_add_f64,
    stable_mul_f32, stable_mul_f64,
    stable_normalize_f32, stable_normalize_f64,
    hash_bytes, hash_u64,
};
pub use platform_drift::{
    PlatformDeterminismSignature, PlatformDriftReport, RuntimeDeterminismSeal,
    compute_platform_signature, verify_platform_compatibility,
    verify_runtime_policy_compatibility, hash_float_policy, hash_simd_policy,
};
pub use spatial_validation::{
    validate_region_ordering, validate_region_monotonicity,
    validate_streaming_sequence, validate_residency_ordering,
    validate_spatial_replay_equivalence, validate_world_snapshot_equivalence,
    validate_region_transition_order, validate_canonical_region_graph,
};
pub use world_contracts::{
    StructuralWorldContract, RuntimeSpatialContract, ForbiddenSpatialAuthority,
};


use mirage_geometry::columnar::ColumnarPage;

/// Maximum number of entities in the world.
pub const NUM_ENTITIES: u32 = 10_000;

pub struct MirageWorld {
    pub positions: ColumnarPage<[f32; 4]>,
    pub colors: ColumnarPage<[f32; 4]>,
}

impl Default for MirageWorld {
    fn default() -> Self {
        Self::new()
    }
}

impl MirageWorld {
    pub fn new() -> Self {
        Self {
            positions: ColumnarPage::new(NUM_ENTITIES as usize),
            colors: ColumnarPage::new(NUM_ENTITIES as usize),
        }
    }

    /// Collect dirty entity positions and colors as (index, position, color) tuples.
    /// Clears dirty bits after collection (reactive delta pattern).
    pub fn collect_deltas(&mut self) -> Vec<(u32, [f32; 4], [f32; 4])> {
        let mut deltas = Vec::new();

        let dirty_indices: Vec<usize> = self.positions.dirty_tracker.iter_dirty().collect();

        for index in dirty_indices {
            deltas.push((
                index as u32,
                self.positions.data[index],
                self.colors.data[index],
            ));
        }

        self.positions.dirty_tracker.clear();
        self.colors.dirty_tracker.clear();

        deltas
    }
}