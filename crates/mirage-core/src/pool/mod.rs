// ===================================================================
// ملف: mod.rs (نظام MAVA - Mirage Adaptive Virtual Arena)
// الوظيفة: النخاع الشوكي للمحرك الذي يربط Oasis و Math و Matrix
// ===================================================================

pub mod handle;
pub use handle::Handle;

use crate::oasis::OasisVirtualPage;
use std::marker::PhantomData;

/// 🗺️ خريطة العنوان (Address Mapping)
#[derive(Debug, Clone, Copy)]
pub struct AddressMapping {
    pub chunk_id: u32,    
    pub slot_id: u32,     
    pub generation: u32,  
}

/// 🌌 Mirage Adaptive Virtual Arena (MAVA)
pub struct Pool<T> {
    directory: Vec<AddressMapping>,
    chunks: Vec<OasisVirtualPage>,
    mutation_journal: Vec<u32>,
    free_slots: Vec<u32>,
    _marker: PhantomData<T>,
}

impl<T> Pool<T> {

pub fn add_chunk(&mut self, chunk: OasisVirtualPage) {
        self.chunks.push(chunk);
    }

    /// 1. إنشاء مجمع ذاكرة تكيفي جديد
    pub fn new() -> Self {
        Self {
            directory: Vec::new(),
            chunks: Vec::new(),
            mutation_journal: Vec::new(),
            free_slots: Vec::new(),
            _marker: PhantomData,
        }
    }

    /// 2. الحصول على العنوان الفيزيائي من المقبض
    #[inline(always)]

pub fn get_dirty_journal(&self) -> &[u32] {
        &self.mutation_journal
    }

pub fn get_by_index(&self, index: u32) -> Option<&T> 
    where T: bytemuck::Pod 
    {
        let mapping = self.directory.get(index as usize)?;
        let chunk = self.chunks.get(mapping.chunk_id as usize)?;
        let data_slice: &[T] = chunk.cast_to_math_batches::<T>();
        data_slice.get(mapping.slot_id as usize)
    }

    pub fn get_mut(&mut self, handle: Handle<T>) -> Option<&mut T> 
    where T: bytemuck::Pod 
    {
        let mapping = self.get_address(handle)?;
        let chunk = self.chunks.get_mut(mapping.chunk_id as usize)?;
        let data_slice: &mut [T] = chunk.cast_to_math_batches_mut::<T>();
        data_slice.get_mut(mapping.slot_id as usize)
    }

pub fn get_mut_by_index(&mut self, index: u32) -> Option<&mut T> 
    where T: bytemuck::Pod 
    {
        // 1. استخراج المسار من الدليل
        let mapping = self.directory.get(index as usize)?;
        
        // 2. الحصول على الكتلة كمرجع قابل للتعديل (&mut)
        let chunk = self.chunks.get_mut(mapping.chunk_id as usize)?;
        
        // 3. تحويل البيانات (Cast) - الآن ستعمل لأننا أضفناها في Oasis
        let data_slice: &mut [T] = chunk.cast_to_math_batches_mut::<T>();
        
        // 4. إرجاع الخانة المطلوبة
        data_slice.get_mut(mapping.slot_id as usize)
    }

    pub fn get_address(&self, handle: Handle<T>) -> Option<AddressMapping> {
        let mapping = self.directory.get(handle.index() as usize)?;
        if mapping.generation != handle.generation() {
            return None;
        }
        Some(*mapping)
    }

    /// 3. استحضار الكائن لحظياً من الذاكرة الافتراضية (Oasis Connection)
    pub fn get(&self, handle: Handle<T>) -> Option<&T> 
    where T: bytemuck::Pod 
    {
        let mapping = self.get_address(handle)?;
        let chunk = self.chunks.get(mapping.chunk_id as usize)?;
        
        // Zero-Copy Cast: تحويل البيانات الخام إلى نوع T عند اللمس
        let data_slice: &[T] = chunk.cast_to_math_batches::<T>();
        data_slice.get(mapping.slot_id as usize)
    }

    /// 4. تسجيل كائن جديد في الدليل
    pub fn register_entity(&mut self, generation: u32, chunk_id: u32, slot_id: u32) -> Handle<T> {
        let index = if let Some(i) = self.free_slots.pop() {
            i
        } else {
            self.directory.len() as u32
        };

        let mapping = AddressMapping { chunk_id, slot_id, generation };

        if (index as usize) < self.directory.len() {
            self.directory[index as usize] = mapping;
        } else {
            self.directory.push(mapping);
        }

        Handle::new(index, generation)
    }

    /// 5. تسجيل التعديلات للنبضة القادمة
    #[inline(always)]
    pub fn mark_dirty(&mut self, handle: Handle<T>) {
        self.mutation_journal.push(handle.index());
    }

    /// 6. إرسال التعديلات للمصفوفة (Matrix Pulse)
    pub fn commit_to_matrix(&mut self) -> Vec<u32> {
        std::mem::take(&mut self.mutation_journal)
    }
}

// تنفيذ الـ Default
impl<T> Default for Pool<T> {
    fn default() -> Self {
        Self::new()
    }
}