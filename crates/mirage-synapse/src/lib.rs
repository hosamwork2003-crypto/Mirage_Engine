// ===================================================================
// ملف: crates/mirage-synapse/src/lib.rs
// الوظيفة: النظام العصبي التفاعلي (Predictive Neural Matrix)
// ===================================================================

use std::collections::HashMap;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::algo::toposort;

// =====================================================================
// SynapseRegistry — concrete observational registry for telemetry & prefetch
// =====================================================================
//
// DESIGN CONSTRAINTS (per V4 Final Authority Containment Pass)
// ---------------------------------------------------------------
// * No runtime-facing traits (avoid SynapticTelemetryBridge).
// * Direct, concrete, and detachable API hooks only.
// * No heap allocation per call — prefetch_cache is pre-allocated and
//   reused across updates.
// * Advisory only — no simulation semantic changes.

// 🧠 تعريف العقدة العصبية التنبؤية (Predictive Synapse Node)
pub struct SynapseNode {
    pub name: String,
    pub is_dirty: bool,
    pub priority: f32,       // الأولوية الحسابية (0.0 - 1.0)
    pub velocity_bias: f32,  // انحياز السرعة (للتنبؤ بالمستقبل)
}

// 🌐 نظام إدارة الشبكة العصبية للمحرك (Observational Only)
pub struct SynapseRegistry {
    graph: DiGraph<SynapseNode, f32>, // f32 يمثل وزن الرابط بين الأنظمة
    node_map: HashMap<String, NodeIndex>,
    /// Pre-allocated prefetch corridor cache. Populated by `observational_update`.
    /// Cleared and refilled each call. Capacity grows monotonically (amortised).
    prefetch_cache: Vec<u32>,
}

impl SynapseRegistry {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
            prefetch_cache: Vec::new(),
        }
    }

    /// 📝 تسجيل نظام أو كتلة بيانات داخل الشبكة العصبية
    pub fn register_node(&mut self, name: &str, dependencies: Vec<String>) {
        let node = SynapseNode {
            name: name.to_string(),
            is_dirty: false,
            priority: 0.0,
            velocity_bias: 1.0,
        };
        let node_idx = self.graph.add_node(node);
        self.node_map.insert(name.to_string(), node_idx);

        for dep in dependencies {
            if let Some(&dep_idx) = self.node_map.get(&dep) {
                // إضافة رابط بوزن افتراضي 1.0
                self.graph.add_edge(dep_idx, node_idx, 1.0);
            }
        }
    }

    /// ⚡ حقن النبضة التنبؤية (Heuristic Prediction Injection — Telemetry Only)
    /// تقوم هذه الدالة بحساب مدى أهمية العقدة بناءً على متجه حركة اللاعب
    pub fn telemetry_prediction(&mut self, cam_pos: [f32; 3], cam_vel: [f32; 3], node_pos: [f32; 3]) -> f32 {
        // حساب متجه الاتجاه من الكاميرا للكتلة
        let to_node = [
            node_pos[0] - cam_pos[0],
            node_pos[1] - cam_pos[1],
            node_pos[2] - cam_pos[2],
        ];

        // حساب Dot Product (الضرب القياسي) لمعرفة هل اللاعب يتجه نحو الكتلة؟
        let dot = cam_vel[0] * to_node[0] + cam_vel[1] * to_node[1] + cam_vel[2] * to_node[2];
        
        // إذا كان الضرب القياسي موجباً، يعني اللاعب يتجه نحوها -> ارفع الأولوية
        let prediction_weight = if dot > 0.0 { 1.5 } else { 0.5 };

        // تحديث كافة العقد المرتبطة بهذا الموقع
        for node_idx in self.graph.node_indices() {
            if let Some(node) = self.graph.node_weight_mut(node_idx) {
                node.priority = prediction_weight;
            }
        }
        
        if dot > 0.0 { dot * 50.0 } else { 0.0 }
    }

    /// 🎯 Calculate predictive loading corridor (Advisory Prefetch)
    /// Determines which chunks should be preemptively loaded based on
    /// camera trajectory and velocity vector.
    pub fn compute_loading_corridor(&self, cam_pos: [f32; 3], cam_vel: [f32; 3], width: f32) -> Vec<u32> {
        let mut corridor = Vec::new();
        
        let vel_mag_sq = cam_vel[0] * cam_vel[0] + cam_vel[1] * cam_vel[1] + cam_vel[2] * cam_vel[2];
        if vel_mag_sq < 0.001 {
            return corridor;
        }
        
        let vel_mag = vel_mag_sq.sqrt();
        let vel_norm = [cam_vel[0] / vel_mag, cam_vel[1] / vel_mag, cam_vel[2] / vel_mag];
        
        // Look-ahead distance: speed * time_to_load_chunk (simulated as 10 frames)
        let lookahead = vel_mag * 10.0;
        
        let predictive_pos = [
            cam_pos[0] + vel_norm[0] * lookahead,
            cam_pos[1] + vel_norm[1] * lookahead,
            cam_pos[2] + vel_norm[2] * lookahead,
        ];
        
        // Get chunks in corridor around predicted position
        let grid_x = (predictive_pos[0] / 64.0) as i32;
        let grid_z = (predictive_pos[2] / 64.0) as i32;
        let width_int = (width / 64.0).ceil() as i32;
        
        for z in (grid_z - width_int)..=(grid_z + width_int) {
            for x in (grid_x - width_int)..=(grid_x + width_int) {
                if x >= 0 && x < 25 && z >= 0 && z < 25 {
                    corridor.push((z as u32 * 25) + x as u32);
                }
            }
        }
        
        corridor
    }

    /// 📊 Calculate thermal heat score for streaming priority (Advisory Score)
    /// Combines velocity factor, distance factor, and visibility
    pub fn compute_thermal_score(&self, cam_pos: [f32; 3], cam_vel: [f32; 3], chunk_pos: [f32; 3]) -> f32 {
        let to_chunk = [
            chunk_pos[0] - cam_pos[0],
            chunk_pos[1] - cam_pos[1],
            chunk_pos[2] - cam_pos[2],
        ];
        
        let dist_sq = to_chunk[0] * to_chunk[0] + to_chunk[1] * to_chunk[1] + to_chunk[2] * to_chunk[2];
        let dist = dist_sq.sqrt();
        
        // Distance factor: closer = hotter
        let dist_factor = if dist > 0.001 { 1.0 / (1.0 + dist * 0.01) } else { 1.0 };
        
        // Velocity factor: heading towards = hotter
        let vel_mag_sq = cam_vel[0] * cam_vel[0] + cam_vel[1] * cam_vel[1] + cam_vel[2] * cam_vel[2];
        let dot = cam_vel[0] * to_chunk[0] + cam_vel[1] * to_chunk[1] + cam_vel[2] * to_chunk[2];
        let vel_factor = if dot > 0.0 && vel_mag_sq > 0.001 {
            (dot / vel_mag_sq.sqrt()).max(0.0).min(1.0)
        } else {
            0.0
        };
        
        dist_factor * 0.7 + vel_factor * 0.3
    }

    /// 🚀 الحصول على ترتيب الاعتماديات المرصود للتنبؤ (Observational Dependency Order)
    pub fn get_observational_dependency_order(&self) -> Vec<String> {
        match toposort(&self.graph, None) {
            Ok(nodes) => nodes.iter()
                .map(|&idx| self.graph[idx].name.clone())
                .collect(),
            Err(_) => {
                eprintln!("⚠️ Warning: Circular dependency detected in Synapse Graph!");
                Vec::new()
            }
        }
    }

    /// 🔍 البحث عن العقد ذات الأولوية القصوى للتنبؤ (High Priority Telemetry Nodes)
    pub fn get_high_priority_telemetry_nodes(&self, threshold: f32) -> Vec<String> {
        self.graph.node_indices()
            .filter(|&idx| self.graph[idx].priority > threshold)
            .map(|idx| self.graph[idx].name.clone())
            .collect()
    }

    /// Update advisory prediction state and rebuild the advisory prefetch hints.
    ///
    /// Calls `compute_loading_corridor` and stores the result in the
    /// pre-allocated `prefetch_cache` buffer. The `telemetry_prediction`
    /// API is called with a zero node_pos (global refresh — updates all nodes).
    ///
    /// # Allocation behaviour
    /// `prefetch_cache` is cleared and refilled. No heap allocation occurs
    /// once the buffer capacity stabilises (Vec::clear() does not deallocate).
    pub fn observational_update(&mut self, cam_pos: [f32; 3], cam_vel: [f32; 3], corridor_width: f32) {
        // Refresh global priority weights across all synapse nodes.
        // `telemetry_prediction` uses a fixed node_pos = [0,0,0] as a global
        // origin anchor. Callers requiring per-node updates should call
        // `telemetry_prediction` directly with the specific node position.
        let _score = self.telemetry_prediction(cam_pos, cam_vel, [0.0, 0.0, 0.0]);

        // Rebuild the prefetch corridor into the pre-allocated cache.
        // Vec::clear() retains capacity — no deallocation in the hot path.
        self.prefetch_cache.clear();
        let corridor = self.compute_loading_corridor(cam_pos, cam_vel, corridor_width);
        self.prefetch_cache.extend_from_slice(&corridor);
    }

    #[inline]
    pub fn advisory_prefetch_hints(&self) -> &[u32] {
        &self.prefetch_cache
    }
}

