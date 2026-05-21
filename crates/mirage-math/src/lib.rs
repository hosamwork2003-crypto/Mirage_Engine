// ===================================================================
// mirage-math/src/lib.rs
// PURPOSE: Mathematical and SIMD operations layer.
//
// V6.5 SIMD GOVERNANCE RULES & GUARANTEES:
// ---------------------------------------------------------------
// * All SIMD reductions must reduce in canonical lane ordering.
// * Nondeterministic lane merges are strictly forbidden.
// * Stable accumulation order is preserved using 4-lane independent Kahan sums.
// * SIMD outputs must produce scalar-equivalent results under tolerance.
// ===================================================================

pub mod simd;
pub mod batch;
pub mod differential;
pub mod fused;
pub mod transform;
pub mod deterministic_simd;

pub use deterministic_simd::{
    DeterministicSimdPolicy,
    SimdExecutionMode,
    verify_scalar_simd_equivalence,
    stable_simd_reduce_f32,
    stable_simd_reduce_f64,
};