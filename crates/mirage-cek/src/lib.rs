// ===================================================================
// mirage-cek/src/lib.rs
// PURPOSE: CEK Virtual Machine Substrate
//
// CEK = Control-Environment-Kontinuation
//
// ARCHITECTURAL ROLE
// ---------------------------------------------------------------
// This crate defines the pure CEK machine primitives that the MKR
// kernel uses to implement resumable, heap-allocated computation over
// the activation field. It is intentionally decoupled from the
// concrete ActivationField type.
// ===================================================================

// =====================================================================
// CekEvalField — trait contract for mutable field access
// =====================================================================

/// Minimal mutable interface that a CEK continuation frame needs to
/// interact with the activation field.
pub trait CekEvalField: Send {
    /// Number of cells in this field.
    fn cell_count(&self) -> usize;

    /// Get the current execution probability of cell `index`.
    fn get_exec_prob(&self, index: usize) -> f32;

    /// Set the execution probability of cell `index`.
    fn set_exec_prob(&mut self, index: usize, value: f32);
}

// =====================================================================
// Continuation Primitives (V4 Sealed Engine)
// =====================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContinuationId(pub u64);

impl std::fmt::Display for ContinuationId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "ContID({})", self.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum ContinuationOp {
    AdjustExecProbability {
        cell_idx: usize,
        delta: f32,
    },
    SetExecProbability {
        cell_idx: usize,
        value: f32,
    },
    PropagateExecution {
        source_idx: usize,
        target_idx: usize,
        weight: f32,
    },
}

impl ContinuationOp {
    pub fn cell_idx(&self) -> usize {
        match self {
            ContinuationOp::AdjustExecProbability { cell_idx, .. } => *cell_idx,
            ContinuationOp::SetExecProbability { cell_idx, .. } => *cell_idx,
            ContinuationOp::PropagateExecution { target_idx, .. } => *target_idx,
        }
    }

