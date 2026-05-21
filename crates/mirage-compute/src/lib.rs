// ===================================================================
// mirage-compute/src/lib.rs
// PURPOSE: Execution Mathematics Substrate — Trace Fusion Compiler (TFC)
//
// ===================================================================
// AUTHORITY BOUNDARY DECLARATION (V4 Stabilization Pass)
// ===================================================================
//
// mirage-compute IS:
//   * execution mathematics
//   * fused execution kernels
//   * SIMD propagation math
//   * frontier execution kernels
//   * branchless execution operators
//   * differential trace infrastructure
//   * sparse propagation windows
//
// mirage-compute MUST NOT become:
//   * scheduler
//   * executor
//   * continuation owner
//   * runtime authority
//   * orchestration layer
//   * execution governor
//   * fiber authority
//   * activation authority
//   * topology authority
//
// EXECUTION OWNERSHIP RULE:
//   Compute kernels MAY prepare execution transforms.
//   Compute kernels MUST NOT directly own:
//     - fiber spawning
//     - scheduling
//     - continuation lifecycle
//     - execution orchestration
//
//   Execution ownership remains EXTERNAL to this crate.
//   The caller (MKR orchestrator) drives kernel application.
//
// DEPENDENCY DIRECTION:
//   mirage-compute has ZERO mirage-* crate dependencies.
//   It is a pure mathematical substrate.
//   It must remain dependency-free to avoid authority leakage.
//
// ===================================================================
// MODULE ORGANIZATION (V4 Preparation)
// ===================================================================
//
// Current: monolithic lib.rs (Phase 1 — all types inline)
//
// Future modular layout (Task 7 — do NOT restructure yet):
//
//   fusion/       — TraceFusionCompiler, FusedKernel, trace maturity
//   differential/ — DifferentialTrace, DeltaExecutionMask
//   frontier/     — FrontierWindow, FrontierExecutionSlice
//   simd/         — SIMD-aligned kernel execution loops
//   continuation/ — ComputeContinuation (data record only, NOT owner)
//   kernels/      — Branchless scalar/vector execution operators
//
// TODO(V4-COMPUTE-MODULAR): Split lib.rs into the above modules when
// any single section exceeds 250 lines. Use pub mod declarations here
// and re-export all public types at the crate root.
//
// ===================================================================

// ===================================================================
// SECTION 1 — COMPUTE CONTINUATION
// ===================================================================
//
// NOTE ON NAMING: `Continuation` in this crate is a DATA RECORD only.
// It is NOT the same as `mirage_cek::Continuation` (a heap closure).
//
// ComputeContinuation is:
//   * a lightweight (cell_index, prob_signal) pair
//   * used as a trace record in FusedKernel::path
//   * Copy-able, Clone-able, PartialEq-able — no heap
//   * executor-agnostic: does NOT know what runs it
//
// TODO(V4-COMPUTE-BOUNDARY): When CEK integration occurs, the
// FusedKernel may receive pre-computed ComputeContinuation records
// derived from CEK environment weights. The kernel applies them
// mathematically. CEK RETAINS ownership of the continuation lifecycle.
// mirage-compute NEVER owns CEK continuation semantics.

/// Lightweight execution trace record — a single cell index and its
/// probability signal, captured at emission time.
///
/// # Authority
/// This is a MATHEMATICAL DATA RECORD only. It carries no execution
/// authority, scheduling semantics, or lifecycle ownership.
/// The fiber/CEK machine that produced this record is NOT owned here.
///
/// # Naming Note
/// Named `Continuation` for backwards compatibility with existing
/// `mirage-mkr-core` code that references `mirage_compute::Continuation`.
/// Semantically this is a `ComputeTraceRecord`, not a continuation closure.
///
/// TODO(V4-COMPUTE-BOUNDARY): Rename to `ComputeTraceRecord` in a
/// future breaking-change pass once all callers are updated.
#[derive(Clone, Debug, PartialEq)]
pub struct Continuation {
    /// Flat field cell index this trace record targets.
    pub cell_index: usize,
    /// Emission probability signal at the time this record was captured.
    pub prob_signal: f32,
}

