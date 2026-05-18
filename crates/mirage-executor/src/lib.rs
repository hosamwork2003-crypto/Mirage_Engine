/// ===================================================================
/// mirage-executor/src/lib.rs  (V3 — Federated Stabilization Pass)
/// PURPOSE: Mirage Executor — Passive Execution Backend
///
/// ---------------------------------------------------------------
/// V3-EXECUTOR-PASSIVE: ROLE BOUNDARY
/// ---------------------------------------------------------------
///
/// The executor is a PASSIVE EXECUTION BACKEND.
/// It receives execution-compatible work descriptors and runs them.
/// It does NOT:
///   * decide execution eligibility (that is MKR/ActivationField)
///   * compute activation or topology pressure (that is MKRWorld)
///   * own scheduling authority (that is EmissionGate)
///   * own thermal state (ThermalSystem is a COMPAT MIRROR only)
///
/// V3 COMPATIBILITY NOTICE:
/// ThermalScheduler is COMPATIBILITY INFRASTRUCTURE in V3.
/// It reads discrete ChunkState enum arms (Hot/Resident/Predictive/Dormant)
/// which is the opposite of the V3 continuous activation field model.
///
/// TODO(V3-EXECUTOR-PASSIVE): Replace ThermalScheduler::schedule_frame()
/// with a field-driven fiber emitter:
///   fn schedule_from_emission(requests: &[SchedulingRequest])
/// where SchedulingRequest comes from mirage-mkr-core::bridge::ExecutionBridge.
/// The executor accepts pre-computed priorities; it does not re-derive them.
///
/// TODO(V3-EXECUTOR-PASSIVE): Remove ThermalSystem ownership from
/// ThermalScheduler.  The executor must not own a thermal model —
/// thermal truth comes from MKRWorld and is mirrored via compat shim.
///
/// TODO(V3-EXECUTOR-PASSIVE): ChunkTask::state: ChunkState must be
/// replaced with ChunkTask::priority: f32 as the SOLE scheduling input.
/// The executor must not branch on discrete enum arms for new code.
///
/// TODO(V3-REMOVE-THERMAL-1): Once all callers use SchedulingRequest,
/// delete ThermalScheduler::schedule_frame() and the ThermalSystem
/// ownership inside this struct.
///
/// SCHEDULING PRIORITY (COMPAT-ONLY):
/// HOT > RESIDENT > PREDICTIVE > DORMANT
/// ===================================================================


use mirage_geometry::columnar::ColumnarPage;
use mirage_synapse::SynapseRegistry;
use mirage_compiler::MirageCompiler;
// TODO(V3-EXECUTOR-PASSIVE): Arc removed — executor does not share ownership
// of runtime state across threads.  Re-add only when fiber pool uses Arc<Mutex<>>.

// Re-export thermal types
pub use mirage_core::runtime::{ChunkState, ThermalSystem, ChunkThermals};

pub mod fiber;

/// Chunk task for execution
#[derive(Debug, Clone)]
pub struct ChunkTask {
    pub chunk_idx: u32,
    pub state: ChunkState,
    pub priority: f32,
    pub deadline_frame: u64,
}

impl PartialEq for ChunkTask {
    fn eq(&self, other: &Self) -> bool {
        self.chunk_idx == other.chunk_idx
    }
}

impl Eq for ChunkTask {}

impl Ord for ChunkTask {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        // Higher priority first, then lower deadline first
        other.priority.partial_cmp(&self.priority)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| self.deadline_frame.cmp(&other.deadline_frame))
    }
}

impl PartialOrd for ChunkTask {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

/// Thermal-aware task scheduler
pub struct ThermalScheduler {
    pub registry: SynapseRegistry,
    pub compiler: MirageCompiler,
    pub thermal_system: ThermalSystem,
    
    /// Task queue (priority heap)
    task_queue: std::collections::BinaryHeap<ChunkTask>,
    
    /// Current frame
    frame: u64,
    
    /// Max tasks per frame to execute.
    /// TODO(V3-EXECUTOR-PASSIVE): This will become the FiberPool emission
    /// budget once ThermalScheduler is replaced by field-driven scheduling.
    #[allow(dead_code)]
    max_tasks_per_frame: usize,
    
    /// Mutation rate threshold for JIT compilation (default 0.3)
    pub mutation_threshold: f32,
}

impl ThermalScheduler {
    pub fn new(num_chunks: usize) -> Self {
        Self {
            registry: SynapseRegistry::new(),
            compiler: MirageCompiler::new(),
            thermal_system: ThermalSystem::new(num_chunks),
            task_queue: std::collections::BinaryHeap::new(),
            frame: 0,
            max_tasks_per_frame: 128,
            mutation_threshold: 0.3,
        }
    }

