// ===================================================================
// mirage-mkr-core/src/bridge/mod.rs  (V3 — Federated Stabilization Pass)
// PURPOSE: Bridge module root — PROTOCOL/TRANSLATION layers only
//
// ---------------------------------------------------------------
// BRIDGE OWNERSHIP INVARIANT (CRITICAL)
// ---------------------------------------------------------------
// Every bridge in this module MUST satisfy:
//
//   INPUT:  owned by a canonical subsystem (ActivationField, OASIS, etc.)
//   OUTPUT: protocol descriptor consumed by the target subsystem
//
// Bridges MUST NOT:
//   * Own runtime state (no heap-persistent scheduling queues)
//   * Make execution eligibility decisions (that is EmissionGate)
//   * Own streaming lifecycle (that is OASIS/StreamingFabric)
//   * Own residency truth (that is ResidencyTracker in renderer)
//   * Spawn threads or fibers (future: FiberPool will do this)
//
// ---------------------------------------------------------------
// CURRENT BRIDGES
// ---------------------------------------------------------------
//
// renderer_bridge  — ActivationField → RuntimeDirectory::chunk_runtime_states
//                    (continuous probability → discrete ChunkState)
//                    STATUS: Stable. Translation-only. No state owned.
//
// renderer_validation — Shadow validation layer for sparse renderer path.
//                        Compares apply_changed_cells() against
//                        apply_to_directory().
//                        STATUS: Non-authoritative validation only.
//
// execution_bridge — EmissionRequest → SchedulingRequest
//                    (emission layer output → executor-compatible descriptor)
//                    STATUS: Stable. Stateless struct. Pure translation.
//
// ---------------------------------------------------------------
// PLANNED BRIDGES (not yet wired)
// ---------------------------------------------------------------
//
// TODO(V3-BRIDGE-STREAMING): streaming_bridge
//   Translate StreamingDecision → OASIS StreamRequest.
//   MKR produces StreamingDecisions (eligibility only).
//   OASIS executes them (lifecycle only).
//   The bridge does NOT execute the stream itself.
//
// TODO(V3-BRIDGE-PHYSICS): physics_bridge
//   Translate execution_probability → simulation_factor per chunk.
//   Physics reads simulation_factor; field is the authority.
//
// TODO(V3-BRIDGE-RESIDENCY): residency_bridge
//   Translate ActivationField probability → ResidencyDescriptor.
//   Renderer's ResidencyTracker consumes descriptors passively.
//   ResidencyTracker MUST NOT infer thermal authority independently.
// ===================================================================

pub mod renderer_bridge;
pub mod renderer_validation;
pub mod execution_bridge;

pub use renderer_bridge::RendererBridge;
pub use renderer_bridge::probability_to_chunk_state;

pub use renderer_validation::{
    RendererParityReport,
    RendererShadowValidator,
};

pub use execution_bridge::{
    ExecutionBridge,
    SchedulingRequest,
    DEFAULT_DEADLINE_FRAMES,
};