use std::sync::atomic::{AtomicU64, Ordering};

pub struct ProfileStats {
    pub frame_count: AtomicU64,
    pub shader_upload_bytes: AtomicU64,
}

impl ProfileStats {
    pub fn new() -> Self {
        Self { frame_count: AtomicU64::new(0), shader_upload_bytes: AtomicU64::new(0) }
    }

    pub fn inc_frame(&self) { self.frame_count.fetch_add(1, Ordering::Relaxed); }
    pub fn add_upload_bytes(&self, bytes: u64) { self.shader_upload_bytes.fetch_add(bytes, Ordering::Relaxed); }
}

pub struct DebugProfiler {
    pub stats: ProfileStats,
}

impl DebugProfiler {
    pub fn new() -> Self { Self { stats: ProfileStats::new() } }
}
