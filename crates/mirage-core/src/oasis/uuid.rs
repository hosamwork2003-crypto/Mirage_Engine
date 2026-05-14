// ===================================================================
// ملف: uuid.rs (داخل نظام Oasis)
// الوظيفة: إنشاء "رقم قومي" (128-bit) لا يتكرر أبداً لكل كائن في اللعبة.
// ===================================================================

use serde::{Deserialize, Serialize};
use bytemuck::{Pod, Zeroable};

/// [MirageUuid] هو الرقم القومي للكائن.
/// السر الهندسي: استخدمنا `#[repr(transparent)]` ومصفوفة من 16 بايت `[u8; 16]`.
/// هذا يعني أن هذا الهيكل في الذاكرة هو "مجرد 16 بايت خام" بدون أي إضافات،
/// مما يجعله جاهزاً 100% ليتم قراءته من الهارد (SSD) إلى الرام فوراً (Zero-Copy).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Pod, Zeroable)]
#[repr(transparent)]
pub struct MirageUuid(pub [u8; 16]);

impl MirageUuid {
    /// دالة لإنشاء رقم قومي فارغ (أصفار) - تُستخدم عند تهيئة الذاكرة
    #[inline]
    pub const fn zero() -> Self {
        Self([0; 16])
    }

    /// دالة لإنشاء رقم قومي جديد وعشوائي كلياً
    #[inline]
    pub fn new() -> Self {
        // نستخدم مكتبة uuid القياسية في لغة Rust لتوليد رقم عشوائي (Version 4)
        // ثم نحوله فوراً إلى مصفوفة بايتات (Bytes) ليتوافق مع معمارية Oasis
        let id = uuid::Uuid::new_v4();
        Self(id.into_bytes())
    }
}

// لضمان أن المولد الافتراضي ينشئ رقماً جديداً دائماً
impl Default for MirageUuid {
    fn default() -> Self {
        Self::new()
    }
}