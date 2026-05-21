//! mirage-morphogenic — V6 Deterministic Structural Continuity Substrate
//!
//! V6 OWNERSHIP DECLARATION:
//! ---------------------------------------------------------------
//! OWNS:
//!   * ContinuitySnapshot, StructuralContinuityField  — continuity state
//!   * StructuralPropagationSequence, MorphogenicLaneId — propagation descriptors
//!   * ReinforcementMemoryField                        — reinforcement substrate
//!   * StructuralPersistenceField                      — persistence substrate
//!   * ContinuityDiff, ContinuityDiffSequence          — diff substrate
//!   * StructuralReplaySnapshot, StructuralReplayBuffer — replay substrate
//!   * StructuralEmergenceState                        — emergence substrate
//!   * EmergenceResonanceField, ResonancePropagationSequence — resonance
//!   * EmergenceThresholdDescriptor, StructuralConvergenceState — convergence
//!   * EmergenceRealizationFrame, EmergenceHistoryBuffer — history
//!   * StructuralRuntimeFrame, RuntimeFrameSequence, RuntimeFrameIdentity — V6 runtime frames
//!   * RuntimeReplayBuffer, RuntimeReplaySnapshot — V6 replay governance
//!
//! MUST NOT OWN:
//!   * topology                   — owned by mirage-mts
//!   * execution authority        — owned by mirage-mkr-core
//!   * orchestration              — owned by mirage-mkr-core
//!
//! DETERMINISTIC & REPLAY GUARANTEES:
//!   * All propagation sequences are immutable after construction.
//!   * All descriptors have stable, non-decreasing sequence indices.
//!   * No hash-order or allocator-order dependence.
//!   * Replay equivalence: same input produces same sequence output.
//!   * Stable Accumulation: Insertion order is preserved and sequence index stable sorting is enforced.
//!   * Replay Exactness: Strict byte equivalence mode checks raw LE serializations without epsilon comparison.
//!
//! DEPENDENCY DIRECTION:
//!   mirage-core -> mirage-morphogenic
//!   mirage-morphogenic does NOT depend on mirage-mts or mirage-mkr-core.
//!   Allowed deps: mirage-geometry, mirage-math, serde, uuid, bytemuck.

pub mod continuity;
pub mod state;
pub mod propagation;
pub mod reinforcement;
pub mod persistence;
pub mod diff;
pub mod replay;
pub mod emergence;
pub mod resonance;
pub mod convergence;
pub mod history;
pub mod runtime_frame;
pub mod runtime_replay;
pub mod accumulation;
pub mod replay_exactness;
pub mod canonical_serialization;
pub mod spatial_continuity;
pub mod world_snapshot;
pub mod region_replay;


pub use continuity::{ContinuitySnapshot, ContinuityEpoch, StructuralContinuityField, ContinuityLifecycleState, ManagedContinuitySnapshot, SnapshotIdentity};
pub use state::{StructuralState, StructuralProvenance, StructuralPressureState};
pub use propagation::{
    MorphogenicLaneId, MorphogenicLane,
    StructuralPropagationDescriptor, StructuralPropagationSequence,
    MorphogenicRealizer, StructuralRealizationFrame, MorphogenicRealizationSequence,
    DeterministicDecayDescriptor, DeterministicDecaySequence, StructuralHistoryBuffer, apply_decay_sequence,
};
pub use reinforcement::{ReinforcementMemoryField, ReinforcementMemoryCell};
pub use persistence::StructuralPersistenceField;
pub use diff::{ContinuityDiff, ContinuityDiffSequence};
pub use replay::{StructuralReplaySnapshot, StructuralReplayFrame, StructuralReplayBuffer};
pub use emergence::{StructuralEmergenceState};
pub use resonance::{EmergenceResonanceField, EmergenceResonanceSnapshot, ResonancePropagationSequence, ResonancePropagationDescriptor, EmergenceProvenance};
pub use convergence::{EmergenceThresholdDescriptor, StructuralConvergenceState};
pub use history::{EmergenceRealizationFrame, EmergenceRealizationSequence, EmergenceHistoryBuffer};
pub use runtime_frame::{StructuralRuntimeFrame, RuntimeFrameIdentity, RuntimeFrameSequence};
pub use runtime_replay::{RuntimeReplayBuffer, RuntimeReplaySnapshot};
pub use accumulation::{CanonicalAccumulatorF32, CanonicalAccumulatorF64};
pub use replay_exactness::{
    StrictReplayMode, ReplayExactnessReport,
    verify_byte_replay_equivalence, verify_runtime_frame_exactness,
    verify_snapshot_exactness,
};
pub use canonical_serialization::{
    canonicalize_runtime_frame_bytes, canonicalize_snapshot_bytes,
    canonicalize_replay_bytes,
};
pub use spatial_continuity::{
    SpatialContinuityState, SpatialContinuityField,
    SpatialContinuityPropagationDescriptor, SpatialContinuitySequence,
};
pub use world_snapshot::{
    WorldSnapshotIdentity, StructuralWorldSnapshot, StructuralWorldFrame,
};
pub use region_replay::{
    RegionReplayFrame, RegionReplayBuffer, RegionReplaySnapshot,
};