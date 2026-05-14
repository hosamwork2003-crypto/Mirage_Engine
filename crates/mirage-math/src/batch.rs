use std::simd::f32x16;
use std::simd::num::SimdFloat; // المسار الجديد للنسخة الليلية

#[derive(Clone, Debug)]
pub struct Vector3Batch16 {
    pub x: f32x16,
    pub y: f32x16,
    pub z: f32x16,
}

impl Vector3Batch16 {
    #[inline(always)]
    pub fn apply_delta(&mut self, delta_x: f32x16, delta_y: f32x16, delta_z: f32x16) {
        self.x += delta_x;
        self.y += delta_y;
        self.z += delta_z;
    }

    #[inline(always)]
    pub fn distance_squared(&self, target_x: f32, target_y: f32, target_z: f32) -> f32x16 {
        let dx = self.x - f32x16::splat(target_x);
        let dy = self.y - f32x16::splat(target_y);
        let dz = self.z - f32x16::splat(target_z);
        
        (dx * dx) + (dy * dy) + (dz * dz)
    }
}