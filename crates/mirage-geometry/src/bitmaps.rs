// Layer 1: Hierarchical Dirty Bitmaps
// وظيفته: تتبع العناصر المتغيرة للقفز فوق العناصر الساكنة بسرعة O(K)

pub struct DirtyBitmap {
    mask: u64,
}

impl DirtyBitmap {
    pub fn new() -> Self {
        Self { mask: 0 }
    }

    // رفع راية التغيير لعنصر معين
    #[inline(always)]
    pub fn set_dirty(&mut self, index: usize) {
        if index < 64 {
            self.mask |= 1 << index;
        }
    }

    // تنظيف جميع الرايات بعد المعالجة
    pub fn clear(&mut self) {
        self.mask = 0;
    }

    // القفز مباشرة لأول بت "قذر" باستخدام تعليمات المعالج
    pub fn iter_dirty(&self) -> BitIter {
        BitIter(self.mask)
    }
}

pub struct BitIter(u64);

impl Iterator for BitIter {
    type Item = usize;
    fn next(&mut self) -> Option<Self::Item> {
        if self.0 == 0 { return None; }
        // استخدام TZCNT (Trailing Zeros Count) للوصول للتغيير التالي فوراً
        let bit = self.0.trailing_zeros() as usize;
        self.0 &= self.0 - 1; // إزالة البت الذي تمت معالجته
        Some(bit)
    }
}