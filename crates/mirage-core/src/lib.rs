// ===================================================================
// ملف: lib.rs (نواة محرك Mirage)
// ===================================================================

pub mod pool;
pub mod oasis;
pub mod runtime;
pub mod archetype;
pub mod continuation;

pub use runtime::{ChunkState, ChunkThermals, ThermalSystem};

use mirage_geometry::columnar::ColumnarPage;

// إزاحة NUM_ENTITIES هنا ليكون الكور مستقل
pub const NUM_ENTITIES: u32 = 10_000;

pub struct MirageWorld {
    pub positions: ColumnarPage<[f32; 4]>,
    pub colors: ColumnarPage<[f32; 4]>,
}

impl MirageWorld {
    pub fn new() -> Self {
        Self {
            positions: ColumnarPage::new(NUM_ENTITIES as usize),
            colors: ColumnarPage::new(NUM_ENTITIES as usize),
        }
    }

    /// 🔄 تجميع التغييرات فقط (Reactive Delta Collection)
    /// ترجع البيانات كـ Tuples بسيطة لفك الارتباط عن الـ Renderer
    pub fn collect_deltas(&mut self) -> Vec<(u32, [f32; 4], [f32; 4])> {
        let mut deltas = Vec::new();
        
        // الوصول للـ dirty_tracker الخاص بالـ positions كمؤشر للتغيير
        let dirty_indices: Vec<usize> = self.positions.dirty_tracker.iter_dirty().collect();

        for index in dirty_indices {
            deltas.push((
                index as u32,
                self.positions.data[index],
                self.colors.data[index],
            ));
        }

        // تنظيف الـ Dirty Bits بعد القراءة
        self.positions.dirty_tracker.clear();
        self.colors.dirty_tracker.clear();
        
        deltas
    }
}