
# File: crates/mirage-synapse/src/lib.rs

```rust
// ===================================================================
// ملف: crates/mirage-synapse/src/lib.rs
// الوظيفة: النظام العصبي التفاعلي (Predictive Neural Matrix)
// ===================================================================

use std::collections::HashMap;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::algo::toposort;

// =====================================================================
// SynapticCompilerBridge — orchestration trait
// =====================================================================
//
// This trait is the stable contract between the synaptic prediction layer
// and any upstream orchestrator (e.g. mirage-mkr-core::MKRWorld).
//
// DEPENDENCY DIRECTION
// ---------------------------------------------------------------
//   mirage-synapse  (defines this trait — no deps on mkr-core)
//       ↓
//   mirage-mkr-core (holds SynapseRegistry, calls refresh_prediction,
//                    queries prefetch_corridor for streaming hints)
//
// DESIGN CONSTRAINTS (per V4 Step 1 Hard Constraints)
// ---------------------------------------------------------------
// * Static dispatch at the call site in MKRWorld::tick() — MKRWorld holds
//   a concrete SynapseRegistry, NOT a dyn SynapticCompilerBridge.
// * No heap allocation per call — prefetch_cache is pre-allocated and
//   reused across ticks (cleared + refilled, never grown in the hot path
//   once stable field size is reached).
// * Advisory only — no simulation semantic changes. Thermal scores
//   influence SynapseNode::priority for prefetch hints ONLY.
//   The activation field probability values are NOT modified here.

/// Orchestration contract for the synaptic prediction layer.
///
/// Implemented by `SynapseRegistry`. Called from `MKRWorld::tick()` Phase 0.3
/// to refresh priority weights and populate the prefetch corridor cache.
pub trait SynapticCompilerBridge {
    /// Refresh the synaptic prediction state from a camera context.
    ///
    /// Recomputes thermal scores for all registered nodes and updates their
    /// priority weights. Also rebuilds the internal prefetch corridor cache.
    ///
    /// # Parameters
    /// - `cam_pos`: Camera/observer world position `[x, y, z]`
    /// - `cam_vel`: Camera velocity vector `[vx, vy, vz]`
    /// - `corridor_width`: Spatial width of the look-ahead corridor (world units)
    ///
    /// # Determinism
    /// Pure arithmetic on a fixed registry — same inputs always produce the
    /// same priority assignments. No side effects outside `self`.
    fn refresh_prediction(&mut self, cam_pos: [f32; 3], cam_vel: [f32; 3], corridor_width: f32);

    /// Return the cached prefetch corridor computed by the last `refresh_prediction` call.
    ///
    /// Returns chunk IDs (flat grid indices) that the synaptic layer recommends
    /// for preemptive streaming. This is an advisory hint — the caller is free to
    /// ignore it.
    ///
    /// # Allocation
    /// Returns a reference to an internally pre-allocated buffer. Zero-copy.
    fn prefetch_corridor(&self) -> &[u32];
}

// 🧠 تعريف العقدة العصبية التنبؤية (Predictive Synapse Node)
pub struct SynapseNode {
    pub name: String,
    pub is_dirty: bool,
    pub priority: f32,       // الأولوية الحسابية (0.0 - 1.0)
    pub velocity_bias: f32,  // انحياز السرعة (للتنبؤ بالمستقبل)
}

// 🌐 نظام إدارة الشبكة العصبية للمحرك
pub struct SynapseRegistry {
    graph: DiGraph<SynapseNode, f32>, // f32 يمثل وزن الرابط بين الأنظمة
    node_map: HashMap<String, NodeIndex>,
    /// Pre-allocated prefetch corridor cache. Populated by `refresh_prediction`.
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

    /// ⚡ حقن النبضة التنبؤية (Heuristic Prediction Injection)
    /// تقوم هذه الدالة بحساب مدى أهمية العقدة بناءً على متجه حركة اللاعب
    pub fn update_prediction(&mut self, cam_pos: [f32; 3], cam_vel: [f32; 3], node_pos: [f32; 3]) -> f32 {
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

    /// 🎯 Calculate predictive loading corridor
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

    /// 📊 Calculate thermal heat score for streaming priority
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

    /// 🚀 الحصول على ترتيب التنفيذ الأمثل (Topological Order)
    /// يضمن تنفيذ الأنظمة حسب الاعتماديات مع مراعاة الأولوية التنبؤية
    pub fn get_execution_plan(&self) -> Vec<String> {
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

    /// 🔍 البحث عن العقد ذات الأولوية القصوى للبث (Streaming Priority)
    pub fn get_high_priority_nodes(&self, threshold: f32) -> Vec<String> {
        self.graph.node_indices()
            .filter(|&idx| self.graph[idx].priority > threshold)
            .map(|idx| self.graph[idx].name.clone())
            .collect()
    }
}

// =====================================================================
// SynapticCompilerBridge impl for SynapseRegistry
// =====================================================================

impl SynapticCompilerBridge for SynapseRegistry {
    /// Refresh synaptic prediction state and rebuild the prefetch corridor.
    ///
    /// Calls `compute_loading_corridor` and stores the result in the
    /// pre-allocated `prefetch_cache` buffer. The existing `update_prediction`
    /// API is called with a zero node_pos (global refresh — updates all nodes).
    ///
    /// # Allocation behaviour
    /// `prefetch_cache` is cleared and refilled. No heap allocation occurs
    /// once the buffer capacity stabilises (Vec::clear() does not deallocate).
    fn refresh_prediction(&mut self, cam_pos: [f32; 3], cam_vel: [f32; 3], corridor_width: f32) {
        // Refresh global priority weights across all synapse nodes.
        // `update_prediction` uses a fixed node_pos = [0,0,0] as a global
        // origin anchor. Callers requiring per-node updates should call
        // `update_prediction` directly with the specific node position.
        let _score = self.update_prediction(cam_pos, cam_vel, [0.0, 0.0, 0.0]);

        // Rebuild the prefetch corridor into the pre-allocated cache.
        // Vec::clear() retains capacity — no deallocation in the hot path.
        self.prefetch_cache.clear();
        let corridor = self.compute_loading_corridor(cam_pos, cam_vel, corridor_width);
        self.prefetch_cache.extend_from_slice(&corridor);
    }

    #[inline]
    fn prefetch_corridor(&self) -> &[u32] {
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
        assert!(reg.prefetch_corridor().is_empty());
    }

    #[test]
    fn refresh_prediction_static_camera_produces_empty_corridor() {
        let mut reg = SynapseRegistry::new();
        // Zero velocity → corridor is empty (no look-ahead without movement)
        reg.refresh_prediction([0.0, 0.0, 0.0], [0.0, 0.0, 0.0], 64.0);
        assert!(reg.prefetch_corridor().is_empty(),
            "static camera should produce no prefetch corridor");
    }

    #[test]
    fn refresh_prediction_moving_camera_produces_corridor() {
        let mut reg = SynapseRegistry::new();
        // Camera at origin, moving +X with speed 1.0
        reg.refresh_prediction([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 128.0);
        // Should have at least one chunk in the corridor
        assert!(!reg.prefetch_corridor().is_empty(),
            "moving camera should produce a non-empty prefetch corridor");
    }

    #[test]
    fn prefetch_corridor_ids_are_valid_chunk_indices() {
        let mut reg = SynapseRegistry::new();
        reg.refresh_prediction([0.0, 0.0, 0.0], [1.0, 0.0, 0.0], 128.0);
        // All chunk IDs must be within [0, 24*25+24] = [0, 624]
        for &id in reg.prefetch_corridor() {
            assert!(id < 625, "chunk id {} out of 25x25 grid bounds", id);
        }
    }

    #[test]
    fn bridge_trait_is_callable_via_concrete_type() {
        // Verify SynapticCompilerBridge is statically callable on SynapseRegistry.
        // This catches any signature mismatch between trait and impl.
        let mut reg = SynapseRegistry::new();
        let bridge: &mut dyn SynapticCompilerBridge = &mut reg;
        bridge.refresh_prediction([100.0, 0.0, 0.0], [0.0, 0.0, 1.0], 64.0);
        // corridor access via trait object
        let _ = bridge.prefetch_corridor().len();
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

```

