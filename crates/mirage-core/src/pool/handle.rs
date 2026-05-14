// ===================================================================
// ملف: handle.rs
// الوظيفة: المعرّف الذكي (المقبض) الذي يربط المطور بالدليل (Directory).
// السر الهندسي: حجمه 8 بايت فقط، مما يجعله Cache-friendly تماماً.
// ===================================================================

use std::marker::PhantomData;
use serde::{Deserialize, Serialize};

/// [Handle] هو "المفتاح" الذي يستخدمه المطور للوصول للكائنات.
/// هو لا يحتوي على البيانات الفيزيائية، بل يحتوي على الفهرس (Index) والجيل (Generation).
/// يتم استخدامه للبحث في الـ Directory Layer للوصول لمكان الكائن الفعلي (Chunk/Slot).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)] // لضمان ثبات ترتيب الذاكرة وتوافقها مع الأنظمة الأخرى (ككارت الشاشة أو الهارد)
pub struct Handle<T> {
    /// الفهرس الثابت داخل الـ Directory (Logical Identity)
    index: u32,
    /// رقم الجيل لمنع الـ Use-After-Free (لو الكائن تم حذفه واستبداله بآخر في نفس المكان)
    generation: u32,
    /// علامة وهمية لنوع البيانات T لضمان النوع (Type Safety) دون استهلاك أي بايت إضافي
    #[serde(skip)]
    _marker: PhantomData<T>,
}

impl<T> Handle<T> {
    
pub const NONE: Self = Self {
        index: 0,
        generation: 0,
        _marker: PhantomData, // إضافة underscore
    };

    pub fn new(index: u32, generation: u32) -> Self {
        Self { index, generation, _marker: PhantomData } // إضافة underscore
    }
    /// الحصول على الفهرس (المستخدم للبحث في جدول الـ Indirection)
    #[inline(always)]
    pub fn index(&self) -> u32 {
        self.index
    }

    /// الحصول على الجيل الحالي للمقبض
    #[inline(always)]
    pub fn generation(&self) -> u32 {
        self.generation
    }

    /// التحقق مما إذا كان المقبض يشير لـ "لاشيء"
    #[inline(always)]
    pub fn is_none(&self) -> bool {
        self.index == 0 && self.generation == 0
    }
}

// تنفيذ الـ Default لسهولة البدء بقيمة NONE
impl<T> Default for Handle<T> {
    fn default() -> Self {
        Self::NONE
    }
}