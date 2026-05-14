pub mod uuid;
pub use uuid::MirageUuid;

use memmap2::Mmap;
use std::fs::File;
use std::path::Path;
use std::fs::OpenOptions;
use memmap2::MmapMut;

/// 🌌 Mirage Oasis: Virtualized Simulation Memory
/// هذا النظام لا "يقرأ" الملفات، بل يربط الذاكرة الافتراضية (Virtual Memory) 
/// الخاصة بنظام التشغيل مباشرة بالقرص الصلب.
/// العالم يتجسد (Materializes) فقط عندما يطلبه المعالج!

pub struct OasisVirtualPage {
    pub data: memmap2::MmapMut, // تأكد أنها MmapMut وليس Mmap
}

impl OasisVirtualPage {
    /// 🗺️ ربط ملف من الهارد بالذاكرة الافتراضية (Zero-Loading Screen)
pub fn map_file(path: impl AsRef<Path>) -> std::io::Result<Self> {
        // 1. فتح الملف بصلاحيات القراءة والكتابة (ضروري لـ MmapMut)
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        
        // 2. استخدام MmapMut بدلاً من Mmap
        // لاحظ استخدام map_mut بدلاً من map
        let mmap = unsafe { MmapMut::map_mut(&file)? };
        
        println!("🌌 Oasis: Virtual Page (Writable) mapped. Size: {} bytes.", mmap.len());
        
        Ok(Self { data: mmap }) // تأكد أن اسم الحقل في الـ Struct هو data
    }

    /// ⚡ الاندماج الحسابي (Zero-Copy SIMD Cast)
    /// هذه الدالة تقرأ البايتات الخام وتحولها لحظياً إلى "دفعات رياضية" (SIMD Batches)
    /// دون أي عملية Deserialization أو نسخ. من الهارد لـ Registers المعالج مباشرة!
    pub fn cast_to_math_batches<T: bytemuck::Pod>(&self) -> &[T] {
        bytemuck::cast_slice(&self.data)
    }

    pub fn cast_to_math_batches_mut<T: bytemuck::Pod>(&mut self) -> &mut [T] {
        bytemuck::cast_slice_mut(&mut self.data)
    }

    /// 🕸️ النبضة التفاعلية (Reactive Page Fault Trigger)
    /// سيتم دمج هذه الدالة لاحقاً مع MirageMatrix
    /// عندما يمس المعالج هذه الصفحة، سيتم إرسال نبضة لإيقاظ الأنظمة المرتبطة
    pub fn touch_and_awaken(&self, system_uuid: MirageUuid) {
        println!("🔍 Page Fault! Waking up computational graph for system: {:?}", system_uuid);
        // هنا سيتم استدعاء: matrix.trace_impact(node)
    }
}