// ===================================================================
// SECTION 2 — CELL FIELD INTERFACE
// ===================================================================
//
// CellField is the abstract mathematical interface that fused kernels
// operate on. It is intentionally minimal.
//
// AUTHORITY: CellField is a MATHEMATICAL ACCESS INTERFACE.
//   * Implementors (e.g. ActivationField) retain all authority.
//   * mirage-compute only reads/writes through this interface.
//   * The kernel has no knowledge of the concrete field type.
//
// TODO(V4-COMPUTE-BOUNDARY): When differential kernels are introduced,
// add `fn is_changed(&self, index: usize) -> bool` to support
// delta-aware kernel execution without leaking field authority.

/// Minimal mathematical interface for fused kernel execution.
///
/// Implemented by concrete field types (e.g. `ActivationField`).
/// mirage-compute only sees this interface — never the concrete type.
///
/// # Authority
/// The implementor retains full field authority. This trait only
/// exposes the mathematical operations that kernels require.
pub trait CellField {
    fn set_execution_probability(&mut self, index: usize, prob: f32);
    fn get_execution_probability(&self, index: usize) -> f32;
    fn len(&self) -> usize;
}

// ===================================================================
// SECTION 3 — FUSED KERNEL
// ===================================================================
//
// FusedKernel is a MATHEMATICAL EXECUTION PRIMITIVE.
// It applies a pre-compiled trace to a field in a single
// fetch-decode-execute cycle with 4-wide loop unrolling for
// SIMD-friendly auto-vectorization.
//
// AUTHORITY BOUNDARY:
//   * FusedKernel::execute() is pure mathematics on the field.
//   * It does NOT schedule fibers.
//   * It does NOT own continuations.
//   * It does NOT decide execution eligibility.
//   * The CALLER (MKR orchestrator) decides WHEN to call execute().
//
// TODO(V4-COMPUTE-BOUNDARY): Add `execute_frontier` method that
// only applies kernel steps to cells within a FrontierWindow.
// This enables delta-driven SIMD execution without the kernel
// needing to know about the activation field's topology authority.
//
// TODO(V4-COMPUTE-SIMD): Replace the manual 4-wide unroll with
// explicit SIMD intrinsics (std::arch or wide crate) when the
// field layout is aligned to 32-byte boundaries.

/// Pre-compiled fused execution kernel over a trace path.
///
/// # Execution Model
/// A single `execute()` call evaluates all trace records in one
/// fetch-decode-execute pass. Loop-unrolled 4-wide for auto-vectorization.
///
/// # Authority
/// Execute is the mathematical authority. Scheduling authority belongs
/// to the MKR orchestrator that decides when to call `execute()`.
#[derive(Clone, Debug)]
pub struct FusedKernel {
    pub path: Vec<Continuation>,
}

impl FusedKernel {
    /// Evaluate all trace records in a single pass onto the target field.
    ///
    /// Applies `probability = current * 0.9 + signal * 0.1` per record.
    /// 4-wide unrolled for SIMD-like pipeline efficiency.
    ///
    /// # Authority
    /// Pure mathematics. The caller owns scheduling and lifecycle.
    pub fn execute<F: CellField>(&self, field: &mut F) {
        let len = self.path.len();
        let mut i = 0;

        // 4-wide loop unroll for auto-vectorization friendliness.
        // TODO(V4-COMPUTE-SIMD): Replace with explicit SIMD once field
        // layout alignment is guaranteed.
        while i + 4 <= len {
            let c0 = &self.path[i];
            let c1 = &self.path[i + 1];
            let c2 = &self.path[i + 2];
            let c3 = &self.path[i + 3];

            if c0.cell_index < field.len() {
                let p = field.get_execution_probability(c0.cell_index);
                field.set_execution_probability(c0.cell_index, (p * 0.9 + c0.prob_signal * 0.1).clamp(0.0, 1.0));
            }
            if c1.cell_index < field.len() {
                let p = field.get_execution_probability(c1.cell_index);
                field.set_execution_probability(c1.cell_index, (p * 0.9 + c1.prob_signal * 0.1).clamp(0.0, 1.0));
            }
            if c2.cell_index < field.len() {
                let p = field.get_execution_probability(c2.cell_index);
                field.set_execution_probability(c2.cell_index, (p * 0.9 + c2.prob_signal * 0.1).clamp(0.0, 1.0));
            }
            if c3.cell_index < field.len() {
                let p = field.get_execution_probability(c3.cell_index);
                field.set_execution_probability(c3.cell_index, (p * 0.9 + c3.prob_signal * 0.1).clamp(0.0, 1.0));
            }
            i += 4;
        }

        // Scalar tail for remainder.
        while i < len {
            let c = &self.path[i];
            if c.cell_index < field.len() {
                let p = field.get_execution_probability(c.cell_index);
                field.set_execution_probability(c.cell_index, (p * 0.9 + c.prob_signal * 0.1).clamp(0.0, 1.0));
            }
            i += 1;
        }
    }

