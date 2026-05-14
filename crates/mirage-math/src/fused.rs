use std::simd::f32x4;

/// 🔗 مسار التنفيذ المدمج (Fused Execution Trace)
/// ندمج التغييرات (Scale, Rotation, Translation) في كائن واحد
/// ليتم تطبيقهم معاً في الـ CPU Registers دون العودة للـ RAM.
#[derive(Clone, Debug)]
pub struct FusedTransformOp {
    pub position_delta: f32x4,
    pub rotation_spin: f32x4,
    pub scale_multiplier: f32x4,
}

impl Default for FusedTransformOp {
    fn default() -> Self {
        Self {
            position_delta: f32x4::splat(0.0),
            rotation_spin: f32x4::splat(0.0),
            scale_multiplier: f32x4::splat(1.0),
        }
    }
}

impl FusedTransformOp {
    /// ⚡ التنفيذ المدمج (Fused Application)
    /// المعالج هنا سيعتبرها عملية مجمعة (Batch) وينفذها في أقل عدد ممكن من النبضات
    #[inline(always)]
    pub fn apply_fused(&self, base_pos: &mut f32x4, base_rot: &mut f32x4, base_scale: &mut f32x4) {
        // SIMD Operations: يتم تنفيذها بالتوازي على نفس الكاش لاين
        *base_pos += self.position_delta;
        *base_rot += self.rotation_spin; // نستخدم التقريب التفاضلي الذي بنيناه سابقاً
        *base_scale *= self.scale_multiplier; 
    }
}