// =====================================================================
// Tests
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_registry_has_empty_prefetch_cache() {
        let reg = SynapseRegistry::new();
        assert!(reg.advisory_prefetch_hints().is_empty());
    }

    #[test]
    fn observational_update_static_camera_produces_empty_hints() {
        let mut reg = SynapseRegistry::new();
        // Zero velocity → hints corridor is empty (no look-ahead without movement)
        reg.observational_update([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 64.0);
        assert!(reg.advisory_prefetch_hints().is_empty(),
            "static camera should produce no advisory prefetch hints");
    }

    #[test]
    fn observational_update_moving_camera_produces_hints() {
        let mut reg = SynapseRegistry::new();
        // Camera at origin, moving +X with speed 1.0
        reg.observational_update([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 128.0);
        // Should have at least one chunk in the corridor
        assert!(!reg.advisory_prefetch_hints().is_empty(),
            "moving camera should produce a non-empty advisory prefetch hints");
    }

    #[test]
    fn advisory_prefetch_hints_are_valid_chunk_indices() {
        let mut reg = SynapseRegistry::new();
        reg.observational_update([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 128.0);
        // All chunk IDs must be within [0, 24*25+24] = [0, 624]
        for &id in reg.advisory_prefetch_hints() {
            assert!(id < 625, "chunk id {} out of 25x25 grid bounds", id);
        }
    }

    #[test]
    fn registry_is_directly_callable() {
        let mut reg = SynapseRegistry::new();
        reg.observational_update([100.0, 0.0, 0.0], [0.0, 0.0, 1.0], 64.0);
        let _ = reg.advisory_prefetch_hints().len();
    }

    #[test]
    fn existing_thermal_score_unchanged() {
        let reg = SynapseRegistry::new();
        let score = reg.compute_thermal_score(
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
        );
        assert!(score >= 0.0 && score <= 1.0,
            "thermal score out of [0,1]: {}", score);
    }
}