    /// Execute only the trace records whose cell indices fall within
    /// the provided `FrontierWindow`.
    ///
    /// This is the frontier-aware execution path: the kernel processes
    /// only cells that are within the active propagation frontier,
    /// skipping dormant cells entirely.
    ///
    /// # Authority
    /// Purely mathematical — the FrontierWindow is a data filter.
    /// The kernel does NOT decide what constitutes a frontier;
    /// MKR provides that context externally.
    ///
    /// # Performance
    /// Avoids iterating the full path when only a sparse subset of cells
    /// are active. Designed for sparse differential execution workloads.
    ///
    /// TODO(V4-COMPUTE-FRONTIER): Add 4-wide SIMD unroll once
    /// FrontierWindow supports aligned index iteration.
    pub fn execute_frontier<F: CellField>(&self, field: &mut F, window: &FrontierWindow) {
        for c in &self.path {
            if window.contains(c.cell_index) && c.cell_index < field.len() {
                let p = field.get_execution_probability(c.cell_index);
                field.set_execution_probability(
                    c.cell_index,
                    (p * 0.9 + c.prob_signal * 0.1).clamp(0.0, 1.0),
                );
            }
        }
    }
}

// ===================================================================
// SECTION 4 — TRACE FUSION COMPILER
// ===================================================================
//
// TraceFusionCompiler: compile hot execution traces into FusedKernels.
//
// AUTHORITY BOUNDARY:
//   * TFC observes trace signatures (Vec<usize> of cell indices).
//   * It counts hits and compiles on maturity threshold.
//   * It returns FusedKernel — a mathematical primitive.
//   * It does NOT schedule when kernels run.
//   * It does NOT own fiber lifecycle.
//   * It does NOT decide execution eligibility.
//   * MKR provides the trace signatures; TFC compiles them.
//
// TODO(V4-COMPUTE-BOUNDARY): Add `compile_frontier_trace` that takes
// a DifferentialTrace instead of raw Vec<Continuation>. This allows
// the TFC to produce frontier-aware FusedKernels without the TFC
// knowing what the frontier is or where it came from.
//
// TODO(V4-COMPUTE-BOUNDARY): Add cache eviction for stale compiled
// kernels. The compiled_kernels HashMap will grow unboundedly if
// trace signatures change every tick. LRU or TTL eviction needed
// for long-running simulations.
//
// TODO(V4-COMPUTE-BOUNDARY): maturity_threshold should be externally
// injectable at runtime to allow MKR to tune JIT aggressiveness
// based on activation field density without the TFC knowing about
// the field.

/// Trace maturity threshold type alias.
/// Separates concerns: TFC counts hits, MKR sets the policy threshold.
pub type MaturityThreshold = u32;

/// Trace Fusion Compiler — hot path detection and kernel compilation.
///
/// # Authority
/// Mathematical compilation authority only. Does NOT schedule, spawn
/// fibers, or own execution lifecycle. The caller (MKR) decides when
/// to call `optimize()` and what to do with the returned `FusedKernel`.
///
/// # Determinism
/// `optimize()` and `compile_trace()` are deterministic for identical
/// inputs. HashMap iteration order does not affect kernel correctness
/// (the kernel path is constructed from the caller-provided hot_path).
pub struct TraceFusionCompiler {
    /// Hit counts keyed by trace signature (cell index sequence).
    /// Non-deterministic iteration order — used only for threshold checks.
    pub trace_frequencies: std::collections::HashMap<Vec<usize>, MaturityThreshold>,

    /// Compiled kernels keyed by trace signature.
    /// TODO(V4-COMPUTE-BOUNDARY): Add LRU eviction when entry count
    /// exceeds a configurable capacity (default: 256 kernels).
    pub compiled_kernels: std::collections::HashMap<Vec<usize>, FusedKernel>,

    /// Number of hits required before a trace is compiled.
    /// Controlled externally by MKR activation policy.
    pub maturity_threshold: MaturityThreshold,
}