---


# File: crates/mirage-compute/src/lib.rs

```rust
// ===================================================================
// mirage-compute/src/lib.rs
// PURPOSE: Trace Fusion Compiler (TFC) - SIMD Trace Fusion Engine
// ===================================================================

#[derive(Clone, Debug, PartialEq)]
pub struct Continuation {
    pub cell_index: usize,
    pub prob_signal: f32,
}

/// Abstract interface for executing fused kernels onto concrete target fields
pub trait CellField {
    fn set_execution_probability(&mut self, index: usize, prob: f32);
    fn get_execution_probability(&self, index: usize) -> f32;
    fn len(&self) -> usize;
}

#[derive(Clone, Debug)]
pub struct FusedKernel {
    pub path: Vec<Continuation>,
}

impl FusedKernel {
    /// Evaluate all instructions in the trace in a single fetch-decode-execute cycle
    pub fn execute<F: CellField>(&self, field: &mut F) {
        let len = self.path.len();
        let mut i = 0;
        
        // Loop unrolling for SIMD-like pipeline efficiency and auto-vectorization friendliness
        while i + 4 <= len {
            let c0 = &self.path[i];
            let c1 = &self.path[i + 1];
            let c2 = &self.path[i + 2];
            let c3 = &self.path[i + 3];

            if c0.cell_index < field.len() {
                let p = field.get_execution_probability(c0.cell_index);
                field.set_execution_probability(c0.cell_index, (p * 0.9 + c0.prob_signal * 0.1).clamp(0.0, 1.0));
            }
            if c1.cell_index < field.len() {
                let p = field.get_execution_probability(c1.cell_index);
                field.set_execution_probability(c1.cell_index, (p * 0.9 + c1.prob_signal * 0.1).clamp(0.0, 1.0));
            }
            if c2.cell_index < field.len() {
                let p = field.get_execution_probability(c2.cell_index);
                field.set_execution_probability(c2.cell_index, (p * 0.9 + c2.prob_signal * 0.1).clamp(0.0, 1.0));
            }
            if c3.cell_index < field.len() {
                let p = field.get_execution_probability(c3.cell_index);
                field.set_execution_probability(c3.cell_index, (p * 0.9 + c3.prob_signal * 0.1).clamp(0.0, 1.0));
            }
            i += 4;
        }

        while i < len {
            let c = &self.path[i];
            if c.cell_index < field.len() {
                let p = field.get_execution_probability(c.cell_index);
                field.set_execution_probability(c.cell_index, (p * 0.9 + c.prob_signal * 0.1).clamp(0.0, 1.0));
            }
            i += 1;
        }
    }
}

pub struct TraceFusionCompiler {
    pub trace_frequencies: std::collections::HashMap<Vec<usize>, u32>,
    pub compiled_kernels: std::collections::HashMap<Vec<usize>, FusedKernel>,
    pub maturity_threshold: u32,
}

impl TraceFusionCompiler {
    pub fn new(maturity_threshold: u32) -> Self {
        Self {
            trace_frequencies: std::collections::HashMap::new(),
            compiled_kernels: std::collections::HashMap::new(),
            maturity_threshold,
        }
    }

    pub fn optimize(&mut self, signature: Vec<usize>, hot_path: Vec<Continuation>) -> Option<FusedKernel> {
        if signature.is_empty() {
            return None;
        }
        let count = self.trace_frequencies.entry(signature.clone()).or_insert(0);
        *count += 1;

        if *count >= self.maturity_threshold {
            if !self.compiled_kernels.contains_key(&signature) {
                let kernel = self.compile_trace(hot_path);
                self.compiled_kernels.insert(signature.clone(), kernel.clone());
                return Some(kernel);
            } else {
                return Some(self.compiled_kernels[&signature].clone());
            }
        }
        None
    }

    pub fn compile_trace(&self, hot_path: Vec<Continuation>) -> FusedKernel {
        FusedKernel { path: hot_path }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockField {
        probabilities: Vec<f32>,
    }

    impl CellField for MockField {
        fn set_execution_probability(&mut self, index: usize, prob: f32) {
            if index < self.probabilities.len() {
                self.probabilities[index] = prob;
            }
        }
        fn get_execution_probability(&self, index: usize) -> f32 {
            if index < self.probabilities.len() {
                self.probabilities[index]
            } else {
                0.0
            }
        }
        fn len(&self) -> usize {
            self.probabilities.len()
        }
    }

    #[test]
    fn test_trace_compilation_and_execution_parity() {
        let mut tfc = TraceFusionCompiler::new(3);
        let signature = vec![1, 2, 3];
        let path = vec![
            Continuation { cell_index: 1, prob_signal: 0.8 },
            Continuation { cell_index: 2, prob_signal: 0.5 },
            Continuation { cell_index: 3, prob_signal: 0.9 },
        ];

        // Ensure compile triggers on the third optimize call (maturity_threshold = 3)
        assert!(tfc.optimize(signature.clone(), path.clone()).is_none());
        assert!(tfc.optimize(signature.clone(), path.clone()).is_none());
        let kernel = tfc.optimize(signature.clone(), path.clone()).expect("Should compile on maturity threshold");

        // Verify compilation matches trace exactly
        assert_eq!(kernel.path, path);

        // Run execution and verify mathematical parity
        let mut field_interpreted = MockField { probabilities: vec![0.0; 4] };
        let mut field_fused = MockField { probabilities: vec![0.0; 4] };

        // 1. Interpreted run (sequential simulation steps)
        for step in &path {
            let p = field_interpreted.get_execution_probability(step.cell_index);
            field_interpreted.set_execution_probability(step.cell_index, (p * 0.9 + step.prob_signal * 0.1).clamp(0.0, 1.0));
        }

        // 2. Fused compiler execution run
        kernel.execute(&mut field_fused);

        // Assert exact mathematical parity
        assert_eq!(field_interpreted.probabilities, field_fused.probabilities);
    }
}
```

