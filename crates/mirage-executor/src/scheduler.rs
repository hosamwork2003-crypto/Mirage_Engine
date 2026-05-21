// ===================================================================
// mirage-executor/src/scheduler.rs
// PURPOSE: NUMA-Aware Work Stealing Scheduler
// ===================================================================

use std::thread::available_parallelism;
use crossbeam_deque::{Worker, Stealer, Injector, Steal};
use crate::fiber::Fiber;

pub type CoreId = usize;

#[derive(Debug, Clone)]
pub struct HardwareAffinityMap {
    pub num_cores: usize,
    pub cores_per_numa: usize,
}

impl HardwareAffinityMap {
    pub fn new() -> Self {
        let num_cores = available_parallelism()
            .map(|p| p.get())
            .unwrap_or(4);
        Self {
            num_cores,
            cores_per_numa: 4, // standard virtual NUMA sizing
        }
    }

    pub fn numa_node_of(&self, core: CoreId) -> usize {
        core / self.cores_per_numa
    }

    pub fn share_numa_or_l3(&self, core_a: CoreId, core_b: CoreId) -> bool {
        self.numa_node_of(core_a) == self.numa_node_of(core_b)
    }
}

pub struct NUMAAwareScheduler {
    pub affinity_map: HardwareAffinityMap,
    workers: Vec<Worker<Fiber>>,
    stealers: Vec<Stealer<Fiber>>,
    injector: Injector<Fiber>,
}

impl NUMAAwareScheduler {
    pub fn new() -> (
        Self,
        crate::ExecutionBridgeAuthority,
        crate::SchedulerCapability,
        crate::FrontierExecutionCapability,
    ) {
        let affinity_map = HardwareAffinityMap::new();
        let num_cores = affinity_map.num_cores;
        let mut workers = Vec::with_capacity(num_cores);
        let mut stealers = Vec::with_capacity(num_cores);

        for _ in 0..num_cores {
            let w = Worker::new_fifo();
            stealers.push(w.stealer());
            workers.push(w);
        }

        (
            Self {
                affinity_map,
                workers,
                stealers,
                injector: Injector::new(),
            },
            crate::ExecutionBridgeAuthority::new(),
            crate::SchedulerCapability::new(),
            crate::FrontierExecutionCapability::new(),
        )
    }

    pub fn schedule_request(
        &self,
        req: &crate::ExecutionRequest,
        fiber: Fiber,
        _cap: &crate::SchedulerCapability,
    ) {
        let core_id = req.cell_index() % self.affinity_map.num_cores;
        self.schedule_with_affinity_internal(fiber, core_id);
    }

    pub fn schedule_batch(
        &self,
        batch: &crate::FrontierExecutionBatch,
        fibers: Vec<Fiber>,
        _cap: &crate::SchedulerCapability,
    ) {
        for fiber in fibers {
            self.schedule_with_affinity_internal(fiber, batch.affinity_hint());
        }
    }

    pub fn schedule_with_affinity(
        &self,
        fiber: Fiber,
        affinity: CoreId,
        _cap: &crate::SchedulerCapability,
    ) {
        self.schedule_with_affinity_internal(fiber, affinity);
    }

    pub fn schedule_global(&self, fiber: Fiber, _cap: &crate::SchedulerCapability) {
        self.injector.push(fiber);
    }

    fn schedule_with_affinity_internal(&self, fiber: Fiber, affinity: CoreId) {
        let core_id = affinity % self.affinity_map.num_cores;
        self.workers[core_id].push(fiber);
    }

    pub fn get_task_for_core(&self, core_id: CoreId) -> Option<Fiber> {
        let core_id = core_id % self.affinity_map.num_cores;

        // 1. Pop from local queue
        if let Some(fiber) = self.workers[core_id].pop() {
            return Some(fiber);
        }

        // 2. Try to steal from cores sharing the same NUMA node / L3 cache
        for other_core in 0..self.affinity_map.num_cores {
            if other_core != core_id && self.affinity_map.share_numa_or_l3(core_id, other_core) {
                loop {
                    match self.stealers[other_core].steal() {
                        Steal::Success(fiber) => return Some(fiber),
                        Steal::Empty => break,
                        Steal::Retry => continue,
                    }
                }
            }
        }

        // 3. Try to steal from the global injector
        loop {
            match self.injector.steal() {
                Steal::Success(fiber) => return Some(fiber),
                Steal::Empty => break,
                Steal::Retry => continue,
            }
        }

        // 4. Try to steal from any other core (cold steal)
        for other_core in 0..self.affinity_map.num_cores {
            if other_core != core_id && !self.affinity_map.share_numa_or_l3(core_id, other_core) {
                loop {
                    match self.stealers[other_core].steal() {
                        Steal::Success(fiber) => return Some(fiber),
                        Steal::Empty => break,
                        Steal::Retry => continue,
                    }
                }
            }
        }

        None
    }

    /// Returns the number of tasks pending in the local queue for a specific core.
    pub fn queue_len(&self, core_id: CoreId) -> usize {
        let core_id = core_id % self.affinity_map.num_cores;
        self.workers[core_id].len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_numa_scheduler_affinity_and_stealing() {
        let (scheduler, _auth, cap, _cap_front) = NUMAAwareScheduler::new();
        let numa_map = &scheduler.affinity_map;

        // If available parallelism is 1, stealing tests are degenerate; force multi-core simulation if needed,
        // but get_task_for_core behaves correctly regardless of num_cores.
        let f1 = Fiber::new(101, Box::new(|| {}));
        let f2 = Fiber::new(102, Box::new(|| {}));

        // Schedule with affinity to core 0 and core 1
        scheduler.schedule_with_affinity(f1, 0, &cap);
        scheduler.schedule_with_affinity(f2, 1, &cap);

        // Fetch tasks
        let task_c0 = scheduler.get_task_for_core(0).expect("Core 0 should get task 101");
        assert_eq!(task_c0.id, 101);

        // If core 1 hasn't been drained, core 0 can steal from it if they share NUMA node (e.g. core 0 and 1 usually share)
        if numa_map.num_cores > 1 && numa_map.share_numa_or_l3(0, 1) {
            let stolen = scheduler.get_task_for_core(0).expect("Core 0 should steal from Core 1");
            assert_eq!(stolen.id, 102);
        } else {
            let task_c1 = scheduler.get_task_for_core(1).expect("Core 1 should get task 102");
            assert_eq!(task_c1.id, 102);
        }
    }
}
