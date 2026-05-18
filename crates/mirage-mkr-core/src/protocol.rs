// ===================================================================
// mirage-mkr-core/src/protocol.rs  (V3 — Federated Stabilization Pass)
// PURPOSE: Runtime Protocol Descriptors — Federated Communication Layer
//
// ---------------------------------------------------------------
// DESIGN INTENT
// ---------------------------------------------------------------
//
// This module defines lightweight, compatibility-safe protocol types
// that carry runtime information between subsystems WITHOUT creating
// direct crate coupling.
//
// These are NOT:
//   * ECS components
//   * orchestration systems
//   * message queues
//   * scheduler instructions
//
// They ARE:
//   * Plain data descriptors (Copy types where possible)
//   * Translation outputs from MKR's authority fields
//   * Protocol boundaries between federated subsystems
//
// ---------------------------------------------------------------
// PROTOCOL ARCHITECTURE
// ---------------------------------------------------------------
//
// MKR produces:          ActivationDescriptor, StreamingEligibility,
//                        ExecutionDescriptor, RuntimeSignal
//
// OASIS consumes:        StreamingEligibility (to decide stream work)
//
// Executor consumes:     ExecutionDescriptor (via ExecutionBridge)
//
// Renderer consumes:     ActivationDescriptor (via RendererBridge)
//                        ResidencyDescriptor  (future)
//
// ---------------------------------------------------------------
// FUTURE PROTOCOL DIRECTION
// ---------------------------------------------------------------
//
// TODO(V3-PROTOCOL): ActivationDescriptor should replace the raw
//   `execution_probability` f32 passed between MKR and renderer.
//   Wrap it in a typed descriptor so consumers cannot misuse it
//   as a scheduling authority input.
//
// TODO(V3-PROTOCOL): StreamingEligibility should replace the raw
//   StreamingDecision as the cross-crate boundary type.  OASIS
//   receives StreamingEligibility; it produces its own StreamRequest.
//
// TODO(V3-PROTOCOL): ResidencyDescriptor should replace raw chunk_idx
//   in OASIS and renderer residency APIs.  It unifies field-index
//   and page-address into one typed protocol value.
//
// TODO(V3-CEK): When CEK is implemented, RuntimeSignal will carry
//   CEK continuation identifiers alongside probability values.
// ===================================================================

// =====================================================================
// ACTIVATION DESCRIPTOR
// =====================================================================

/// Snapshot of a single cell's activation state, suitable for passing
/// across subsystem boundaries without exposing the raw ActivationCell.
///
/// Produced by MKR; consumed by renderer (passive) and future CEK.
///
/// # V3 Design
/// This descriptor is intentionally minimal — it carries only what
/// downstream subsystems need.  Adding fields here requires justification
/// from the consuming subsystem, not from MKR's internal needs.
#[derive(Debug, Clone, Copy)]
pub struct ActivationDescriptor {
    /// Flat field cell index (== chunk index for 1:1 grids).
    pub cell_index: usize,
    /// Continuous execution probability in [0.0, 1.0].
    /// Derived from ActivationField::execution_probability.
    pub execution_probability: f32,
    /// Mean activation of this cell (heat × 0.55 + pressure × 0.35 + entropy × 0.10).
    /// Provided for downstream display; NOT a scheduling authority input.
    pub activation: f32,
}

// =====================================================================
// STREAMING ELIGIBILITY
// =====================================================================

/// MKR's streaming eligibility signal for a single cell.
///
/// Produced by StreamingCoordinator; consumed by OASIS (streaming execution).
///
/// # Ownership Contract
/// MKR produces eligibility.  OASIS executes.
/// The bridge that converts this to an OASIS StreamRequest is future work.
///
/// TODO(V3-OASIS-CANONICAL): Add conversion: StreamingEligibility → StreamRequest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingEligibility {
    /// Cell is below all streaming thresholds — no streaming needed.
    None,
    /// Cell is above the prefetch threshold — begin async background load.
    PrefetchEligible,
    /// Cell is above the resident threshold — promote to VRAM immediately.
    ResidentEligible,
}

impl StreamingEligibility {
    /// Classify a probability value into a streaming eligibility tier.
    ///
    /// Uses the same thresholds as StreamingCoordinator.
    pub fn classify(probability: f32) -> Self {
        use crate::streaming::{STREAM_PREFETCH_THRESHOLD, STREAM_RESIDENT_THRESHOLD};
        if probability >= STREAM_RESIDENT_THRESHOLD {
            StreamingEligibility::ResidentEligible
        } else if probability >= STREAM_PREFETCH_THRESHOLD {
            StreamingEligibility::PrefetchEligible
        } else {
            StreamingEligibility::None
        }
    }
}