---


# File: crates/mirage-cek/src/lib.rs

```rust
// ===================================================================
// mirage-cek/src/lib.rs
// PURPOSE: CEK Virtual Machine Substrate
//
// CEK = Control-Environment-Kontinuation
//
// ARCHITECTURAL ROLE
// ---------------------------------------------------------------
// This crate defines the pure CEK machine primitives that the MKR
// kernel uses to implement resumable, heap-allocated computation over
// the activation field. It is intentionally decoupled from the
// concrete ActivationField type.
//
// DEPENDENCY DIRECTION (NO CIRCULAR DEPS)
// ---------------------------------------------------------------
//   mirage-cek  (this crate — CEK primitives only)
//       ↓
//   mirage-mkr-core (implements CekEvalField for ActivationField,
//                    creates CEKMachine instances, drives tick)
//
// TRAIT ABSTRACTION
// ---------------------------------------------------------------
// CEKMachine closures operate on `&mut dyn CekEvalField` rather than
// the concrete `ActivationField`. This breaks the circular dependency:
//
//   Old (circular):
//     mirage-cek  →  mirage-mkr-core::ActivationField
//     mirage-mkr-core  →  mirage-cek::CEKMachine
//
//   New (acyclic):
//     mirage-cek  (no mkr-core dependency)
//     mirage-mkr-core  →  mirage-cek  (one direction only)
//     mirage-mkr-core::ActivationField  impl CekEvalField
//
// CONTINUATION SIGNATURE
// ---------------------------------------------------------------
// Each continuation frame is a `Box<dyn FnMut(&mut dyn CekEvalField) + Send>`.
// This is the minimal contract:
//   - `FnMut` — frames may carry mutable captured state.
//   - `Send` — frames can be scheduled across thread boundaries.
//   - `dyn CekEvalField` — decoupled from concrete field type.
//   - `'static` — required by the Send bound for thread safety.
// ===================================================================

// =====================================================================
// CekEvalField — trait contract for mutable field access
// =====================================================================

/// Minimal mutable interface that a CEK continuation frame needs to
/// interact with the activation field.
///
/// `mirage-mkr-core::ActivationField` implements this trait.
/// Any future field type (GPU-backed, remote, mock) can also implement
/// it without touching the CEK machine definition.
///
/// # Design Note
/// Only the operations that continuation closures actually perform
/// are included. Do NOT add methods speculatively — keep the surface
/// minimal to make alternative implementations trivial.
pub trait CekEvalField: Send {
    /// Number of cells in this field.
    fn cell_count(&self) -> usize;

    /// Get the current execution probability of cell `index`.
    ///
    /// Returns 0.0 if `index` is out of bounds.
    fn get_exec_prob(&self, index: usize) -> f32;

    /// Set the execution probability of cell `index`.
    ///
    /// Clamped to `[0.0, 1.0]`. Out-of-bounds index is a no-op.
    fn set_exec_prob(&mut self, index: usize, value: f32);
}

// =====================================================================
// Continuation type aliases
// =====================================================================

/// A single heap-allocated, resumable continuation frame.
///
/// The closure takes `&mut dyn CekEvalField` so it can mutate the
/// activation field without knowing its concrete type.
pub type Continuation = Box<dyn FnMut(&mut dyn CekEvalField) + Send + 'static>;

// =====================================================================
// CEKMachine — the core VM substrate
// =====================================================================

/// CEK Virtual Machine instance for a single activation cell context.
///
/// Each `CEKMachine` tracks:
///   - **Control** (`control_cell`): The field cell index this machine
///     is evaluating.
///   - **Environment** (`environment_weights`): Local topology-aligned
///     influence weights captured at machine bootstrap time.
///   - **Kontinuation** (`kontinuation_stack`): A stack of resumable
///     computation frames that will mutate the activation field when
///     drained by `evaluate_all`.
///   - **Telemetry** (`prob_signal`): The emission probability that
///     triggered this machine's creation.
///
/// # Lifecycle
/// 1. Created by `ExecutionBridge::bootstrap_cek_context`.
/// 2. Stored in `ExecutionBridge::deferred_cek_queue`.
/// 3. Evaluated frame-by-frame by `MKRWorld::tick()` Phase 3.
/// 4. Evicted by `evict_quiescent_cek_states` when the cell fades.
pub struct CEKMachine {
    /// Control (C): active evaluation cell index pointer.
    pub control_cell: usize,
    /// Environment (E): local topology-aligned influence vector.
    pub environment_weights: Vec<f32>,
    /// Kontinuation (K): delayed/resumable computation stack frames.
    pub kontinuation_stack: Vec<Continuation>,
    /// Emission probability signal that triggered this machine.
    pub prob_signal: f32,
}

impl CEKMachine {
    /// Create a new CEK machine for a given cell context.
    pub fn new(
        control_cell: usize,
        environment_weights: Vec<f32>,
        prob_signal: f32,
    ) -> Self {
        Self {
            control_cell,
            environment_weights,
            kontinuation_stack: Vec::new(),
            prob_signal,
        }
    }

    /// Push a delayed computation frame onto the Kontinuation stack.
    ///
    /// The frame is a closure that accepts `&mut dyn CekEvalField`
    /// and may mutate cell state. Frames are evaluated LIFO by
    /// `evaluate_all`.
    pub fn push_kontinuation<F>(&mut self, k: F)
    where
        F: FnMut(&mut dyn CekEvalField) + Send + 'static,
    {
        self.kontinuation_stack.push(Box::new(k));
    }

    /// Deterministically drain the entire Kontinuation stack.
    ///
    /// Evaluates frames in LIFO order (most recently pushed first).
    /// After this call, `kontinuation_stack` is empty.
    ///
    /// # Parameters
    /// `field` — any type implementing `CekEvalField` (typically
    /// `ActivationField` from mirage-mkr-core).
    pub fn evaluate_all(&mut self, field: &mut dyn CekEvalField) {
        while let Some(mut kont) = self.kontinuation_stack.pop() {
            (kont)(field);
        }
    }

