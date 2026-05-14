use aligned_vec::AVec;
use crate::bitmaps::DirtyBitmap;

// Layer 1: Aligned Columnar Page
pub struct ColumnarPage<T: Copy> {
    pub data: AVec<T>, 
    pub dirty_tracker: DirtyBitmap,
}

impl<T: Copy + Default> ColumnarPage<T> {
    pub fn new(capacity: usize) -> Self {
        let mut data = AVec::with_capacity(64, capacity);
        for _ in 0..capacity {
            data.push(T::default());
        }
        Self {
            data,
            dirty_tracker: DirtyBitmap::new(),
        }
    }

    #[inline(always)]
    pub fn update(&mut self, index: usize, value: T) {
        if index < self.data.len() {
            self.data[index] = value;
            self.dirty_tracker.set_dirty(index);
        }
    }

    pub fn process_changed<F>(&mut self, mut func: F) 
    where 
        F: FnMut(usize, &mut T) 
    {
        for index in self.dirty_tracker.iter_dirty() {
            func(index, &mut self.data[index]);
        }
        self.dirty_tracker.clear();
    }

    // =======================================================
    // 🔥 Layer 5: Predictive Defragmentation (Data Packing)
    // =======================================================
    /// يقوم بضغط الذاكرة لإزالة الفجوات (Holes) الناتجة عن الكائنات الميتة.
    /// يعمل بأسلوب تقاطع المؤشرات (Two Pointers) ليحقق سرعة O(N).
    pub fn defragment<F>(&mut self, is_alive: F) -> Vec<(usize, usize)>
    where
        F: Fn(usize) -> bool,
    {
        let mut moves = Vec::new();
        let mut left = 0;
        let mut right = self.data.len().saturating_sub(1);

        while left < right {
            // تحريك المؤشر الأيسر للبحث عن "فجوة" (كيان ميت)
            while left < right && is_alive(left) {
                left += 1;
            }
            // تحريك المؤشر الأيمن للبحث عن "كيان حي" لنقله
            while left < right && !is_alive(right) {
                right -= 1;
            }

            if left < right {
                // سد الفجوة: نقل البيانات من اليمين لليسار
                self.data[left] = self.data[right];
                self.data[right] = T::default(); // تصفير المكان القديم
                
                // تسجيل عملية النقل (من -> إلى) لكي يقوم المحرك بتحديث مقابض (Handles) الكيانات
                moves.push((right, left));
                
                left += 1;
                right -= 1;
            }
        }
        moves // نعيد خريطة التحركات
    }
}

// =======================================================
// اختبار الطبقة الخامسة
// =======================================================
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_defragmentation() {
        println!("\n--- Mirage Layer 5: Defragmentation Test ---");
        let mut page = ColumnarPage::<f32>::new(10);
        
        // إعطاء قيم للكيانات (0, 10, 20, ..., 90)
        for i in 0..10 { page.update(i, i as f32 * 10.0); }

        // لنفترض أن الكيانات رقم (3, 4, 5, 6, 7) تم تدميرها في المعركة
        let is_alive = |idx| !(3..=7).contains(&idx);

        // تشغيل نظام الـ Defragmentation
        let moves = page.defragment(is_alive);

        println!("Dead entities detected. Triggering Smart Packing...");
        println!("Memory Moves mapping: {:?}", moves);
        
        // الكيان 9 يجب أن يذهب ليسد الفجوة 3
        assert_eq!(page.data[3], 90.0);
        // الكيان 8 يجب أن يذهب ليسد الفجوة 4
        assert_eq!(page.data[4], 80.0);
        
        println!("Result: Memory is 100% Packed! Cache is happy. ⚡");
    }
}