impl TraceFusionCompiler {
    pub fn new(maturity_threshold: MaturityThreshold) -> Self {
        Self {
            trace_frequencies: std::collections::HashMap::new(),
            compiled_kernels: std::collections::HashMap::new(),
            maturity_threshold,
        }
    }

    /// Record a trace hit and return a compiled kernel if maturity is reached.
    ///
    /// # Authority
    /// Mathematical only. Returns `Option<FusedKernel>` — caller decides
    /// whether and when to apply it.
    pub fn optimize(&mut self, signature: Vec<usize>, hot_path: Vec<Continuation>) -> Option<FusedKernel> {
        if signature.is_empty() {
            return None;
        }
        let count = self.trace_frequencies.entry(signature.clone()).or_insert(0);
        *count += 1;

        if *count >= self.maturity_threshold {
            if !self.compiled_kernels.contains_key(&signature) {
                let kernel = self.compile_trace(hot_path);
                self.compiled_kernels.insert(signature.clone(), kernel.clone());
                return Some(kernel);
            } else {
                return Some(self.compiled_kernels[&signature].clone());
            }
        }
        None
    }

    /// Compile a hot path into a FusedKernel.
    ///
    /// # Authority
    /// Pure mathematical compilation. No scheduling, no fiber creation.
    pub fn compile_trace(&self, hot_path: Vec<Continuation>) -> FusedKernel {
        FusedKernel { path: hot_path }
    }

    /// Compile a DifferentialTrace into a frontier-aware FusedKernel.
    ///
    /// Only includes trace records whose cell indices appear in the
    /// differential trace's active set. This allows MKR to provide
    /// a pre-filtered trace without the TFC knowing what "active" means.
    ///
    /// # Authority
    /// Mathematical compilation. DifferentialTrace is a data input.
    /// MKR decides what the active set is; TFC compiles the math.
    ///
    /// TODO(V4-COMPUTE-BOUNDARY): Wire this into the MKR tick loop's
    /// trace collection path once DifferentialTrace is propagated from
    /// the sparse solver output.
    pub fn compile_differential_trace(&self, trace: &DifferentialTrace) -> FusedKernel {
        FusedKernel {
            path: trace.active_records.clone(),
        }
    }
}

// ===================================================================
// SECTION 5 — DIFFERENTIAL EXECUTION PRIMITIVES (Task 2)
// ===================================================================
//
// These types prepare the compute layer for future sparse differential
// execution. They are INFRASTRUCTURE ONLY in this phase:
//   * No global sparse runtime yet.
//   * No CEK semantics.
//   * No scheduler integration.
//   * No activation field authority.
//
// Their purpose is to define the DATA SHAPES that the differential
// runtime will use when it is wired in Phase 5+.
//
// AUTHORITY:
//   * DifferentialTrace — data record of which cells changed
//   * FrontierWindow — a spatial filter (read-only data)
//   * DeltaExecutionMask — a bitmask of execution eligibility
//
// These types do NOT make scheduling decisions.
// MKR provides these as inputs to compute kernels.

/// A differential execution trace — records which cells are active
/// in the current propagation frontier and their probability signals.
///
/// # Authority
/// MATHEMATICAL DATA RECORD only. Produced by MKR's propagation
/// frontier; consumed by TFC and FusedKernel::execute_frontier().
///
/// MKR owns the frontier that produces this record.
/// mirage-compute only performs mathematics on its contents.
///
/// # Lifecycle
/// Created per-tick from the MKR propagation frontier.
/// Not stored across ticks — ephemeral input to kernel compilation.
///
/// TODO(V4-COMPUTE-BOUNDARY): Add frontier region metadata so that
/// cache-local execution grouping can batch cells from the same
/// region (L2/L3 cache locality). Region ID comes from MKR; TFC
/// uses it only for batching math, not for authority decisions.
#[derive(Clone, Debug)]
pub struct DifferentialTrace {
    /// The trace records for cells active in the current frontier.
    /// Subset of all cell records — only changed/active cells.
    pub active_records: Vec<Continuation>,

    /// The frontier tick this trace was captured on.
    /// Used for staleness detection only — not for scheduling.
    pub tick_captured: u64,
}

impl DifferentialTrace {
    /// Create a new differential trace for a given tick.
    pub fn new(tick_captured: u64) -> Self {
        Self {
            active_records: Vec::new(),
            tick_captured,
        }
    }