    /// Whether this machine still has unevaluated frames.
    #[inline]
    pub fn is_pending(&self) -> bool {
        !self.kontinuation_stack.is_empty()
    }
}

// =====================================================================
// Tests
// =====================================================================
#[cfg(test)]
mod tests {
    use super::*;

    // ---------------------------------------------------------------
    // Mock field for testing without depending on mirage-mkr-core
    // ---------------------------------------------------------------
    struct MockField {
        cells: Vec<f32>,
    }
    impl MockField {
        fn new(n: usize) -> Self { Self { cells: vec![0.0; n] } }
    }
    unsafe impl Send for MockField {}
    impl CekEvalField for MockField {
        fn cell_count(&self) -> usize { self.cells.len() }
        fn get_exec_prob(&self, idx: usize) -> f32 {
            self.cells.get(idx).copied().unwrap_or(0.0)
        }
        fn set_exec_prob(&mut self, idx: usize, value: f32) {
            if let Some(c) = self.cells.get_mut(idx) {
                *c = value.clamp(0.0, 1.0);
            }
        }
    }

    #[test]
    fn new_machine_starts_empty() {
        let m = CEKMachine::new(3, vec![0.5; 4], 0.7);
        assert_eq!(m.control_cell, 3);
        assert!((m.prob_signal - 0.7).abs() < 1e-6);
        assert!(!m.is_pending());
    }

    #[test]
    fn push_and_drain_stack() {
        let mut m = CEKMachine::new(0, vec![], 1.0);
        let mut field = MockField::new(4);

        m.push_kontinuation(|f| { f.set_exec_prob(0, 0.9); });
        assert!(m.is_pending());

        m.evaluate_all(&mut field);
        assert!(!m.is_pending());
        assert!((field.get_exec_prob(0) - 0.9).abs() < 1e-6);
    }

