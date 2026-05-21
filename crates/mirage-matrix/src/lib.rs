// ===================================================================
// ملف: crates/mirage-matrix/src/lib.rs
// الوظيفة: النخاع الشوكي للمحرك (Neural Matrix) - عقل الـ Adaptive Runtime
// ===================================================================

pub mod bus;

// Topology moved to mirage-mts; mirage-matrix contains math primitives only.

use mirage_core::pool::{RuntimeDirectory, Handle, AddressMapping};
use std::collections::HashMap;

/// 🧠 Neural Matrix (المصفوفة العصبية): تدير العلاقات بين المقابض لتفعيل الـ Zero-Cost Dormancy
pub struct NeuralMatrix {
    /// Dependency Graph (مخطط الاعتماديات): أي مقبض يؤثر على أي مقبض آخر؟
    dependencies: HashMap<Handle, Vec<Handle>>,
}

impl NeuralMatrix {
    pub fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
        }
    }

    /// ربط علاقة عصبية (Propagation Edge)
    pub fn connect(&mut self, source: Handle, target: Handle) {
        self.dependencies.entry(source).or_insert_with(Vec::new).push(target);
    }

    /// Pulse Trace (تتبع النبضة): معرفة الأثر التنبؤي للتغيير
    pub fn trace_impact(&self, source: Handle, directory: &RuntimeDirectory) -> Vec<AddressMapping> {
        let mut impacts = Vec::new();

        if let Some(targets) = self.dependencies.get(&source) {
            for target_handle in targets {
                if let Some(mapping) = directory.get_mapping(*target_handle) {
                    impacts.push(mapping);
                }
            }
        }
        impacts
    }
}

// استدعاء الماكرو المحدث من المصنع
pub use mirage_matrix_macros::NeuralCluster;

#[derive(NeuralCluster)]
pub struct PlayerTransform {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    const NUM_CHUNKS: u32 = 1024; // Local test const: number of chunks

    #[test]
    fn test_mava_to_matrix_synapse() {
        // ✅ إصلاح: تمرير عدد الكتل للمنشئ ليتوافق مع تحديث Core
        let mut directory = RuntimeDirectory::new(NUM_CHUNKS as usize);
        let mut matrix = NeuralMatrix::new();

        let player = PlayerTransform { x: 0.0, y: 0.0, z: 0.0 };
        
        // 1. توليد المقابض للكيان باستخدام الماكرو
        let handles = player.wire_to_matrix(&mut matrix, &mut directory);
        
        if handles.len() >= 2 {
            // 2. ربط الـ x بالـ y مثلاً
            matrix.connect(handles[0], handles[1]);
            
            // 3. اختبار النبضة
            let impacts = matrix.trace_impact(handles[0], &directory);
            assert_eq!(impacts.len(), 1, "يجب أن يتأثر كيان واحد فقط");
            println!("🚀 Matrix Trace Success! Affected: {:?}", impacts);
        }
    }
}