    pub fn realize(&self, field: &mut dyn CekEvalField) {
        match self {
            ContinuationOp::AdjustExecProbability { cell_idx, delta } => {
                let current = field.get_exec_prob(*cell_idx);
                field.set_exec_prob(*cell_idx, (current * 0.9 + delta * 0.1).clamp(0.0, 1.0));
            }
            ContinuationOp::SetExecProbability { cell_idx, value } => {
                field.set_exec_prob(*cell_idx, *value);
            }
            ContinuationOp::PropagateExecution { source_idx, target_idx, weight } => {
                let src_prob = field.get_exec_prob(*source_idx);
                let target_prob = field.get_exec_prob(*target_idx);
                field.set_exec_prob(*target_idx, (target_prob + src_prob * weight).clamp(0.0, 1.0));
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContinuationProvenance {
    pub request_id: mirage_executor::ExecutionRequestId,
    pub originating_tick: u64,
    pub originating_frontier_generation: u64,
    pub emission_source_id: u32,
    pub deterministic_sequence_index: u64,
    pub realization_sequence_index: u64,
}

impl ContinuationProvenance {
    pub fn from_request(
        request: &mirage_executor::ExecutionRequest,
        realization_sequence_index: u64,
    ) -> Self {
        Self {
            request_id: request.request_id(),
            originating_tick: request.originating_tick(),
            originating_frontier_generation: request.originating_frontier_generation(),
            emission_source_id: request.emission_source_id(),
            deterministic_sequence_index: request.deterministic_sequence_index(),
            realization_sequence_index,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ContinuationDescriptor {
    pub continuation_id: ContinuationId,
    pub request_id: mirage_executor::ExecutionRequestId,
    pub sequence_index: u64,
    pub provenance: ContinuationProvenance,
    pub op: ContinuationOp,
}

pub type ContinuationRecord = ContinuationDescriptor;

#[derive(Debug, Clone)]
pub struct DeterministicContinuationBuffer {
    pub continuations: Vec<ContinuationDescriptor>,
}

pub type RealizationSequence = DeterministicContinuationBuffer;
pub type ContinuationRealizationSequence = DeterministicContinuationBuffer;

pub fn stable_sort_continuations(continuations: &mut [ContinuationDescriptor]) {
    continuations.sort_by(|a, b| {
        a.provenance.deterministic_sequence_index.cmp(&b.provenance.deterministic_sequence_index)
            .then_with(|| a.provenance.originating_tick.cmp(&b.provenance.originating_tick))
            .then_with(|| a.request_id.cmp(&b.request_id))
            .then_with(|| a.op.cell_idx().cmp(&b.op.cell_idx()))
    });
}

impl DeterministicContinuationBuffer {
    pub fn stable_sort(&mut self) {
        stable_sort_continuations(&mut self.continuations);
    }
}

#[derive(Debug, Clone)]
pub struct LinearizedRealizationPass {
    pub buffer: DeterministicContinuationBuffer,
}

impl LinearizedRealizationPass {
    pub fn new(buffer: DeterministicContinuationBuffer) -> Self {
        Self { buffer }
    }

    pub fn execute(&self, field: &mut dyn CekEvalField) {
        for desc in &self.buffer.continuations {
            desc.op.realize(field);
        }
    }
}

// =====================================================================
// ContinuationArena — Allocation-stable deterministic storage
// =====================================================================

#[derive(Debug, Clone)]
pub struct ContinuationArena {
    pub slots: Vec<Option<ContinuationDescriptor>>,
    pub free_list: Vec<usize>,
}

impl ContinuationArena {
    pub fn new() -> Self {
        Self {
            slots: Vec::new(),
            free_list: Vec::new(),
        }
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            slots: Vec::with_capacity(capacity),
            free_list: Vec::with_capacity(capacity),
        }
    }

    pub fn clear(&mut self) {
        self.slots.clear();
        self.free_list.clear();
    }

    pub fn reserve(&mut self, additional: usize) {
        self.slots.reserve(additional);
        self.free_list.reserve(additional);
    }

    pub fn insert(&mut self, node: ContinuationDescriptor) -> usize {
        if let Some(idx) = self.free_list.pop() {
            self.slots[idx] = Some(node);
            idx
        } else {
            let idx = self.slots.len();
            self.slots.push(Some(node));
            idx
        }
    }

    pub fn remove(&mut self, index: usize) -> Option<ContinuationDescriptor> {
        if index < self.slots.len() {
            if let Some(node) = self.slots[index].take() {
                self.free_list.push(index);
                // Keep sorted descending so pop() yields smallest index (stable)
                self.free_list.sort_unstable_by(|a, b| b.cmp(a));
                return Some(node);
            }
        }
        None
    }

    pub fn get(&self, index: usize) -> Option<&ContinuationDescriptor> {
        self.slots.get(index).and_then(|x| x.as_ref())
    }
}

// =====================================================================
// CEKMachine — Control-Environment-Kontinuation Context
// =====================================================================

pub struct CEKMachine {
    pub control_cell: usize,
    pub environment_weights: std::sync::Arc<[f32]>,
    pub kontinuation_stack: Vec<usize>,
    pub prob_signal: f32,
    pub request_id: mirage_executor::ExecutionRequestId,
}

impl CEKMachine {
    pub fn new(
        control_cell: usize,
        environment_weights: std::sync::Arc<[f32]>,
        prob_signal: f32,
        request_id: mirage_executor::ExecutionRequestId,
    ) -> Self {
        Self {
            control_cell,
            environment_weights,
            kontinuation_stack: Vec::new(),
            prob_signal,
            request_id,
        }
    }

    pub fn push_kontinuation(&mut self, k: usize) {
        self.kontinuation_stack.push(k);
    }

    pub fn evaluate_all(&mut self, field: &mut dyn CekEvalField, arena: &ContinuationArena) {
        while let Some(idx) = self.kontinuation_stack.pop() {
            if let Some(node) = arena.get(idx) {
                node.op.realize(field);
            }
        }
    }

    #[inline]
    pub fn is_pending(&self) -> bool {
        !self.kontinuation_stack.is_empty()
    }

    #[inline]
    pub fn request_id(&self) -> mirage_executor::ExecutionRequestId {
        self.request_id
    }
}

// =====================================================================
// Tests
// =====================================================================
#[cfg(test)]
mod tests {
    use super::*;
    use mirage_executor::ExecutionRequestId;

    struct MockField {
        cells: Vec<f32>,
    }
    impl MockField {
        fn new(n: usize) -> Self { Self { cells: vec![0.0; n] } }
    }
    unsafe impl Send for MockField {}
    impl CekEvalField for MockField {
        fn cell_count(&self) -> usize { self.cells.len() }
        fn get_exec_prob(&self, idx: usize) -> f32 {
            self.cells.get(idx).copied().unwrap_or(0.0)
        }
        fn set_exec_prob(&mut self, idx: usize, value: f32) {
            if let Some(c) = self.cells.get_mut(idx) {
                *c = value.clamp(0.0, 1.0);
            }
        }
    }

    #[test]
    fn new_machine_starts_empty() {
        let m = CEKMachine::new(3, std::sync::Arc::from(vec![0.5; 4]), 0.7, ExecutionRequestId(123));
        assert_eq!(m.control_cell, 3);
        assert!((m.prob_signal - 0.7).abs() < 1e-6);
        assert_eq!(m.request_id(), ExecutionRequestId(123));
        assert!(!m.is_pending());
    }

    #[test]
    fn push_and_drain_stack() {
        let mut m = CEKMachine::new(0, std::sync::Arc::from(vec![]), 1.0, ExecutionRequestId(1));
        let mut field = MockField::new(4);
        let mut arena = ContinuationArena::new();

        let req_id = ExecutionRequestId(1);
        let prov = ContinuationProvenance {
            request_id: req_id,
            originating_tick: 0,
            originating_frontier_generation: 0,
            emission_source_id: 0,
            deterministic_sequence_index: 0,
            realization_sequence_index: 0,
        };

        let node = ContinuationDescriptor {
            continuation_id: ContinuationId(1),
            request_id: req_id,
            sequence_index: 0,
            provenance: prov,
            op: ContinuationOp::SetExecProbability { cell_idx: 0, value: 0.9 },
        };
        let idx = arena.insert(node);
        m.push_kontinuation(idx);
        assert!(m.is_pending());

        m.evaluate_all(&mut field, &arena);
        assert!(!m.is_pending());
        assert!((field.get_exec_prob(0) - 0.9).abs() < 1e-6);
    }

    #[test]
    fn stack_drains_lifo() {
        let mut m = CEKMachine::new(0, std::sync::Arc::from(vec![]), 1.0, ExecutionRequestId(1));
        let mut field = MockField::new(2);
        let mut arena = ContinuationArena::new();

        let req_id = ExecutionRequestId(1);
        let prov = ContinuationProvenance {
            request_id: req_id,
            originating_tick: 0,
            originating_frontier_generation: 0,
            emission_source_id: 0,
            deterministic_sequence_index: 0,
            realization_sequence_index: 0,
        };

        let n1 = ContinuationDescriptor {
            continuation_id: ContinuationId(1),
            request_id: req_id,
            sequence_index: 0,
            provenance: prov,
            op: ContinuationOp::SetExecProbability { cell_idx: 0, value: 0.1 },
        };
        let n2 = ContinuationDescriptor {
            continuation_id: ContinuationId(2),
            request_id: req_id,
            sequence_index: 0,
            provenance: prov,
            op: ContinuationOp::SetExecProbability { cell_idx: 0, value: 0.9 },
        };

        let idx1 = arena.insert(n1);
        let idx2 = arena.insert(n2);
        m.push_kontinuation(idx1);
        m.push_kontinuation(idx2);

        m.evaluate_all(&mut field, &arena);
        // LIFO: n2 runs first, then n1 runs last and overwrites with 0.1
        assert!((field.get_exec_prob(0) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn evaluate_all_mutates_field() {
        let mut m = CEKMachine::new(5, std::sync::Arc::from(vec![1.0; 8]), 0.85, ExecutionRequestId(1));
        let mut field = MockField::new(8);
        let mut arena = ContinuationArena::new();

        let req_id = ExecutionRequestId(1);
        let prov = ContinuationProvenance {
            request_id: req_id,
            originating_tick: 0,
            originating_frontier_generation: 0,
            emission_source_id: 0,
            deterministic_sequence_index: 0,
            realization_sequence_index: 0,
        };

        let node = ContinuationDescriptor {
            continuation_id: ContinuationId(1),
            request_id: req_id,
            sequence_index: 0,
            provenance: prov,
            op: ContinuationOp::AdjustExecProbability { cell_idx: 5, delta: 0.85 },
        };
        let idx = arena.insert(node);
        m.push_kontinuation(idx);

        m.evaluate_all(&mut field, &arena);
        // 0.0 * 0.9 + 0.85 * 0.1 = 0.085
        assert!((field.get_exec_prob(5) - 0.085).abs() < 1e-4);
    }

    #[test]
    fn out_of_bounds_set_is_noop() {
        let mut field = MockField::new(2);
        field.set_exec_prob(999, 1.0);
        assert!((field.get_exec_prob(999) - 0.0).abs() < 1e-6);
    }

    #[test]
    fn deterministic_arena_slot_reuse() {
        let mut arena = ContinuationArena::new();
        let req_id = ExecutionRequestId(1);
        let prov = ContinuationProvenance {
            request_id: req_id,
            originating_tick: 0,
            originating_frontier_generation: 0,
            emission_source_id: 0,
            deterministic_sequence_index: 0,
            realization_sequence_index: 0,
        };
        let d1 = ContinuationDescriptor {
            continuation_id: ContinuationId(1),
            request_id: req_id,
            sequence_index: 0,
            provenance: prov,
            op: ContinuationOp::SetExecProbability { cell_idx: 0, value: 0.1 },
        };
        let d2 = d1.clone();
        let d3 = d1.clone();

        let idx1 = arena.insert(d1.clone());
        let idx2 = arena.insert(d2.clone());
        let idx3 = arena.insert(d3.clone());

        assert_eq!(idx1, 0);
        assert_eq!(idx2, 1);
        assert_eq!(idx3, 2);

        // Remove the first and third
        arena.remove(0);
        arena.remove(2);

        // Pop should yield 0 first, then 2.
        let d4 = d3.clone();
        let idx4 = arena.insert(d4);
        assert_eq!(idx4, 0, "Should reuse lowest slot first (0)");

        let d5 = d3.clone();
        let idx5 = arena.insert(d5);
        assert_eq!(idx5, 2, "Should reuse next lowest slot (2)");
    }
}