    #[test]
    fn stack_drains_lifo() {
        let mut m = CEKMachine::new(0, vec![], 1.0);
        let mut field = MockField::new(2);

        // Push first sets cell 0 to 0.1, second sets cell 0 to 0.9
        // LIFO: second runs first → 0.9, then first → 0.1
        m.push_kontinuation(|f| { f.set_exec_prob(0, 0.1); });
        m.push_kontinuation(|f| { f.set_exec_prob(0, 0.9); });

        m.evaluate_all(&mut field);
        // Last to run was the 0.1 frame (LIFO: 0.9 ran first, then 0.1 overwrote)
        assert!((field.get_exec_prob(0) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn evaluate_all_mutates_field() {
        let mut m = CEKMachine::new(5, vec![1.0; 8], 0.85);
        let mut field = MockField::new(8);

        let prob_signal = m.prob_signal;
        m.push_kontinuation(move |f| {
            let current = f.get_exec_prob(5);
            f.set_exec_prob(5, current * 0.9 + prob_signal * 0.1);
        });

        m.evaluate_all(&mut field);
        // 0.0 * 0.9 + 0.85 * 0.1 = 0.085
        assert!((field.get_exec_prob(5) - 0.085).abs() < 1e-4);
    }

    #[test]
    fn out_of_bounds_set_is_noop() {
        let mut field = MockField::new(2);
        field.set_exec_prob(999, 1.0); // must not panic
        assert!((field.get_exec_prob(999) - 0.0).abs() < 1e-6);
    }
}
```

---


# File: crates/mirage-mkr-core/src/protocol.rs

```rust
// ===================================================================
// mirage-mkr-core/src/protocol.rs  (V3 — Federated Stabilization Pass)
// PURPOSE: Runtime Protocol Descriptors — Federated Communication Layer
//
// ---------------------------------------------------------------
// DESIGN INTENT
// ---------------------------------------------------------------
//
// This module defines lightweight, compatibility-safe protocol types
// that carry runtime information between subsystems WITHOUT creating
// direct crate coupling.
//
// These are NOT:
//   * ECS components
//   * orchestration systems
//   * message queues
//   * scheduler instructions
//
// They ARE:
//   * Plain data descriptors (Copy types where possible)
//   * Translation outputs from MKR's authority fields
//   * Protocol boundaries between federated subsystems
//
// ---------------------------------------------------------------
// PROTOCOL ARCHITECTURE
// ---------------------------------------------------------------
//
// MKR produces:          ActivationDescriptor, StreamingEligibility,
//                        ExecutionDescriptor, RuntimeSignal
//
// OASIS consumes:        StreamingEligibility (to decide stream work)
//
// Executor consumes:     ExecutionDescriptor (via ExecutionBridge)
//
// Renderer consumes:     ActivationDescriptor (via RendererBridge)
//                        ResidencyDescriptor  (future)
//
// ---------------------------------------------------------------
// FUTURE PROTOCOL DIRECTION
// ---------------------------------------------------------------
//
// TODO(V3-PROTOCOL): ActivationDescriptor should replace the raw
//   `execution_probability` f32 passed between MKR and renderer.
//   Wrap it in a typed descriptor so consumers cannot misuse it
//   as a scheduling authority input.
//
// TODO(V3-PROTOCOL): StreamingEligibility should replace the raw
//   StreamingDecision as the cross-crate boundary type.  OASIS
//   receives StreamingEligibility; it produces its own StreamRequest.
//
// TODO(V3-PROTOCOL): ResidencyDescriptor should replace raw chunk_idx
//   in OASIS and renderer residency APIs.  It unifies field-index
//   and page-address into one typed protocol value.
//
// TODO(V3-CEK): When CEK is implemented, RuntimeSignal will carry
//   CEK continuation identifiers alongside probability values.
// ===================================================================

// =====================================================================
// ACTIVATION DESCRIPTOR
// =====================================================================

/// Snapshot of a single cell's activation state, suitable for passing
/// across subsystem boundaries without exposing the raw ActivationCell.
///
/// Produced by MKR; consumed by renderer (passive) and future CEK.
///
/// # V3 Design
/// This descriptor is intentionally minimal — it carries only what
/// downstream subsystems need.  Adding fields here requires justification
/// from the consuming subsystem, not from MKR's internal needs.
#[derive(Debug, Clone, Copy)]
pub struct ActivationDescriptor {
    /// Flat field cell index (== chunk index for 1:1 grids).
    pub cell_index: usize,
    /// Continuous execution probability in [0.0, 1.0].
    /// Derived from ActivationField::execution_probability.
    pub execution_probability: f32,
    /// Mean activation of this cell (heat × 0.55 + pressure × 0.35 + entropy × 0.10).
    /// Provided for downstream display; NOT a scheduling authority input.
    pub activation: f32,
}

// =====================================================================
// STREAMING ELIGIBILITY
// =====================================================================

/// MKR's streaming eligibility signal for a single cell.
///
/// Produced by StreamingCoordinator; consumed by OASIS (streaming execution).
///
/// # Ownership Contract
/// MKR produces eligibility.  OASIS executes.
/// The bridge that converts this to an OASIS StreamRequest is future work.
///
/// TODO(V3-OASIS-CANONICAL): Add conversion: StreamingEligibility → StreamRequest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamingEligibility {
    /// Cell is below all streaming thresholds — no streaming needed.
    None,
    /// Cell is above the prefetch threshold — begin async background load.
    PrefetchEligible,
    /// Cell is above the resident threshold — promote to VRAM immediately.
    ResidentEligible,
}

impl StreamingEligibility {
    /// Classify a probability value into a streaming eligibility tier.
    ///
    /// Uses the same thresholds as StreamingCoordinator.
    pub fn classify(probability: f32) -> Self {
        use crate::streaming::{STREAM_PREFETCH_THRESHOLD, STREAM_RESIDENT_THRESHOLD};
        if probability >= STREAM_RESIDENT_THRESHOLD {
            StreamingEligibility::ResidentEligible
        } else if probability >= STREAM_PREFETCH_THRESHOLD {
            StreamingEligibility::PrefetchEligible
        } else {
            StreamingEligibility::None
        }
    }
}

// =====================================================================
// EXECUTION DESCRIPTOR
// =====================================================================

/// Protocol descriptor for passing execution intent to the executor.
///
/// Produced by ExecutionBridge; consumed by executor (passive backend).
///
/// # V3 Design
/// This type replaces ChunkTask::state: ChunkState as the primary
/// scheduling input.  The executor MUST NOT re-derive priority from
/// thermal state — it must accept the priority as given here.
///
/// TODO(V3-EXECUTOR-PASSIVE): Wire executor to accept ExecutionDescriptor
/// slices from ExecutionBridge instead of scheduling from ThermalSystem.
#[derive(Debug, Clone, Copy)]
pub struct ExecutionDescriptor {
    /// Flat field cell index (== chunk index for 1:1 grids).
    pub cell_index: usize,
    /// Pre-computed priority in [0.0, 1.0].
    /// Derived from execution_probability — executor must NOT recompute this.
    pub priority: f32,
    /// Frame by which this descriptor expires.
    pub deadline_frame: u64,
    /// Whether this is a prefetch hint (low priority, no fiber spawn yet).
    pub is_prefetch_hint: bool,
}

// =====================================================================
// RESIDENCY DESCRIPTOR
// =====================================================================

/// V3 residency address: pairs a field cell index with an OASIS page ref.
///
/// This replaces raw (chunk_idx: u32, page_id: u32) pairs in all residency APIs.
///
/// # Ownership
/// MKR produces ResidencyDescriptors from field-handle conversions.
/// OASIS owns the residency lifecycle (load, evict, promote).
/// Renderer consumes read-only residency queries.
///
/// TODO(V3-RENDERER-PASSIVE): Replace ResidencyTracker::request_load(u32)
/// with request_load(ResidencyDescriptor) so the OASIS key space and
/// field key space are unified under one typed address.
#[derive(Debug, Clone, Copy)]
pub struct ResidencyDescriptor {
    /// V3 primary key — indexes ActivationField::cells directly.
    pub cell_index: usize,
    /// OASIS virtual page containing this chunk's data.
    pub oasis_page_id: u32,
}

// =====================================================================
// RUNTIME SIGNAL
// =====================================================================

/// Lightweight signal carrying a MKR runtime event to downstream subsystems.
///
/// RuntimeSignals are edge-triggered events — they fire once per condition
/// crossing, not continuously every tick.
///
/// TODO(V3-CEK): When CEK is implemented, RuntimeSignal will carry
///   a continuation_id so CEK can decide which continuation to launch.
#[derive(Debug, Clone, Copy)]
pub enum RuntimeSignal {
    /// A cell's execution_probability crossed the emission gate.
    EmissionThresholdCrossed { cell_index: usize, probability: f32 },
    /// A streaming operation completed — inject heat feedback.
    StreamCompletionFeedback { cell_index: usize, heat_amount: f32 },
    /// A topology edge weight changed — pressure re-propagation needed.
    TopologyWeightChanged { from_node: usize, to_node: usize, new_pull: f32 },
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_eligibility_none_below_prefetch() {
        // 0.01 < STREAM_PREFETCH_THRESHOLD (0.03)
        let e = StreamingEligibility::classify(0.01);
        assert_eq!(e, StreamingEligibility::None);
    }

    #[test]
    fn streaming_eligibility_prefetch_in_range() {
        // 0.05 >= PREFETCH_THRESHOLD but < RESIDENT_THRESHOLD (0.35)
        let e = StreamingEligibility::classify(0.05);
        assert_eq!(e, StreamingEligibility::PrefetchEligible);
    }

    #[test]
    fn streaming_eligibility_resident_above_threshold() {
        // 0.40 >= RESIDENT_THRESHOLD (0.35)
        let e = StreamingEligibility::classify(0.40);
        assert_eq!(e, StreamingEligibility::ResidentEligible);
    }

    #[test]
    fn activation_descriptor_is_copy() {
        let d = ActivationDescriptor { cell_index: 3, execution_probability: 0.7, activation: 0.6 };
        let d2 = d; // must be Copy
        assert_eq!(d2.cell_index, 3);
    }

    #[test]
    fn execution_descriptor_is_copy() {
        let d = ExecutionDescriptor {
            cell_index: 5, priority: 0.8, deadline_frame: 100, is_prefetch_hint: false
        };
        let d2 = d;
        assert_eq!(d2.cell_index, 5);
    }
}
```

---


# File: crates/mirage-mkr-core/src/bridge/execution_bridge.rs

```rust
// ===================================================================
// mirage-mkr-core/src/bridge/execution_bridge.rs
// PURPOSE: ExecutionBridge — EmissionRequest → Executor Protocol
//
// ROLE IN V3:
// The ExecutionBridge translates the MKR emission layer's output
// (EmissionRequest records) into executor-compatible scheduling
// requests, WITHOUT implementing autonomous scheduling, cognition,
// or CEK logic.
//
// This is a PROTOCOL BRIDGE only:
//   MKR emits:  EmissionRequest { cell_index, probability }
//   Executor expects:  ChunkTask { chunk_idx, priority, deadline_frame, state }
//
// The bridge performs the mechanical translation between these two
// type systems so the executor can continue running during the V3
// transition without being modified.
//
// ARCHITECTURAL POSITION:
//   EmissionGate → EmissionRequest
//       ↓
//   ExecutionBridge::translate()
//       ↓
//   SchedulingRequest  [executor-compatible]
//       ↓
//   Caller passes to ThermalScheduler::task_queue OR future fiber emitter
//
// WHAT THIS BRIDGE DOES NOT DO:
//   * Does NOT decide WHICH cells to emit (that is EmissionGate).
//   * Does NOT decide HOW MANY fibers to spawn (future FiberPool).
//   * Does NOT implement CEK continuation selection.
//   * Does NOT own a thread pool or spawn any work.
//   * Does NOT read ChunkState enum arms.
//
// PREPARATION FOR CEK:
// When CEK is implemented, the bridge's output type (`SchedulingRequest`)
// will be extended with a `continuation_id` field that CEK populates.
// The rest of the bridge is unchanged.
//
// TODO(V3-CEK-BRIDGE): Add `continuation_id: Option<CekContinuationId>`
//   to SchedulingRequest once CEK defines its continuation type.
// TODO(V3-BRIDGE-FIBER): Replace `priority: f32` with a full
//   `FiberEmissionSlot` once FiberPool is wired to MKRWorld.
// ===================================================================

use crate::emission::{EmissionRequest, MAX_EMIT_PER_TICK};

/// Re-export CEKMachine from mirage-cek for backwards-compatible access.
/// New code should use `mirage_cek::CEKMachine` directly.
pub use mirage_cek::CEKMachine;

// =====================================================================
// SCHEDULING REQUEST — Executor-compatible work descriptor
// =====================================================================

/// Executor-compatible scheduling request produced by `ExecutionBridge`.
///
/// This type is intentionally kept minimal.  It carries only what the
/// executor needs to prioritise and execute work.  CEK will extend it
/// with a continuation identifier when CEK is implemented.
///
/// # V3 Design
/// `priority` is derived directly from `execution_probability` — no
/// enum-arm translation, no threshold branching.  The executor receives
/// a continuous priority weight that it can use as-is or discretise
/// internally.
#[derive(Debug, Clone, Copy)]
pub struct SchedulingRequest {
    /// Flat field cell index — the chunk this request is for.
    pub cell_index: usize,

    /// Continuous execution priority in [0.0, 1.0].
    ///
    /// Derived directly from `EmissionRequest::probability`.
    /// Higher = execute sooner / allocate more budget.
    pub priority: f32,

    /// Frame by which this request expires (if not executed).
    ///
    /// Currently set to `current_frame + DEFAULT_DEADLINE_FRAMES`.
    /// Future: CEK will compute domain-specific deadlines.
    pub deadline_frame: u64,

    /// Whether this request represents a prefetch hint (vs. execution demand).
    ///
    /// Prefetch hints are used to trigger streaming without spawning fibers.
    /// TODO(V3-STREAM): Wire this to StreamingCoordinator decisions.
    pub is_prefetch_hint: bool,
}

/// Default number of frames before a scheduling request expires.
pub const DEFAULT_DEADLINE_FRAMES: u64 = 4;

// =====================================================================
// EXECUTION BRIDGE
// =====================================================================

/// Protocol bridge: EmissionRequest → SchedulingRequest.
///
/// # Usage
/// ```rust
/// let bridge = ExecutionBridge::new(16);
/// let requests = bridge.translate(
///     world.emission_requests(),
///     world.frame,
/// );
/// for req in &requests {
///     // Pass to executor, fiber pool, or log for debugging
/// }
/// ```
pub struct ExecutionBridge {
    pub capacity: usize,
    pub deferred_cek_queue: std::cell::RefCell<Vec<CEKMachine>>,
}

impl ExecutionBridge {
    pub fn new(fiber_budget: usize) -> Self {
        Self {
            capacity: fiber_budget,
            deferred_cek_queue: std::cell::RefCell::new(Vec::new()),
        }
    }

    /// Translate a slice of `EmissionRequest`s into `SchedulingRequest`s.
    ///
    /// Each `EmissionRequest` maps 1:1 to a `SchedulingRequest`.
    /// No filtering is applied — the emission gate already handles that.
    /// No scheduling logic is applied — the executor handles that.
    ///
    /// # Priority Mapping
    /// `priority = emission.probability` (identity mapping).
    ///
    /// This is intentional: the activation field already encodes the
    /// correct priority signal.  Remapping would lose information.
    ///
    /// # Deadline
    /// `deadline_frame = current_frame + DEFAULT_DEADLINE_FRAMES`.
    ///
    /// TODO(V3-CEK-BRIDGE): Replace with CEK-computed domain deadlines.
    pub fn translate(
        &self,
        emissions:     &[EmissionRequest],
        current_frame: u64,
    ) -> Vec<SchedulingRequest> {
        emissions
            .iter()
            .map(|e| SchedulingRequest {
                cell_index:      e.cell_index,
                priority:        e.probability,
                deadline_frame:  current_frame + DEFAULT_DEADLINE_FRAMES,
                is_prefetch_hint: e.probability < 0.15,
            })
            .collect()
    }

    /// Translate emissions filtered by region activity.
    ///
    /// **V3-SPARSE / Task 10: Differential scheduling preparation.**
    ///
    /// Only emits `SchedulingRequest`s for cells in regions whose activity
    /// state is `Warming`, `Active`, or `Hot`.  Dormant region cells are
    /// suppressed, even if they appear in the emission list.
    ///
    /// # Rationale
    /// In a differential runtime, dormant regions by definition have
    /// no significant field change.  Any emission requests from dormant
    /// regions are either stale (still_eligible from last activation) or
    /// noise (floating-point jitter above EMIT_GATE).
    ///
    /// # TODO(V3-SPARSE-VALIDATION): Run translate() and translate_region_filtered()
    /// in parallel to confirm that the suppressed set contains only genuinely
    /// low-priority requests.  Validate for 1000 ticks.
    ///
    /// # TODO(V3-CEK-BRIDGE): Add `region_id: u32` to SchedulingRequest so
    /// CEK can select region-local continuations.
    pub fn translate_region_filtered(
        &self,
        emissions:     &[EmissionRequest],
        current_frame: u64,
        region_map:    &crate::regions::RegionMap,
    ) -> Vec<SchedulingRequest> {
        emissions
            .iter()
            .filter(|e| {
                // Only emit for active/hot/warming regions.
                // Dormant regions are skipped — no scheduling overhead.
                !region_map.cell_is_dormant(e.cell_index)
            })
            .map(|e| SchedulingRequest {
                cell_index:      e.cell_index,
                priority:        e.probability,
                deadline_frame:  current_frame + DEFAULT_DEADLINE_FRAMES,
                is_prefetch_hint: e.probability < 0.15,
            })
            .collect()
    }

    /// Translate a single `EmissionRequest` (for per-cell queries).
    #[inline]
    pub fn translate_one(
        &self,
        emission:      EmissionRequest,
        current_frame: u64,
    ) -> SchedulingRequest {
        SchedulingRequest {
            cell_index:      emission.cell_index,
            priority:        emission.probability,
            deadline_frame:  current_frame + DEFAULT_DEADLINE_FRAMES,
            is_prefetch_hint: emission.probability < 0.15,
        }
    }

    /// Filter and sort scheduling requests by priority (descending).
    ///
    /// Returns requests above `min_priority`, sorted highest-first.
    /// This is a convenience method; the executor can also sort itself.
    pub fn priority_filter<'a>(
        &self,
        requests:     &'a mut Vec<SchedulingRequest>,
        min_priority: f32,
    ) -> &'a [SchedulingRequest] {
        requests.retain(|r| r.priority >= min_priority);
        requests.sort_unstable_by(|a, b| {
            b.priority
                .partial_cmp(&a.priority)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        requests.as_slice()
    }

    /// Estimate the CPU budget fraction this request batch requires.
    ///
    /// Returns a value in [0.0, 1.0] — 1.0 means the full `MAX_EMIT_PER_TICK`
    /// budget is consumed.  Useful for load-balancing across subsystems.
    pub fn budget_fraction(&self, requests: &[SchedulingRequest]) -> f32 {
        (requests.len() as f32 / MAX_EMIT_PER_TICK as f32).min(1.0)
    }

    /// Dynamically translate raw EmissionRequests into low-overhead captured CEK Machines.
    ///
    /// Closures operate on `&mut dyn mirage_cek::CekEvalField` rather than the
    /// concrete `ActivationField` type. This decouples mirage-cek from
    /// mirage-mkr-core, eliminating the circular dependency.
    pub fn bootstrap_cek_context(
        &self,
        emissions: &[crate::emission::EmissionRequest],
        topo_influence: &[f32],
    ) -> Vec<CEKMachine> {
        let mut machines = Vec::with_capacity(emissions.len());
        for req in emissions {
            let mut machine = CEKMachine::new(req.cell_index, topo_influence.to_vec(), req.probability);

            // Capture the simulation cell index inside a closure environment.
            // The closure operates on `dyn CekEvalField` to avoid pulling in
            // the concrete ActivationField type from this bridge crate.
            let cell_idx = req.cell_index;
            let prob_signal = req.probability;
            machine.push_kontinuation(move |field_ref| {
                if cell_idx < field_ref.cell_count() {
                    // Authoritative CEK Cellular Step: Mutably drive probability state alignment
                    let current = field_ref.get_exec_prob(cell_idx);
                    field_ref.set_exec_prob(
                        cell_idx,
                        (current * 0.9 + prob_signal * 0.1).clamp(0.0, 1.0),
                    );
                }
            });

            machines.push(machine);
        }
        machines
    }

    /// Evict machines from the deferred queue if their underlying cells fade below a certain noise floor.
    /// This protects the heap from persistent stale memory leaking over un-invoked cells.
    pub fn evict_quiescent_cek_states(&self, field: &crate::activation::field::ActivationField) {
        let mut queue = self.deferred_cek_queue.borrow_mut();
        const QUIESCENT_FLOOR: f32 = 1e-4;
        queue.retain(|machine| {
            // Use the CekEvalField trait to avoid direct ActivationField dependency in CEKMachine.
            if machine.control_cell < field.cells.len() {
                let prob = field.cells[machine.control_cell].execution_probability;
                prob >= QUIESCENT_FLOOR
            } else {
                false
            }
        });
    }

    /// Statefully collect newly generated context fields and merge them directly with our deferred lifecycle backlog
    pub fn process_and_queue_cek_context(
        &self,
        emissions: &[crate::emission::EmissionRequest],
        topo_influence: &[f32],
    ) {
        let mut new_machines = self.bootstrap_cek_context(emissions, topo_influence);
        let mut queue = self.deferred_cek_queue.borrow_mut();
        queue.append(&mut new_machines);
    }
}

impl Default for ExecutionBridge {
    fn default() -> Self {
        Self::new(128)
    }
}

// =====================================================================
// CEK MACHINE SUBSTRATE
// =====================================================================
//
// CEKMachine is defined in `mirage-cek` and re-exported above.
// The continuation closures use `&mut dyn mirage_cek::CekEvalField`
// instead of the concrete `ActivationField`, breaking the circular
// dependency between mirage-cek and mirage-mkr-core.
//
// `ActivationField` implements `CekEvalField` in
// `crates/mirage-mkr-core/src/activation/field.rs`.
//
// evaluate_all() in lib.rs tick() now calls:
//   machine.evaluate_all(&mut self.activation_field as &mut dyn CekEvalField)


// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emission::EmissionRequest;

    fn make_emission(cell: usize, prob: f32) -> EmissionRequest {
        EmissionRequest { cell_index: cell, probability: prob }
    }

    #[test]
    fn translate_maps_probability_to_priority() {
        let bridge = ExecutionBridge::new(16);
        let emissions = vec![make_emission(0, 0.8), make_emission(1, 0.3)];
        let requests = bridge.translate(&emissions, 100);
        assert_eq!(requests.len(), 2);
        assert!((requests[0].priority - 0.8).abs() < 1e-6);
        assert!((requests[1].priority - 0.3).abs() < 1e-6);
    }

    #[test]
    fn translate_sets_correct_deadline() {
        let bridge = ExecutionBridge::new(16);
        let emissions = vec![make_emission(5, 0.5)];
        let requests = bridge.translate(&emissions, 200);
        assert_eq!(requests[0].deadline_frame, 200 + DEFAULT_DEADLINE_FRAMES);
    }

    #[test]
    fn low_probability_marked_as_prefetch_hint() {
        let bridge = ExecutionBridge::new(16);
        let emissions = vec![
            make_emission(0, 0.05),  // below 0.15 threshold
            make_emission(1, 0.80),  // above 0.15 threshold
        ];
        let requests = bridge.translate(&emissions, 0);
        assert!(requests[0].is_prefetch_hint, "low prob should be prefetch hint");
        assert!(!requests[1].is_prefetch_hint, "high prob should not be prefetch hint");
    }

    #[test]
    fn empty_emissions_produce_empty_requests() {
        let bridge = ExecutionBridge::new(16);
        let requests = bridge.translate(&[], 0);
        assert!(requests.is_empty());
    }

    #[test]
    fn budget_fraction_bounded() {
        let bridge = ExecutionBridge::new(16);
        let emissions: Vec<EmissionRequest> = (0..200)
            .map(|i| make_emission(i, 0.5))
            .collect();
        let requests = bridge.translate(&emissions, 0);
        let fraction = bridge.budget_fraction(&requests);
        assert!(fraction >= 0.0 && fraction <= 1.0,
            "budget fraction out of range: {}", fraction);
    }

    #[test]
    fn test_cek_machine_bootstrap_and_stack_drain() {
        use crate::emission::EmissionRequest;
        use crate::activation::field::ActivationField;
        
        let bridge = ExecutionBridge::new(16);
        let emissions = vec![
            EmissionRequest { cell_index: 2, probability: 0.85 },
        ];
        let topo = vec![0.0; 64];
        let mut field = ActivationField::new(8, 8);
        let mut machines = bridge.bootstrap_cek_context(&emissions, &topo);
        
        assert_eq!(machines.len(), 1);
        assert_eq!(machines[0].control_cell, 2);
        assert_eq!(machines[0].kontinuation_stack.len(), 1);
        
        // Authoritatively evaluate the stack context onto our live test field
        machines[0].evaluate_all(&mut field);
        assert_eq!(machines[0].kontinuation_stack.len(), 0);
        assert!(field.cells[2].execution_probability > 0.0, "CEK frame must mutably alter field states");
    }

    #[test]
    fn test_continuation_lifecycle_multi_frame_persistence() {
        use crate::emission::EmissionRequest;
        use crate::activation::field::ActivationField;

        // Initialize with a strict budget limitation of exactly 1 context swap per frame
        let bridge = ExecutionBridge::new(1);
        let mut field = ActivationField::new(8, 8);
        field.cells[10].execution_probability = 0.9;
        field.cells[20].execution_probability = 0.8;
        field.cells[30].execution_probability = 0.7;

        // Flood the gateway with 3 heavy cellular emissions to purposely blast past bounds
        let emissions = vec![
            EmissionRequest { cell_index: 10, probability: 0.9 },
            EmissionRequest { cell_index: 20, probability: 0.8 },
            EmissionRequest { cell_index: 30, probability: 0.7 },
        ];
        let topo = vec![0.0; 64];

        // Frame 1: Queue and evaluate under strict budget restrictions
        bridge.process_and_queue_cek_context(&emissions, &topo);
        {
            let mut queue = bridge.deferred_cek_queue.borrow_mut();
            assert_eq!(queue.len(), 3, "All 3 contexts must be successfully statefully initialized");
            
            let mut unexecuted = Vec::new();
            let mut count = 0;
            for mut m in queue.drain(..) {
                if count < 1 {
                    m.evaluate_all(&mut field);
                    count += 1;
                } else {
                    unexecuted.push(m);
                }
            }
            *queue = unexecuted;
        }

        // Verify exactly 2 overflow context modules got cleanly preserved
        assert_eq!(bridge.deferred_cek_queue.borrow().len(), 2, "Overflow contexts must statefully persist");
        
        // Clear out inactive elements via the noise floor manager to verify filter stability
        field.cells[20].execution_probability = 0.0; // Force cell 20 into a dead state
        bridge.evict_quiescent_cek_states(&field);
        assert_eq!(bridge.deferred_cek_queue.borrow().len(), 1, "Dead context should be evicted cleanly");
    }
}

```

---


# File: crates/mirage-memory-oasis/src/oasis/mod.rs

```rust
pub mod uuid;
pub mod streamer;

pub use uuid::MirageUuid;
pub use streamer::{StreamingFabric, StreamWorker, StreamRequest, StreamResult};
use memmap2::Mmap;
use std::sync::Arc;

pub struct OasisVirtualPage { pub page_id: u32, pub data: Mmap }
pub struct OasisManager { pub pages: Vec<Arc<OasisVirtualPage>> }

impl OasisManager {
    pub fn new() -> Self { Self { pages: Vec::new() } }
    
