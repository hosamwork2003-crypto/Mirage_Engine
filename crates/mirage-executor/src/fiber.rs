use std::sync::atomic::{AtomicUsize, Ordering};

pub type ContinuationFn = Box<dyn FnMut() + Send>;

/// Lightweight fiber container — cooperative resumable continuation
pub struct Fiber {
    pub id: usize,
    pub continuation: Option<ContinuationFn>,
    pub budget: u32,
}

impl Fiber {
    pub fn new(id: usize, cont: ContinuationFn) -> Self {
        Self { id, continuation: Some(cont), budget: 100 }
    }

    pub fn resume(&mut self) {
        if let Some(f) = &mut self.continuation {
            (f)();
        }
    }

    pub fn suspend(&mut self) {
        // Cooperative suspend — continuation preserved
    }
}

/// Fixed-capacity fiber pool (no heap churn on spawn in hot-path)
pub struct FiberPool {
    pool: Vec<Option<Fiber>>,
    next_id: AtomicUsize,
}

impl FiberPool {
    pub fn with_capacity(cap: usize) -> Self {
let mut pool = Vec::with_capacity(cap);

for _ in 0..cap {
    pool.push(None);
}

Self {
    pool,
    next_id: AtomicUsize::new(0),
}
    }

    pub fn spawn(&mut self, cont: ContinuationFn) -> usize {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let idx = id % self.pool.len();
        self.pool[idx] = Some(Fiber::new(id, cont));
        id
    }

    pub fn resume(&mut self, id: usize) {
        let idx = id % self.pool.len();
        if let Some(f) = &mut self.pool[idx] {
            f.resume();
        }
    }
}
