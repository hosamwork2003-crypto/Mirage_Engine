/// GPU-driven helpers used by the renderer for culling and indirect generation

pub struct GpuDriven {
    pub visible_chunks: Vec<u32>,
}

impl GpuDriven {
    pub fn new() -> Self { Self { visible_chunks: Vec::new() } }

    /// Simple CPU-side frustum distance culling (cheap, used to seed GPU tests)
    pub fn cull(&mut self, camera_pos: [f32;3], chunk_centers: &[[f32;3]]) {
        self.visible_chunks.clear();
        for (i, center) in chunk_centers.iter().enumerate() {
            let dx = center[0] - camera_pos[0];
            let dy = center[1] - camera_pos[1];
            let dz = center[2] - camera_pos[2];
            let dist2 = dx*dx + dy*dy + dz*dz;
            // conservative radius
            if dist2 < (200.0f32 * 200.0f32) {
                self.visible_chunks.push(i as u32);
            }
        }
    }

    /// Produce a simple indirect list (indices) for upload
    pub fn generate_indirect(&self) -> Vec<u32> {
        self.visible_chunks.clone()
    }
}