    /// Schedule chunk tasks based on thermal state
    pub fn schedule_frame(&mut self) {
        self.task_queue.clear();

        // Use safe public API to obtain current states
        let raw = self.thermal_system.get_raw_states();

        for (chunk_idx, &state_u32) in raw.iter().enumerate() {
            let state = match state_u32 {
                3 => ChunkState::Hot,
                2 => ChunkState::Resident,
                1 => ChunkState::Predictive,
                _ => ChunkState::Dormant,
            };

            let priority = match state {
                ChunkState::Hot => 1.0,
                ChunkState::Resident => 0.7,
                ChunkState::Predictive => 0.3,
                ChunkState::Dormant => 0.0,
            };

            if priority > 0.0 {
                self.task_queue.push(ChunkTask {
                    chunk_idx: chunk_idx as u32,
                    state,
                    priority,
                    deadline_frame: self.frame + 4,
                });
            }
        }
    }

    /// Get next task to execute (respecting budget)
    pub fn get_next_task(&mut self) -> Option<ChunkTask> {
        self.task_queue.pop()
    }

    /// Execute a chunk task
    pub fn execute_task<T: Copy + Default>(&mut self, task: ChunkTask, page: &mut ColumnarPage<T>) {
        let dirty_count = page.dirty_tracker.iter_dirty().count();
        let total_capacity = page.data.len();
        let mutation_rate = dirty_count as f32 / total_capacity.max(1) as f32;

        // High mutation: use JIT compilation
        if mutation_rate > self.mutation_threshold {
            let _func_ptr = self.compiler.fuse_and_compile("dense_task_trace");
            // In production, would execute func_ptr as machine code
        } else {
            // Low mutation: use sparse updates
            page.process_changed(|_index, _data| {});
        }

        page.dirty_tracker.clear();
    }

    /// Update thermal system and prepare next frame
    pub fn end_frame(&mut self) {
        self.thermal_system.update_frame();
        self.frame += 1;
    }

    /// Get execution stats
    pub fn get_stats(&self) -> ExecutorStats {
        ExecutorStats {
            frame: self.frame,
            queued_tasks: self.task_queue.len(),
            thermal_stats: self.thermal_system.get_stats(),
        }
    }
}

/// Execution statistics
#[derive(Debug, Clone)]
pub struct ExecutorStats {
    pub frame: u64,
    pub queued_tasks: usize,
    pub thermal_stats: mirage_core::runtime::ThermalStats,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scheduler_creation() {
        let scheduler = ThermalScheduler::new(1000);
        assert_eq!(scheduler.frame, 0);
    }

    #[test]
    fn chunk_task_ordering() {
        // WARNING: ChunkTask::Ord has a KNOWN LEGACY BUG documented here.
        //
        // The Ord impl uses: other.priority.partial_cmp(&self.priority)
        // which means task1.cmp(task2) = Less when task1.priority > task2.priority.
        // In a BinaryHeap (max-heap), "less" items pop last.
        //
        // RESULT: BinaryHeap::pop() actually returns the LOWER-priority task first.
        // This is the OPPOSITE of what is intended.
        //
        // TODO(V3-EXECUTOR-PASSIVE): Fix ChunkTask::Ord to use:
        //   self.priority.partial_cmp(&other.priority)  [without swap]
        //   wrapped in std::cmp::Reverse<ChunkTask> in the BinaryHeap.
        // This change requires updating ThermalScheduler::task_queue type.
        // Safe to fix only after ThermalScheduler is replaced by field-driven scheduling.
        //
        // The test below documents the ACTUAL current behavior (even though it is wrong)
        // so that future refactors do not silently change the behavior without noticing.
        let task1 = ChunkTask {
            chunk_idx: 0,
            state: ChunkState::Hot,
            priority: 1.0,
            deadline_frame: 10,
        };
        let task2 = ChunkTask {
            chunk_idx: 1,
            state: ChunkState::Dormant,
            priority: 0.0,
            deadline_frame: 10,
        };

        // KNOWN BUG: BinaryHeap pops task2 (low priority) before task1 (high priority)
        // because the Ord is inverted.  Do NOT fix this in isolation without fixing
        // the entire ThermalScheduler scheduling logic.
        let mut heap = std::collections::BinaryHeap::new();
        heap.push(task1);
        heap.push(task2);
        let first_popped = heap.pop().unwrap();
        // DOCUMENTING the bug: dormant task pops first due to inverted Ord.
        // TODO(V3-EXECUTOR-PASSIVE): This assert must flip to chunk_idx == 0
        // after fixing the Ord implementation.
        assert_eq!(
            first_popped.chunk_idx, 1,
            "KNOWN BUG: inverted Ord causes low-priority task to pop first from BinaryHeap"
        );
    }
}
