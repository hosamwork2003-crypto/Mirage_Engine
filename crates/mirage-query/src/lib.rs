// ===================================================================
// mirage-query/src/lib.rs
// PURPOSE: Relational Query IR — Layer 2 CellQuery Substrate
//
// DESIGN INTENT
// ---------------------------------------------------------------
// The Query IR provides a data-oriented, declarative API over the
// activation field. Instead of raw imperative loops, callers build
// lazy query pipelines that the TraceFusionCompiler can recognize
// and fuse into optimized SIMD execution blocks.
//
// EXECUTION MODEL
// ---------------------------------------------------------------
// 1. ColumnarScan: SoA (Structure of Arrays) primary execution backend.
//    Stores each cell attribute (heat, pressure, entropy, activation,
//    execution_probability) in separate contiguous slices. This
//    maximises SIMD auto-vectorization by keeping like-typed data adjacent.
//
// 2. CellQuery: The fluent query builder. Supports:
//    - filter(predicate): Lazily mark cells matching a condition.
//    - map(transform):    Apply a mutation to each selected cell.
//    - collect():         Harvest matching cell indices.
//    - apply(kernel):     Execute a SolverKernel over selected cells.
//
// 3. SolverKernel: An abstraction for field mutation passes. The
//    TraceFusionCompiler pattern-matches on kernel signatures to
//    recognize hot paths and fuse them across frames.
//
// PARITY GUARANTEE
// ---------------------------------------------------------------
// Every query path MUST produce bit-identical results to the
// equivalent procedural loop in the original ActivationSolver.
// Verified by the parity tests in this crate.
// ===================================================================

pub mod columnar;
pub mod query;
pub mod kernel;

pub use columnar::ColumnarScan;
pub use query::{CellQuery, CellView, CellViewMut};
pub use kernel::{SolverKernel, KernelId};
