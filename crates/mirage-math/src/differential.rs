use std::simd::f32x4;
use std::simd::num::SimdFloat;
pub struct FastQuaternion {
    pub data: f32x4, // [x, y, z, w]
    pub error_accumulator: f32,
}

impl FastQuaternion {
    // نسبة التسامح مع الخطأ قبل إجبار المعالج على حساب الجذر التربيعي
    const ERROR_THRESHOLD: f32 = 0.05; 

    pub fn new(x: f32, y: f32, z: f32, w: f32) -> Self {
        Self {
            data: f32x4::from_array([x, y, z, w]),
            error_accumulator: 0.0,
        }
    }

    /// ⚡ تطبيق السرعة الزاوية بدون إعادة ضبط مكلفة
    /// q_{t+1} ≈ q_t + 0.5 * ω * q_t * Δt
    #[inline(always)]
    pub fn apply_angular_velocity(&mut self, angular_velocity: f32x4, dt: f32) {
        let half_dt = f32x4::splat(0.5 * dt);
        
        // (تبسيط رياضي للدمج) يضيف الاضطراب الدوراني مباشرة
        let spin = angular_velocity * half_dt * self.data; 
        self.data += spin; 
        
        // تراكم الخطأ بناءً على الزمن بدل الفحص كل فريم
        self.error_accumulator += dt;

        // 🎯 الـ Branching الذكي: لن يتم الدخول هنا إلا نادراً!
        if self.error_accumulator > Self::ERROR_THRESHOLD {
            self.renormalize();
            self.error_accumulator = 0.0;
        }
    }

    #[inline(never)] // نمنع دمجها لتقليل حجم الكود الساخن (I-Cache Optimization)
    fn renormalize(&mut self) {
        let dot = (self.data * self.data).reduce_sum();
        let inv_len = 1.0 / dot.sqrt();
        self.data *= f32x4::splat(inv_len);
    }
}

/// 📏 الموقع التفاضلي (Differential Position)
/// لا نخزن الموقع المباشر ونعيد حساب المصفوفات، بل نخزن "الاضطراب" (Delta) فقط!
#[derive(Clone, Debug, Default)]
pub struct DifferentialPosition {
    pub base: f32x4,  // الموقع المستقر الأصلي
    pub delta: f32x4, // تراكم الاضطرابات (Perturbations)
}

impl DifferentialPosition {
    /// 💉 حقن الاضطراب (يُستدعى من نظام الفيزياء أو الانفجارات)
    #[inline(always)]
    pub fn perturb(&mut self, disturbance: f32x4) {
        self.delta += disturbance;
    }

    /// 🔄 الحل النهائي: يُستدعى فقط عندما يطلب نظام آخر القيمة المجمعة
    #[inline(always)]
    pub fn resolve(&mut self) -> f32x4 {
        self.base += self.delta;
        self.delta = f32x4::splat(0.0); // تصفير الاضطراب بعد استهلاكه
        self.base
    }
}