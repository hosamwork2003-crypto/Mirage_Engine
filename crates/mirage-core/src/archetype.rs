//! Archetype thermal SoA storage — chunk-native SoA layouts and migration helpers

pub const CHUNK_CAPACITY: usize = 64;

/// SoA storage for many chunks concatenated; chunk-local slice is contiguous
pub struct ChunkSoA {
    pub positions: Vec<[f32;4]>,
    pub velocities: Vec<[f32;4]>,
    pub colors: Vec<[f32;4]>,
    pub num_chunks: usize,
}

impl ChunkSoA {
    pub fn new(num_chunks: usize) -> Self {
        let total = num_chunks * CHUNK_CAPACITY;
        Self {
            positions: vec![[0.0;4]; total],
            velocities: vec![[0.0;4]; total],
            colors: vec![[1.0;4]; total],
            num_chunks,
        }
    }

    pub fn chunk_range(&self, chunk_idx: usize) -> std::ops::Range<usize> {
        let start = chunk_idx * CHUNK_CAPACITY;
        start..start + CHUNK_CAPACITY
    }

    pub fn get_positions(&self, chunk_idx: usize) -> &[[f32;4]] {
        let r = self.chunk_range(chunk_idx);
        &self.positions[r]
    }

    pub fn get_positions_mut(&mut self, chunk_idx: usize) -> &mut [[f32;4]] {
        let r = self.chunk_range(chunk_idx);
        &mut self.positions[r]
    }
}