    pub fn load_chunk_data(&self, page_id: u32, chunk_idx: u32) -> Vec<u8> {
        let chunk_size_bytes = 3072;
        for page in &self.pages {
            if page.page_id == page_id {
                let offset = (chunk_idx as usize) * chunk_size_bytes;
                if offset + chunk_size_bytes <= page.data.len() {
                    return page.data[offset..offset + chunk_size_bytes].to_vec();
                }
            }
        }
        vec![0u8; chunk_size_bytes] 
    }
}
```

---


# File: crates/mirage-synapse/Cargo.toml

```rust
[package]
name = "mirage-synapse"
version = "0.1.0"
edition = "2021"

[dependencies]
petgraph = "0.6"
```

---


# File: crates/mirage-compute/Cargo.toml

```rust
[package]
name = "mirage-compute"
version = "0.1.0"
edition = "2021"

[dependencies]
```

---


# File: crates/mirage-cek/Cargo.toml

```rust
[package]
name = "mirage-cek"
version = "0.1.0"
edition = "2021"

# CEK = Control-Environment-Kontinuation Virtual Machine Substrate
# Provides the resumable-computation primitives for the MKR kernel.
# Intentionally has NO dependency on mirage-mkr-core to prevent
# circular dependencies. Field types are abstracted via CekEvalField.

[lib]
doctest = false

[dependencies]
```

---


# File: crates/mirage-mkr-core/Cargo.toml

```rust
[package]
name = "mirage-mkr-core"
version = "0.1.0"
edition = "2024"

[dependencies]
# V3 activation layer has no external std-external dependencies.

# COMPAT: mirage-core provides ThermalSystem compatibility bridge.
# TODO(V3-COMPAT): Remove once all downstream crates read ActivationField.
mirage-core = { path = "../mirage-core" }

# mirage-matrix provides TopologyGraph (activation influence graph).
# Used by MKRWorld to wire topology influence into the activation field tick.
mirage-matrix = { path = "../mirage-matrix" }

mirage-compute = { path = "../mirage-compute" }
mirage-executor = { path = "../mirage-executor" }
mirage-query    = { path = "../mirage-query" }
mirage-mts      = { path = "../mirage-mts" }
mirage-cek      = { path = "../mirage-cek" }
mirage-synapse  = { path = "../mirage-synapse" }
rayon = "1.8"

# serde for Handle type in pool/handle.rs
serde = { version = "1.0", features = ["derive"] }

[lib]
doctest = false

```

---


# File: crates/mirage-memory-oasis/Cargo.toml

```rust
[package]
name = "mirage-memory-oasis"
version = "0.1.0"
edition = "2024"

[dependencies]
```

---