    /// Push a cell record into the differential trace.
    ///
    /// Called by MKR for each cell in the active frontier.
    #[inline]
    pub fn push(&mut self, cell_index: usize, prob_signal: f32) {
        self.active_records.push(Continuation { cell_index, prob_signal });
    }

    /// Number of active cells in this differential trace.
    #[inline]
    pub fn len(&self) -> usize {
        self.active_records.len()
    }

    /// True if no cells are active in this differential trace.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.active_records.is_empty()
    }
}

/// A spatial frontier window — defines which cell indices are within
/// the active propagation frontier for this tick.
///
/// # Authority
/// READ-ONLY spatial filter. Produced by MKR's PropagationFrontier.
/// mirage-compute uses it only to skip non-frontier cells during
/// kernel execution. The frontier itself is owned by MKR.
///
/// # Implementation
/// Uses a sorted Vec<usize> for O(log N) membership queries.
/// For dense frontiers, a bitset would be more efficient — but that
/// optimization is deferred to V4 Phase 5.
///
/// TODO(V4-COMPUTE-FRONTIER): Replace Vec with a bitset (64-bit words)
/// for O(1) membership check when frontier density > 30%.
/// Bitset layout should match the field's chunk grid width for
/// SIMD-aligned access.
#[derive(Clone, Debug, Default)]
pub struct FrontierWindow {
    /// Sorted list of cell indices in the active frontier.
    /// Sorted to enable binary search membership check.
    active_cells: Vec<usize>,
}

impl FrontierWindow {
    /// Create an empty frontier window.
    pub fn new() -> Self {
        Self { active_cells: Vec::new() }
    }

    /// Build a frontier window from a slice of active cell indices.
    /// Sorts the indices for binary-search membership check.
    pub fn from_cells(mut cells: Vec<usize>) -> Self {
        cells.sort_unstable();
        cells.dedup();
        Self { active_cells: cells }
    }

    /// Check if a cell index is within this frontier window.
    /// O(log N) binary search.
    #[inline]
    pub fn contains(&self, cell_index: usize) -> bool {
        self.active_cells.binary_search(&cell_index).is_ok()
    }

    /// Number of cells in this frontier window.
    #[inline]
    pub fn len(&self) -> usize {
        self.active_cells.len()
    }

    /// True if the frontier window is empty (no active cells).
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.active_cells.is_empty()
    }

    /// Iterate over cell indices in this frontier window.
    #[inline]
    pub fn iter(&self) -> impl Iterator<Item = usize> + '_ {
        self.active_cells.iter().copied()
    }
}

/// A dense bitmask of delta-driven execution eligibility.
///
/// Each bit represents one cell in the field. A set bit means the cell
/// changed sufficiently in this tick to be eligible for kernel execution.
///
/// # Authority
/// MATHEMATICAL DATA RECORD. Produced by MKR's FieldDeltaTracker.
/// mirage-compute reads it to skip non-delta cells.
/// MKR owns the delta tracking authority.
///
/// # Representation
/// Stored as `Vec<u64>` words for SIMD-friendly iteration.
/// Cell `i` maps to bit `i % 64` of word `i / 64`.
///
/// TODO(V4-COMPUTE-BOUNDARY): Add `iter_set()` method that yields
/// cell indices with the bit set, for O(|changed|) kernel iteration
/// without allocating an intermediate Vec<usize>.
#[derive(Clone, Debug)]
pub struct DeltaExecutionMask {
    /// Packed bitmask — 64 cells per u64 word.
    words: Vec<u64>,
    /// Total number of cells represented.
    cell_count: usize,
}

impl DeltaExecutionMask {
    /// Create a zeroed mask for `cell_count` cells.
    pub fn new(cell_count: usize) -> Self {
        let num_words = (cell_count + 63) / 64;
        Self {
            words: vec![0u64; num_words],
            cell_count,
        }
    }

    /// Mark cell `index` as execution-eligible.
    #[inline]
    pub fn set(&mut self, index: usize) {
        if index < self.cell_count {
            self.words[index / 64] |= 1u64 << (index % 64);
        }
    }

    /// Clear cell `index` from execution eligibility.
    #[inline]
    pub fn clear(&mut self, index: usize) {
        if index < self.cell_count {
            self.words[index / 64] &= !(1u64 << (index % 64));
        }
    }

    /// Test if cell `index` is execution-eligible.
    #[inline]
    pub fn is_set(&self, index: usize) -> bool {
        if index < self.cell_count {
            (self.words[index / 64] >> (index % 64)) & 1 == 1
        } else {
            false
        }
    }

