use std::any::Any;
use uuid::Uuid;
use petgraph::graph::DiGraph;

/// 🌡️ تتبع "حرارة" البيانات (Behavioral Reflection)
/// يسجل بدقة متى وكم مرة تم تعديل أو قراءة هذا المتغير
#[derive(Debug, Clone, Default)]
pub struct DataTelemetry {
    pub read_count: u64,
    pub write_count: u64,
    pub last_mutation_frame: u64,
}

/// ⚙️ استراتيجية التنفيذ (Hardware Reflection)
/// يقرر المحرك بناءً عليها كيف سيعالج هذا المتغير
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ExecutionStrategy {
    Scalar,     // المعالجة العادية (للمتغيرات الباردة/البطيئة)
    Simd,       // المعالجة المتوازية للـ CPU (للمتغيرات الساخنة)
    GpuCompute, // المعالجة عبر كارت الشاشة (للمتغيرات الكثيفة جداً)
}

/// 🧬 العقدة العصبية: الوحدة الأساسية في Mirage Matrix
/// أي حقل (Field) في المحرك سيتحول إلى NeuralNode
pub trait NeuralNode: Any + Send + Sync {
    fn id(&self) -> Uuid;
    fn telemetry(&self) -> &DataTelemetry;
    fn strategy(&self) -> ExecutionStrategy;
    
    /// ⚡ النبضة: تُستدعى عندما تتغير قيمة المتغير
    /// لتنبيه الـ Matrix بضرورة تحديث الأنظمة المعتمدة عليه
    fn pulse(&mut self, frame: u64);
    
    fn as_any(&self) -> &dyn Any;
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// 🕸️ المصفوفة العصبية (The Mutation Graph)
/// المخ الحقيقي للمحرك، حيث تُدار التبعيات (Dependencies)
pub struct NeuralMatrix {
    /// نستخدم (Directed Graph) لتمثيل اتجاه تدفق البيانات
    /// مثلاً: Position -> Physics -> Renderer
    pub graph: DiGraph<Uuid, &'static str>,
    pub frame_count: u64,
}

impl NeuralMatrix {

/// 🔗 إنشاء مشبك عصبي (Synapse) بين عقدتين
    pub fn create_synapse(
        &mut self, 
        from: petgraph::graph::NodeIndex, 
        to: petgraph::graph::NodeIndex, 
        relation: &'static str
    ) {
        // إضافة سهم (Edge) يحدد اتجاه تأثير البيانات
        self.graph.add_edge(from, to, relation);
    }

    /// ⚡ تتبع أثر التغيير (Impact Tracing)
    /// عندما تتغير عقدة، من سيتأثر؟
    pub fn trace_impact(&self, source: petgraph::graph::NodeIndex) {
        println!("🔍 Mutation detected! Tracing data flow impact...");
        
        // البحث عن كل العقد المرتبطة بهذه العقدة
        let mut affected_count = 0;
        for neighbor in self.graph.neighbors(source) {
            affected_count += 1;
            let target_uuid = self.graph[neighbor];
            println!("   ⚡ Pulse propagates to Node [{}]: {}", neighbor.index(), target_uuid);
        }
        
        if affected_count == 0 {
            println!("   🛑 Dead end. No systems depend on this data.");
        }
    }

    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            frame_count: 0,
        }
    }

    /// وظيفة لزيادة عداد الإطارات (يتم استدعاؤها في الـ Main Loop)
    pub fn tick(&mut self) {
        self.frame_count += 1;
        // هنا سيتم لاحقاً تقييم الـ Telemetry وتغيير الـ ExecutionStrategy ديناميكياً
    }

    pub fn dispatch_memory_pulse(&mut self, mutation_indices: Vec<u32>) {
        if mutation_indices.is_empty() { return; }

        println!("🧠 Matrix: Processing {} mutations from MAVA Pool...", mutation_indices.len());

        for index in mutation_indices {
            // نحتاج لتحويل الـ Index الخاص بالـ Pool إلى NodeIndex في الـ Graph
            // سنفترض حالياً وجود Mapping بسيط أو استخدام الـ Index مباشرة للاختبار
            let node_idx = petgraph::graph::NodeIndex::new(index as usize);
            
            // إطلاق النبضة التفاعلية
            self.trace_impact(node_idx);
        }
    }

}

// 1. استدعاء الماكرو من المصنع
pub use mirage_matrix_macros::NeuralCluster;

// 2. التجربة السحرية: تعريف كائن عادي جداً، ولكن نضع فوقه تاج الـ Matrix!
#[derive(NeuralCluster)]
pub struct PlayerTransform {
    pub x: f32,
    pub y: f32,
    pub z: f32,
}

// 3. كتابة اختبار (Test) لنرى النتيجة بأعيننا
#[cfg(test)]
mod integration_tests {
    use super::*;
    use mirage_core::oasis::OasisVirtualPage;
    use mirage_math::batch::Vector3Batch16;
    use std::io::Write;

 #[test]
fn test_mava_to_matrix_synapse() {
    use mirage_core::pool::Pool;
    use mirage_core::pool::handle::Handle;

    // 1. إعداد الذاكرة والعقل
    let mut pool: Pool<f32> = Pool::new();
    let mut matrix = NeuralMatrix::new();

    // 2. تسجيل كائن وهمي (كأنه قادم من Oasis)
    let handle = pool.register_entity(1, 0, 0); 
    let node_idx = matrix.graph.add_node(uuid::Uuid::new_v4());

    // 3. محاكاة تعديل الكائن في الـ Pool
    println!("🛠️ Modifying entity in Pool...");
    pool.mark_dirty(handle);

    // 4. نهاية الفريم: ترحيل التعديلات للمصفوفة العصبية
    let dirty_indices = pool.commit_to_matrix();
    matrix.dispatch_memory_pulse(dirty_indices);

    println!("✅ Synapse Test Complete: Memory mutation triggered matrix pulse.");
}

}