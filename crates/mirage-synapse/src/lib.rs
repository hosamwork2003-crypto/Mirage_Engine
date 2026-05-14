use std::collections::HashMap;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::algo::toposort;

// تعريف العقدة التفاعلية (Reactive Node)
pub struct SynapseNode {
    pub name: String,
    pub is_dirty: bool,
}

// نظام إدارة الاعتماديات التفاعلي
pub struct SynapseRegistry {
    // بناء المخطط (Graph) باستخدام petgraph
    graph: DiGraph<SynapseNode, ()>,
    // قاموس للوصول السريع للعقد بالاسم
    node_map: HashMap<String, NodeIndex>,
}

impl SynapseRegistry {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
        }
    }

    // إضافة نظام جديد للمخطط
    pub fn register_system(&mut self, name: &str, dependencies: Vec<String>) {
        let node = SynapseNode {
            name: name.to_string(),
            is_dirty: false,
        };
        let node_idx = self.graph.add_node(node);
        self.node_map.insert(name.to_string(), node_idx);

        // ربط الاعتماديات
        for dep in dependencies {
            if let Some(&dep_idx) = self.node_map.get(&dep) {
                self.graph.add_edge(dep_idx, node_idx, ());
            }
        }
    }

    // رفع راية التغيير (Dirty) لعقدة معينة وتنبيه المعتمدين عليها
    pub fn mark_dirty(&mut self, name: &str) {
        if let Some(&idx) = self.node_map.get(name) {
            if let Some(node) = self.graph.node_weight_mut(idx) {
                node.is_dirty = true;
            }
        }
    }

    // جلب الترتيب الصحيح للتنفيذ (Topological Sort)
    // لضمان أن الأنظمة تنفذ بعد اعتمادياتها دائماً
    pub fn get_execution_order(&self) -> Vec<String> {
        let sorted = toposort(&self.graph, None).unwrap();
        sorted.iter()
            .map(|&idx| self.graph.node_weight(idx).unwrap().name.clone())
            .collect()
    }
}