    /// Clear all bits.
    pub fn reset(&mut self) {
        for w in &mut self.words {
            *w = 0;
        }
    }

    /// Count of set bits (eligible cells) in O(N/64) time.
    pub fn count_eligible(&self) -> usize {
        self.words.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Total cells this mask covers.
    pub fn cell_count(&self) -> usize {
        self.cell_count
    }
}

// ===================================================================
// SECTION 6 — FUTURE CEK INTEGRATION NOTES
// ===================================================================
//
// TODO(V4-COMPUTE-BOUNDARY): CEK integration point.
//
// When mirage-cek continuations are evaluated, their results will
// be expressed as ComputeContinuation records (cell_index, prob_signal).
//
// The expected data flow is:
//
//   CEK evaluates continuation → produces (cell_index, prob_signal)
//   MKR collects results into DifferentialTrace
//   TFC.compile_differential_trace(&trace) → FusedKernel
//   FusedKernel.execute_frontier(&mut field, &window)
//
// CRITICAL CONSTRAINTS:
//   * mirage-compute NEVER holds a reference to a CEK machine.
//   * mirage-compute NEVER calls push_kontinuation or evaluate_all.
//   * mirage-compute receives only the MATHEMATICAL OUTPUT of CEK.
//   * CEK lifecycle remains in mirage-cek + mirage-mkr-core.
//
// TODO(V4-COMPUTE-BOUNDARY): Define `ComputeCekOutput` struct here
// as the stable interface between CEK evaluation and kernel input.
// This prevents CEK internals from leaking into the compute layer.

// ===================================================================
// TESTS
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    struct MockField {
        probabilities: Vec<f32>,
    }

    impl CellField for MockField {
        fn set_execution_probability(&mut self, index: usize, prob: f32) {
            if index < self.probabilities.len() {
                self.probabilities[index] = prob;
            }
        }
        fn get_execution_probability(&self, index: usize) -> f32 {
            if index < self.probabilities.len() {
                self.probabilities[index]
            } else {
                0.0
            }
        }
        fn len(&self) -> usize {
            self.probabilities.len()
        }
    }

    // ---------------------------------------------------------------
    // Original parity test — MUST remain passing (Task 1 constraint)
    // ---------------------------------------------------------------

    #[test]
    fn test_trace_compilation_and_execution_parity() {
        let mut tfc = TraceFusionCompiler::new(3);
        let signature = vec![1, 2, 3];
        let path = vec![
            Continuation { cell_index: 1, prob_signal: 0.8 },
            Continuation { cell_index: 2, prob_signal: 0.5 },
            Continuation { cell_index: 3, prob_signal: 0.9 },
        ];

        // Ensure compile triggers on the third optimize call (maturity_threshold = 3)
        assert!(tfc.optimize(signature.clone(), path.clone()).is_none());
        assert!(tfc.optimize(signature.clone(), path.clone()).is_none());
        let kernel = tfc.optimize(signature.clone(), path.clone()).expect("Should compile on maturity threshold");

        // Verify compilation matches trace exactly
        assert_eq!(kernel.path, path);

        // Run execution and verify mathematical parity
        let mut field_interpreted = MockField { probabilities: vec![0.0; 4] };
        let mut field_fused = MockField { probabilities: vec![0.0; 4] };

        // 1. Interpreted run (sequential simulation steps)
        for step in &path {
            let p = field_interpreted.get_execution_probability(step.cell_index);
            field_interpreted.set_execution_probability(step.cell_index, (p * 0.9 + step.prob_signal * 0.1).clamp(0.0, 1.0));
        }

        // 2. Fused compiler execution run
        kernel.execute(&mut field_fused);

        // Assert exact mathematical parity
        assert_eq!(field_interpreted.probabilities, field_fused.probabilities);
    }

    // ---------------------------------------------------------------
    // Differential trace tests (Task 2)
    // ---------------------------------------------------------------

    #[test]
    fn differential_trace_push_and_len() {
        let mut trace = DifferentialTrace::new(42);
        assert!(trace.is_empty());
        trace.push(5, 0.8);
        trace.push(10, 0.5);
        assert_eq!(trace.len(), 2);
        assert_eq!(trace.tick_captured, 42);
    }

