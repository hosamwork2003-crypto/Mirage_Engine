use crate::simd::{SimdVec16, F32x16};

#[derive(Clone, Debug)]
pub struct Vector3Batch16 {
    pub x: F32x16,
    pub y: F32x16,
    pub z: F32x16,
}

impl Vector3Batch16 {
    #[inline(always)]
    pub fn apply_delta(&mut self, delta_x: F32x16, delta_y: F32x16, delta_z: F32x16) {
        self.x += delta_x;
        self.y += delta_y;
        self.z += delta_z;
    }

    #[inline(always)]
    pub fn distance_squared(&self, target_x: f32, target_y: f32, target_z: f32) -> F32x16 {
        let dx = self.x + F32x16::splat(-target_x);
        let dy = self.y + F32x16::splat(-target_y);
        let dz = self.z + F32x16::splat(-target_z);
        
        (dx * dx) + (dy * dy) + (dz * dz)
    }
}