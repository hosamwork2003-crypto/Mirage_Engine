use crate::simd::{SimdVec4, F32x4};

pub struct FastQuaternion {
    pub data: F32x4, // [x, y, z, w]
    pub error_accumulator: f32,
}

impl FastQuaternion {
    const ERROR_THRESHOLD: f32 = 0.05;

    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self {
            data: F32x4::from_array([x, y, z, w]),
            error_accumulator: 0.0,
        }
    }

    /// Apply angular velocity with lazy renormalization
    #[inline(always)]
    pub fn apply_angular_velocity(&mut self, angular_velocity: F32x4, dt: f32) {
        let half_dt = F32x4::splat(0.5 * dt);

        let spin = angular_velocity * half_dt * self.data;
        self.data += spin;

        self.error_accumulator += dt;

        if self.error_accumulator > Self::ERROR_THRESHOLD {
            self.renormalize();
            self.error_accumulator = 0.0;
        }
    }

    #[inline(never)]
    fn renormalize(&mut self) {
        let dot = self.data.dot(self.data);
        let inv_len = 1.0 / dot.sqrt();
        self.data = self.data.mul_scalar(inv_len);
    }
}

/// Differential Position: stores base + delta for efficient tracking
#[derive(Clone, Debug, Default)]
pub struct DifferentialPosition {
    pub base: F32x4,
    pub delta: F32x4,
}

impl DifferentialPosition {
    /// Add perturbation
    #[inline(always)]
    pub fn perturb(&mut self, disturbance: F32x4) {
        self.delta += disturbance;
    }

    /// Resolve and clear
    #[inline(always)]
    pub fn resolve(&mut self) -> F32x4 {
        self.base += self.delta;
        self.delta = F32x4::splat(0.0);
        self.base
    }
}