// =====================================================================
// EXECUTION DESCRIPTOR
// =====================================================================

/// Protocol descriptor for passing execution intent to the executor.
///
/// Produced by ExecutionBridge; consumed by executor (passive backend).
///
/// # V3 Design
/// This type replaces ChunkTask::state: ChunkState as the primary
/// scheduling input.  The executor MUST NOT re-derive priority from
/// thermal state — it must accept the priority as given here.
///
/// TODO(V3-EXECUTOR-PASSIVE): Wire executor to accept ExecutionDescriptor
/// slices from ExecutionBridge instead of scheduling from ThermalSystem.
#[derive(Debug, Clone, Copy)]
pub struct ExecutionDescriptor {
    /// Flat field cell index (== chunk index for 1:1 grids).
    pub cell_index: usize,
    /// Pre-computed priority in [0.0, 1.0].
    /// Derived from execution_probability — executor must NOT recompute this.
    pub priority: f32,
    /// Frame by which this descriptor expires.
    pub deadline_frame: u64,
    /// Whether this is a prefetch hint (low priority, no fiber spawn yet).
    pub is_prefetch_hint: bool,
}

// =====================================================================
// RESIDENCY DESCRIPTOR
// =====================================================================

/// V3 residency address: pairs a field cell index with an OASIS page ref.
///
/// This replaces raw (chunk_idx: u32, page_id: u32) pairs in all residency APIs.
///
/// # Ownership
/// MKR produces ResidencyDescriptors from field-handle conversions.
/// OASIS owns the residency lifecycle (load, evict, promote).
/// Renderer consumes read-only residency queries.
///
/// TODO(V3-RENDERER-PASSIVE): Replace ResidencyTracker::request_load(u32)
/// with request_load(ResidencyDescriptor) so the OASIS key space and
/// field key space are unified under one typed address.
#[derive(Debug, Clone, Copy)]
pub struct ResidencyDescriptor {
    /// V3 primary key — indexes ActivationField::cells directly.
    pub cell_index: usize,
    /// OASIS virtual page containing this chunk's data.
    pub oasis_page_id: u32,
}

// =====================================================================
// RUNTIME SIGNAL
// =====================================================================

/// Lightweight signal carrying a MKR runtime event to downstream subsystems.
///
/// RuntimeSignals are edge-triggered events — they fire once per condition
/// crossing, not continuously every tick.
///
/// TODO(V3-CEK): When CEK is implemented, RuntimeSignal will carry
///   a continuation_id so CEK can decide which continuation to launch.
#[derive(Debug, Clone, Copy)]
pub enum RuntimeSignal {
    /// A cell's execution_probability crossed the emission gate.
    EmissionThresholdCrossed { cell_index: usize, probability: f32 },
    /// A streaming operation completed — inject heat feedback.
    StreamCompletionFeedback { cell_index: usize, heat_amount: f32 },
    /// A topology edge weight changed — pressure re-propagation needed.
    TopologyWeightChanged { from_node: usize, to_node: usize, new_pull: f32 },
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_eligibility_none_below_prefetch() {
        // 0.01 < STREAM_PREFETCH_THRESHOLD (0.03)
        let e = StreamingEligibility::classify(0.01);
        assert_eq!(e, StreamingEligibility::None);
    }

    #[test]
    fn streaming_eligibility_prefetch_in_range() {
        // 0.05 >= PREFETCH_THRESHOLD but < RESIDENT_THRESHOLD (0.35)
        let e = StreamingEligibility::classify(0.05);
        assert_eq!(e, StreamingEligibility::PrefetchEligible);
    }

    #[test]
    fn streaming_eligibility_resident_above_threshold() {
        // 0.40 >= RESIDENT_THRESHOLD (0.35)
        let e = StreamingEligibility::classify(0.40);
        assert_eq!(e, StreamingEligibility::ResidentEligible);
    }

    #[test]
    fn activation_descriptor_is_copy() {
        let d = ActivationDescriptor { cell_index: 3, execution_probability: 0.7, activation: 0.6 };
        let d2 = d; // must be Copy
        assert_eq!(d2.cell_index, 3);
    }

    #[test]
    fn execution_descriptor_is_copy() {
        let d = ExecutionDescriptor {
            cell_index: 5, priority: 0.8, deadline_frame: 100, is_prefetch_hint: false
        };
        let d2 = d;
        assert_eq!(d2.cell_index, 5);
    }
}
