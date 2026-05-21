//! Mirage Morphogenic — V4.4
//! Deterministic structural continuity substrate.
//! Immutable realization sequences, provenance, continuity snapshots,
//! replay-safe realization applier. No topology or execution authority.

pub mod continuity;
pub mod state;
pub mod propagation;

pub use continuity::{ContinuitySnapshot, ContinuityEpoch, StructuralContinuityField};
pub use state::{StructuralState, StructuralProvenance};
pub use propagation::{
    MorphogenicLaneId, MorphogenicLane,
    StructuralPropagationDescriptor, StructuralPropagationSequence,
    MorphogenicRealizer, StructuralRealizationFrame, MorphogenicRealizationSequence,
};