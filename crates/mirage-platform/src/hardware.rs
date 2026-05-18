/// Hardware awareness utilities — NUMA, affinity, cache topology.
/// Kept intentionally small and stable. Uses num_cpus to detect core counts.

pub struct HardwareInfo {
    pub logical_cores: usize,
    pub physical_cores: usize,
    pub numa_nodes: usize,
    pub simd_width: usize,
}

impl HardwareInfo {
    pub fn detect() -> Self {
        // num_cpus is optional but present in Cargo.toml for platform crate
        let logical = num_cpus::get();
        let physical = num_cpus::get_physical();
        Self {
            logical_cores: logical,
            physical_cores: physical,
            numa_nodes: 1, // conservative default; NUMA detection can be added per-platform
            simd_width: 8, // default fallback (x86_64 SSE/AVX lanes)
        }
    }

    /// Simple affinity decision: map chunk -> core index
    pub fn affinity_for_chunk(&self, chunk_idx: usize) -> usize {
        chunk_idx % self.logical_cores
    }
}
