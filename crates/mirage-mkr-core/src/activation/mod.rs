// ===================================================================
// mirage-mkr-core/src/activation/mod.rs
// PURPOSE: MKR Activation Field System — V3 Execution Foundation
//
// ARCHITECTURE:
// The activation system replaces discrete chunk-state orchestration
// with a continuous field-based execution model. Each cell in the
// ActivationField accumulates heat, pressure, and entropy from the
// surrounding environment. The solver propagates these values across
// the field every tick. Execution probability is derived continuously
// — no discrete state transitions, no threshold-only gates.
//
// V3 DESIGN PRINCIPLES:
// * Continuous activation values, not enum-based state machines.
// * Chunk-native memory layout (SoA-compatible, contiguous).
// * Branchless inner loops — SIMD auto-vectorization friendly.
// * Designed for future GPU compute migration.
// * CEK (field computation kernel) integration point is explicit.
// ===================================================================

pub mod field;
pub mod solver;
pub mod weights;
pub mod delta;      // V3-DIFFERENTIAL: field delta tracking
pub mod frontier;   // V3-DIFFERENTIAL: sparse propagation frontier
pub mod sparse;     // V3-SPARSE: frontier-local solver passes
pub mod validation; // V3-SPARSE: parity comparison infrastructure

pub use field::{ActivationCell, ActivationField};
pub use solver::ActivationSolver;
pub use weights::ExecutionWeights;
pub use delta::{FieldDeltaTracker, FieldDeltaMask, CellChangeFlags};
pub use frontier::PropagationFrontier;
pub use sparse::{step_sparse, SparseSolverResult, SPARSE_DIVERGENCE_EPSILON};
pub use validation::{
    SparseValidationRunner, ValidationMode,
    ParityComparisonResult, FrontierValidationReport,
};
