// ===================================================================
// ملف: handle.rs
// الوظيفة: المعرّف الذكي (المقبض) الذي يربط المطور بالدليل (Runtime Directory).
// السر الهندسي: تم إزالة الـ <T> ليصبح مقبضاً عالمياً (Type Erasure).
// ===================================================================

// serde import retained for when Handle needs Serialize/Deserialize for asset persistence.
// TODO(V3): Enable when Handle is used in serialisable asset manifests.
// use serde::{Deserialize, Serialize};

/// [Handle] هو "المفتاح" السريع الذي يستخدمه المحرك.
/// حجمه 8 بايت فقط، صديق للـ Cache، ولا يهتم بنوع البيانات.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]#[repr(C)] 
pub struct Handle {
    index: u32,
    generation: u32,
}

impl Handle {
    pub const NONE: Self = Self {
        index: 0,
        generation: 0,
    };

    pub fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    #[inline(always)]
    pub fn index(&self) -> u32 {
        self.index
    }

    #[inline(always)]
    pub fn generation(&self) -> u32 {
        self.generation
    }
}