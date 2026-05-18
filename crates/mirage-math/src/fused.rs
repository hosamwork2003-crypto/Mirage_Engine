use crate::simd::{SimdVec4, F32x4};

/// Fused Transform: combines scale, rotation, translation in single operation
#[derive(Clone, Debug)]
pub struct FusedTransformOp {
    pub position_delta: F32x4,
    pub rotation_spin: F32x4,
    pub scale_multiplier: F32x4,
}

impl Default for FusedTransformOp {
    fn default() -> Self {
        Self {
            position_delta: F32x4::splat(0.0),
            rotation_spin: F32x4::splat(0.0),
            scale_multiplier: F32x4::splat(1.0),
        }
    }
}

impl FusedTransformOp {
    /// Apply fused transform (all ops in single batch)
    #[inline(always)]
    pub fn apply_fused(&self, base_pos: &mut F32x4, base_rot: &mut F32x4, base_scale: &mut F32x4) {
        *base_pos += self.position_delta;
        *base_rot += self.rotation_spin;
        *base_scale *= self.scale_multiplier;
    }
}