    #[test]
    fn differential_trace_compiles_to_fused_kernel() {
        let mut trace = DifferentialTrace::new(1);
        trace.push(0, 0.9);
        trace.push(3, 0.6);

        let tfc = TraceFusionCompiler::new(1);
        let kernel = tfc.compile_differential_trace(&trace);

        assert_eq!(kernel.path.len(), 2);
        assert_eq!(kernel.path[0].cell_index, 0);
        assert_eq!(kernel.path[1].cell_index, 3);

        let mut field = MockField { probabilities: vec![0.0; 4] };
        kernel.execute(&mut field);
        assert!(field.probabilities[0] > 0.0);
        assert!(field.probabilities[3] > 0.0);
        // Unchanged cells must remain zero
        assert_eq!(field.probabilities[1], 0.0);
        assert_eq!(field.probabilities[2], 0.0);
    }

    // ---------------------------------------------------------------
    // FrontierWindow tests (Task 2)
    // ---------------------------------------------------------------

    #[test]
    fn frontier_window_contains_and_empty() {
        let w = FrontierWindow::new();
        assert!(w.is_empty());
        assert!(!w.contains(0));
    }

    #[test]
    fn frontier_window_from_cells_sorted_dedup() {
        let w = FrontierWindow::from_cells(vec![5, 2, 5, 8, 2]);
        assert_eq!(w.len(), 3);
        assert!(w.contains(2));
        assert!(w.contains(5));
        assert!(w.contains(8));
        assert!(!w.contains(3));
    }

    #[test]
    fn fused_kernel_execute_frontier_skips_non_frontier_cells() {
        let kernel = FusedKernel {
            path: vec![
                Continuation { cell_index: 0, prob_signal: 0.9 },
                Continuation { cell_index: 1, prob_signal: 0.9 },
                Continuation { cell_index: 2, prob_signal: 0.9 },
            ],
        };
        // Only cell 1 is in the frontier
        let window = FrontierWindow::from_cells(vec![1]);
        let mut field = MockField { probabilities: vec![0.0; 3] };
        kernel.execute_frontier(&mut field, &window);

        assert_eq!(field.probabilities[0], 0.0, "cell 0 not in frontier");
        assert!(field.probabilities[1] > 0.0, "cell 1 in frontier");
        assert_eq!(field.probabilities[2], 0.0, "cell 2 not in frontier");
    }

    // ---------------------------------------------------------------
    // DeltaExecutionMask tests (Task 2)
    // ---------------------------------------------------------------

    #[test]
    fn delta_mask_set_and_query() {
        let mut mask = DeltaExecutionMask::new(128);
        assert!(!mask.is_set(0));
        mask.set(63);
        mask.set(64);
        assert!(mask.is_set(63));
        assert!(mask.is_set(64));
        assert!(!mask.is_set(65));
        assert_eq!(mask.count_eligible(), 2);
    }

    #[test]
    fn delta_mask_reset_clears_all() {
        let mut mask = DeltaExecutionMask::new(64);
        for i in 0..64 { mask.set(i); }
        assert_eq!(mask.count_eligible(), 64);
        mask.reset();
        assert_eq!(mask.count_eligible(), 0);
    }

    #[test]
    fn delta_mask_out_of_bounds_is_noop() {
        let mut mask = DeltaExecutionMask::new(4);
        mask.set(999); // must not panic
        assert!(!mask.is_set(999));
    }

    // ---------------------------------------------------------------
    // TraceFusionCompiler frontier-aware test (Task 3)
    // ---------------------------------------------------------------

    #[test]
    fn tfc_compile_differential_trace_parity() {
        // Build a differential trace with 3 active cells
        let mut trace = DifferentialTrace::new(10);
        trace.push(1, 0.8);
        trace.push(2, 0.5);
        trace.push(3, 0.9);

        let tfc = TraceFusionCompiler::new(1);
        let kernel = tfc.compile_differential_trace(&trace);

        // Execute on full field and verify mathematical parity
        let mut field_interpreted = MockField { probabilities: vec![0.0; 4] };
        let mut field_fused = MockField { probabilities: vec![0.0; 4] };

        for r in &trace.active_records {
            let p = field_interpreted.get_execution_probability(r.cell_index);
            field_interpreted.set_execution_probability(r.cell_index, (p * 0.9 + r.prob_signal * 0.1).clamp(0.0, 1.0));
        }
        kernel.execute(&mut field_fused);

        assert_eq!(field_interpreted.probabilities, field_fused.probabilities);
    }
}
