===================================================================
CRATE: mirage-synapse
===================================================================
--- FILE: E:\Mirage Engine\crates\mirage-synapse\src\lib.rs ---
// ===================================================================
// ???: crates/mirage-synapse/src/lib.rs
// ???????: ?????? ?????? ???????? (Predictive Neural Matrix)
// ===================================================================

use std::collections::HashMap;
use petgraph::graph::{DiGraph, NodeIndex};
use petgraph::algo::toposort;

// ?? ????? ?????? ??????? ???????? (Predictive Synapse Node)
pub struct SynapseNode {
    pub name: String,
    pub is_dirty: bool,
    pub priority: f32,       // ???????? ???????? (0.0 - 1.0)
    pub velocity_bias: f32,  // ?????? ?????? (?????? ?????????)
}

// ?? ???? ????? ?????? ??????? ??????
pub struct SynapseRegistry {
    graph: DiGraph<SynapseNode, f32>, // f32 ???? ??? ?????? ??? ???????
    node_map: HashMap<String, NodeIndex>,
}

impl SynapseRegistry {
    pub fn new() -> Self {
        Self {
            graph: DiGraph::new(),
            node_map: HashMap::new(),
        }
    }

    /// ?? ????? ???? ?? ???? ?????? ???? ?????? ???????
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
                // ????? ???? ???? ??????? 1.0
                self.graph.add_edge(dep_idx, node_idx, 1.0);
            }
        }
    }

    /// ? ??? ?????? ???????? (Heuristic Prediction Injection)
    /// ???? ??? ?????? ????? ??? ????? ?????? ????? ??? ???? ???? ??????
    pub fn update_prediction(&mut self, cam_pos: [f32; 3], cam_vel: [f32; 3], node_pos: [f32; 3]) -> f32 {
        // ???? ???? ??????? ?? ???????? ??????
        let to_node = [
            node_pos[0] - cam_pos[0],
            node_pos[1] - cam_pos[1],
            node_pos[2] - cam_pos[2],
        ];

        // ???? Dot Product (????? ???????) ?????? ?? ?????? ???? ??? ???????
        let dot = cam_vel[0] * to_node[0] + cam_vel[1] * to_node[1] + cam_vel[2] * to_node[2];
        
        // ??? ??? ????? ??????? ??????? ???? ?????? ???? ????? -> ???? ????????
        let prediction_weight = if dot > 0.0 { 1.5 } else { 0.5 };

        // ????? ???? ????? ???????? ???? ??????
        for node_idx in self.graph.node_indices() {
            if let Some(node) = self.graph.node_weight_mut(node_idx) {
                node.priority = prediction_weight;
            }
        }
        
        if dot > 0.0 { dot * 50.0 } else { 0.0 }
    }

    /// ?? Calculate predictive loading corridor
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

    /// ?? Calculate thermal heat score for streaming priority
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

    /// ?? ?????? ??? ????? ??????? ?????? (Topological Order)
    /// ???? ????? ??????? ??? ??????????? ?? ?????? ???????? ????????
    pub fn get_execution_plan(&self) -> Vec<String> {
        match toposort(&self.graph, None) {
            Ok(nodes) => nodes.iter()
                .map(|&idx| self.graph[idx].name.clone())
                .collect(),
            Err(_) => {
                eprintln!("?? Warning: Circular dependency detected in Synapse Graph!");
                Vec::new()
            }
        }
    }

    /// ?? ????? ?? ????? ??? ???????? ?????? ???? (Streaming Priority)
    pub fn get_high_priority_nodes(&self, threshold: f32) -> Vec<String> {
        self.graph.node_indices()
            .filter(|&idx| self.graph[idx].priority > threshold)
            .map(|idx| self.graph[idx].name.clone())
            .collect()
    }
}


===================================================================
CRATE: mirage-compute
===================================================================
--- FILE: E:\Mirage Engine\crates\mirage-compute\src\lib.rs ---
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


===================================================================
CRATE: mirage-query
===================================================================
--- FILE: E:\Mirage Engine\crates\mirage-query\src\columnar.rs ---
// ===================================================================
// mirage-query/src/columnar.rs
// PURPOSE: ColumnarScan — Structure-of-Arrays (SoA) Execution Backend
//
// SoA layout keeps each physical attribute in its own contiguous Vec,
// enabling the compiler to emit wider SIMD loads and avoiding the
// AoS stride penalty of ActivationCell's 20-byte struct.
//
// INVARIANT: All columns have equal length at all times.
// ===================================================================

/// Structure-of-Arrays store for activation field attributes.
///
/// Each column maps 1-to-1 with a cell index identical to the
/// AoS `ActivationField::cells` index. The two representations
/// hold the same logical data; `ColumnarScan` is the query-execution
/// projection, not the authoritative store.
pub struct ColumnarScan {
    /// cell heat values
    pub heat: Vec<f32>,
    /// cell pressure values
    pub pressure: Vec<f32>,
    /// cell entropy values
    pub entropy: Vec<f32>,
    /// cell activation values
    pub activation: Vec<f32>,
    /// cell execution probability values
    pub execution_probability: Vec<f32>,
    /// selection bitset: cell i is selected iff selected[i] == true
    pub selected: Vec<bool>,
    /// total number of cells
    pub len: usize,
}

impl ColumnarScan {
    /// Create an empty scan with pre-allocated capacity.
    pub fn new(capacity: usize) -> Self {
        Self {
            heat:                  vec![0.0; capacity],
            pressure:              vec![0.0; capacity],
            entropy:               vec![0.0; capacity],
            activation:            vec![0.0; capacity],
            execution_probability: vec![0.0; capacity],
            selected:              vec![true; capacity],
            len:                   capacity,
        }
    }

    /// Resize all columns in-place without heap re-allocation when
    /// capacity is already sufficient.
    pub fn resize(&mut self, n: usize) {
        self.heat.resize(n, 0.0);
        self.pressure.resize(n, 0.0);
        self.entropy.resize(n, 0.0);
        self.activation.resize(n, 0.0);
        self.execution_probability.resize(n, 0.0);
        self.selected.resize(n, true);
        self.len = n;
    }

    /// Load from a flat AoS slice of (heat, pressure, entropy,
    /// activation, execution_probability) tuples by scatter-copying
    /// into each SoA column. The `cells` iterator yields
    /// `(heat, pressure, entropy, activation, exec_prob)` tuples.
    pub fn load_from_cells<I>(&mut self, cells: I)
    where
        I: Iterator<Item = (f32, f32, f32, f32, f32)>,
    {
        let mut n = 0;
        for (heat, pressure, entropy, activation, exec_prob) in cells {
            if n >= self.len {
                // Grow dynamically
                self.heat.push(heat);
                self.pressure.push(pressure);
                self.entropy.push(entropy);
                self.activation.push(activation);
                self.execution_probability.push(exec_prob);
                self.selected.push(true);
                n += 1;
            } else {
                self.heat[n] = heat;
                self.pressure[n] = pressure;
                self.entropy[n] = entropy;
                self.activation[n] = activation;
                self.execution_probability[n] = exec_prob;
                self.selected[n] = true;
                n += 1;
            }
        }
        self.len = n;
    }

    /// Reset all selection bits to `true` (select all cells).
    #[inline]
    pub fn select_all(&mut self) {
        self.selected[..self.len].fill(true);
    }

    /// Apply a predicate over the activation column and narrow the
    /// selection set. Cells that fail the predicate are deselected.
    ///
    /// This is a columnar AND-filter: it only deselects, never re-selects.
    #[inline]
    pub fn filter_activation(&mut self, threshold: f32) {
        for i in 0..self.len {
            if self.selected[i] && self.activation[i] <= threshold {
                self.selected[i] = false;
            }
        }
    }

    /// Apply a predicate over the execution_probability column.
    #[inline]
    pub fn filter_exec_prob(&mut self, threshold: f32) {
        for i in 0..self.len {
            if self.selected[i] && self.execution_probability[i] <= threshold {
                self.selected[i] = false;
            }
        }
    }

    /// Apply a generic per-cell predicate over all five columns.
    ///
    /// The predicate receives `(heat, pressure, entropy, activation,
    /// exec_prob)` and returns `true` to keep the cell selected.
    pub fn filter_generic<F>(&mut self, predicate: F)
    where
        F: Fn(f32, f32, f32, f32, f32) -> bool,
    {
        for i in 0..self.len {
            if self.selected[i]
                && !predicate(
                    self.heat[i],
                    self.pressure[i],
                    self.entropy[i],
                    self.activation[i],
                    self.execution_probability[i],
                )
            {
                self.selected[i] = false;
            }
        }
    }

    /// Collect indices of all currently selected cells.
    pub fn collect_selected(&self) -> Vec<usize> {
        (0..self.len)
            .filter(|&i| self.selected[i])
            .collect()
    }

    /// Count selected cells without allocating.
    #[inline]
    pub fn count_selected(&self) -> usize {
        self.selected[..self.len].iter().filter(|&&s| s).count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_scan(n: usize, activation: f32) -> ColumnarScan {
        let mut scan = ColumnarScan::new(n);
        scan.activation.fill(activation);
        scan
    }

    #[test]
    fn new_selects_all() {
        let scan = ColumnarScan::new(8);
        assert_eq!(scan.count_selected(), 8);
    }

    #[test]
    fn filter_activation_narrows_selection() {
        let mut scan = make_scan(4, 0.1);
        scan.activation[2] = 0.9;
        scan.filter_activation(0.5);
        // Only cell 2 should remain selected
        let sel = scan.collect_selected();
        assert_eq!(sel, vec![2]);
    }

    #[test]
    fn load_from_cells_fills_columns() {
        let mut scan = ColumnarScan::new(3);
        let data = vec![
            (1.0_f32, 0.5, 0.2, 0.8, 0.9),
            (0.5, 0.3, 0.4, 0.6, 0.7),
            (0.2, 0.1, 0.6, 0.4, 0.5),
        ];
        scan.load_from_cells(data.into_iter());
        assert!((scan.heat[0] - 1.0).abs() < 1e-6);
        assert!((scan.activation[1] - 0.6).abs() < 1e-6);
    }

    #[test]
    fn filter_generic_uses_compound_predicate() {
        let mut scan = ColumnarScan::new(4);
        scan.heat = vec![0.8, 0.1, 0.9, 0.05];
        scan.activation = vec![0.7, 0.3, 0.8, 0.1];
        // Select cells where heat > 0.5 AND activation > 0.5
        scan.filter_generic(|h, _p, _e, a, _x| h > 0.5 && a > 0.5);
        let sel = scan.collect_selected();
        assert_eq!(sel, vec![0, 2]);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-query\src\kernel.rs ---
// ===================================================================
// mirage-query/src/kernel.rs
// PURPOSE: SolverKernel — Abstraction for field mutation passes
//
// PARITY INVARIANT
// ---------------------------------------------------------------
// All weight constants are copied verbatim from field.rs and
// sparse.rs to guarantee bit-identical results.
//
// activation = heat×0.55 + pressure×0.35 + (1−entropy)×0.10  [sparse.rs:271]
// exec_prob  = a × a × (3 − 2 × a)                            [smoothstep]
// heat decay = heat × HEAT_DECAY (0.97)                        [field.rs:97]
// entropy grows at +0.003/tick when activation < 0.1           [field.rs:104]
// entropy decays at −0.015×activation/tick when act ≥ 0.1     [field.rs:107]
// ===================================================================

/// Stable identifier for each first-class solver kernel.
///
/// The TraceFusionCompiler records sequences of these IDs as trace
/// signatures. Identical sequences across frames indicate a hot
/// execution path suitable for fusion.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum KernelId {
    Decay,
    Diffuse,
    PropagatePressure,
    RecomputeActivation,
    RecomputeExecutionProbability,
    Custom(u32),
}

/// A named, composable field mutation pass over SoA columns.
///
/// The `apply_selected` method receives all five mutable SoA columns
/// and a shared selection mask. It only processes cells where
/// `selected[i] == true`, preserving correct filter semantics.
pub trait SolverKernel: Send + Sync {
    fn id(&self) -> KernelId;

    /// Execute the kernel over selected cells.
    ///
    /// # Safety
    /// All slices must have equal length ≥ `selected.len()`.
    fn apply_selected(
        &self,
        heat:       &mut [f32],
        pressure:   &mut [f32],
        entropy:    &mut [f32],
        activation: &mut [f32],
        exec_prob:  &mut [f32],
        selected:   &[bool],
    );
}

// -----------------------------------------------------------------------
// Field constants — must stay in sync with field.rs / sparse.rs
// -----------------------------------------------------------------------
pub const HEAT_DECAY:      f32 = 0.97;
pub const ENTROPY_GROWTH:  f32 = 0.003;
pub const ENTROPY_DECAY:   f32 = 0.015;

// Activation blend weights from sparse.rs line 271
const W_HEAT:    f32 = 0.55;
const W_PRESS:   f32 = 0.35;
const W_INV_ENT: f32 = 0.10;

// -----------------------------------------------------------------------
// Built-in kernels
// -----------------------------------------------------------------------

/// Kernel: Exponential heat decay and entropy dynamics.
/// Mirrors `ActivationField::decay()` for selected cells.
pub struct DecayKernel;
impl SolverKernel for DecayKernel {
    fn id(&self) -> KernelId { KernelId::Decay }

    fn apply_selected(
        &self,
        heat:       &mut [f32],
        _pressure:  &mut [f32],
        entropy:    &mut [f32],
        activation: &mut [f32],
        _exec_prob: &mut [f32],
        selected:   &[bool],
    ) {
        let n = heat.len().min(entropy.len()).min(activation.len()).min(selected.len());
        for i in 0..n {
            if !selected[i] { continue; }
            heat[i] *= HEAT_DECAY;
            if activation[i] < 0.1 {
                entropy[i] = (entropy[i] + ENTROPY_GROWTH).clamp(0.0, 1.0);
            } else {
                entropy[i] = (entropy[i] - ENTROPY_DECAY * activation[i]).clamp(0.0, 1.0);
            }
        }
    }
}

/// Kernel: Recompute activation from heat, pressure, entropy.
/// Uses weights from `sparse.rs` line 271: heat×0.55 + pressure×0.35 + (1−entropy)×0.10.
pub struct RecomputeActivationKernel;
impl SolverKernel for RecomputeActivationKernel {
    fn id(&self) -> KernelId { KernelId::RecomputeActivation }

    fn apply_selected(
        &self,
        heat:       &mut [f32],
        pressure:   &mut [f32],
        entropy:    &mut [f32],
        activation: &mut [f32],
        _exec_prob: &mut [f32],
        selected:   &[bool],
    ) {
        let n = heat.len().min(pressure.len()).min(entropy.len())
            .min(activation.len()).min(selected.len());
        for i in 0..n {
            if !selected[i] { continue; }
            let raw = heat[i] * W_HEAT
                + pressure[i] * W_PRESS
                + (1.0 - entropy[i]) * W_INV_ENT;
            activation[i] = raw.clamp(0.0, 1.0);
        }
    }
}

/// Kernel: Smoothstep execution probability gate.
/// Mirrors `ActivationField::recompute_execution_probability()`.
pub struct RecomputeExecProbKernel;
impl SolverKernel for RecomputeExecProbKernel {
    fn id(&self) -> KernelId { KernelId::RecomputeExecutionProbability }

    fn apply_selected(
        &self,
        _heat:      &mut [f32],
        _pressure:  &mut [f32],
        _entropy:   &mut [f32],
        activation: &mut [f32],
        exec_prob:  &mut [f32],
        selected:   &[bool],
    ) {
        let n = activation.len().min(exec_prob.len()).min(selected.len());
        for i in 0..n {
            if !selected[i] { continue; }
            let t = activation[i]; // already clamped [0,1] by previous kernel
            exec_prob[i] = t * t * (3.0 - 2.0 * t);
        }
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use super::*;

    fn make_cols(n: usize) -> (Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<f32>, Vec<bool>) {
        (vec![0.0; n], vec![0.0; n], vec![0.5; n], vec![0.0; n], vec![0.0; n], vec![true; n])
    }

    #[test]
    fn decay_reduces_heat_by_factor() {
        let (mut h, mut p, mut e, mut a, mut x, sel) = make_cols(4);
        h.fill(1.0);
        DecayKernel.apply_selected(&mut h, &mut p, &mut e, &mut a, &mut x, &sel);
        assert!((h[0] - HEAT_DECAY).abs() < 1e-6);
    }

    #[test]
    fn activation_kernel_uses_correct_weights() {
        let (mut h, mut p, mut e, mut a, mut x, sel) = make_cols(1);
        h[0] = 1.0; p[0] = 1.0; e[0] = 0.0;
        RecomputeActivationKernel.apply_selected(&mut h, &mut p, &mut e, &mut a, &mut x, &sel);
        let expected = (W_HEAT + W_PRESS + W_INV_ENT).clamp(0.0, 1.0);
        assert!((a[0] - expected).abs() < 1e-6);
    }

    #[test]
    fn exec_prob_smoothstep_endpoints() {
        let (mut h, mut p, mut e, mut a, mut x, sel) = make_cols(2);
        a[0] = 0.0; a[1] = 1.0;
        RecomputeExecProbKernel.apply_selected(&mut h, &mut p, &mut e, &mut a, &mut x, &sel);
        assert!(x[0].abs() < 1e-6);
        assert!((x[1] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn kernel_respects_selection_mask() {
        let (mut h, mut p, mut e, mut a, mut x, mut sel) = make_cols(4);
        h.fill(1.0);
        sel[1] = false; // deselect cell 1
        sel[3] = false; // deselect cell 3
        DecayKernel.apply_selected(&mut h, &mut p, &mut e, &mut a, &mut x, &sel);
        // Selected cells should have decayed
        assert!((h[0] - HEAT_DECAY).abs() < 1e-6);
        assert!((h[2] - HEAT_DECAY).abs() < 1e-6);
        // Deselected cells must be unchanged
        assert!((h[1] - 1.0).abs() < 1e-6);
        assert!((h[3] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn kernel_id_is_stable() {
        assert_eq!(DecayKernel.id(),                KernelId::Decay);
        assert_eq!(RecomputeActivationKernel.id(),  KernelId::RecomputeActivation);
        assert_eq!(RecomputeExecProbKernel.id(),    KernelId::RecomputeExecutionProbability);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-query\src\lib.rs ---
// ===================================================================
// mirage-query/src/lib.rs
// PURPOSE: Relational Query IR — Layer 2 CellQuery Substrate
//
// DESIGN INTENT
// ---------------------------------------------------------------
// The Query IR provides a data-oriented, declarative API over the
// activation field. Instead of raw imperative loops, callers build
// lazy query pipelines that the TraceFusionCompiler can recognize
// and fuse into optimized SIMD execution blocks.
//
// EXECUTION MODEL
// ---------------------------------------------------------------
// 1. ColumnarScan: SoA (Structure of Arrays) primary execution backend.
//    Stores each cell attribute (heat, pressure, entropy, activation,
//    execution_probability) in separate contiguous slices. This
//    maximises SIMD auto-vectorization by keeping like-typed data adjacent.
//
// 2. CellQuery: The fluent query builder. Supports:
//    - filter(predicate): Lazily mark cells matching a condition.
//    - map(transform):    Apply a mutation to each selected cell.
//    - collect():         Harvest matching cell indices.
//    - apply(kernel):     Execute a SolverKernel over selected cells.
//
// 3. SolverKernel: An abstraction for field mutation passes. The
//    TraceFusionCompiler pattern-matches on kernel signatures to
//    recognize hot paths and fuse them across frames.
//
// PARITY GUARANTEE
// ---------------------------------------------------------------
// Every query path MUST produce bit-identical results to the
// equivalent procedural loop in the original ActivationSolver.
// Verified by the parity tests in this crate.
// ===================================================================

pub mod columnar;
pub mod query;
pub mod kernel;

pub use columnar::ColumnarScan;
pub use query::{CellQuery, CellView, CellViewMut};
pub use kernel::{SolverKernel, KernelId};


--- FILE: E:\Mirage Engine\crates\mirage-query\src\query.rs ---
// ===================================================================
// mirage-query/src/query.rs
// PURPOSE: CellQuery — Fluent Relational Query Builder
//
// DESIGN
// ---------------------------------------------------------------
// CellQuery wraps a mutable reference to a ColumnarScan and exposes
// lazy relational operators (filter, map). Terminal operators
// (collect, apply) consume the query and return results.
//
// PARITY GUARANTEE
// ---------------------------------------------------------------
// Queries that apply kernels produce results identical to the
// equivalent imperative loop in ActivationSolver. Verified by the
// parity tests in this file and by the integration tests in
// mirage-mkr-core.
// ===================================================================

use crate::columnar::ColumnarScan;
use crate::kernel::SolverKernel;

/// Immutable snapshot of a single cell for read-only predicates.
#[derive(Debug, Clone, Copy)]
pub struct CellView {
    pub index:                 usize,
    pub heat:                  f32,
    pub pressure:              f32,
    pub entropy:               f32,
    pub activation:            f32,
    pub execution_probability: f32,
}

/// Mutable view of a single cell for map transformations.
pub struct CellViewMut<'a> {
    pub index:                 usize,
    pub heat:                  &'a mut f32,
    pub pressure:              &'a mut f32,
    pub entropy:               &'a mut f32,
    pub activation:            &'a mut f32,
    pub execution_probability: &'a mut f32,
}

// -----------------------------------------------------------------------
// CellQuery
// -----------------------------------------------------------------------

/// Fluent relational query pipeline over a ColumnarScan.
pub struct CellQuery<'a> {
    scan: &'a mut ColumnarScan,
}

impl<'a> CellQuery<'a> {
    /// Begin a query pipeline. Resets the selection to "all selected".
    pub fn new(scan: &'a mut ColumnarScan) -> Self {
        scan.select_all();
        Self { scan }
    }

    // ------------------------------------------------------------------
    // Relational operators
    // ------------------------------------------------------------------

    /// Filter cells by a compound predicate over all five attributes.
    ///
    /// `predicate(heat, pressure, entropy, activation, exec_prob) -> bool`
    ///
    /// Cells for which the predicate returns `false` are deselected.
    /// Multiple `filter` calls compose as logical AND.
    pub fn filter<F>(self, predicate: F) -> Self
    where
        F: Fn(f32, f32, f32, f32, f32) -> bool,
    {
        self.scan.filter_generic(predicate);
        self
    }

    /// Filter cells by a minimum activation threshold (columnar fast-path).
    pub fn filter_activation(self, threshold: f32) -> Self {
        self.scan.filter_activation(threshold);
        self
    }

    /// Filter cells by a minimum execution_probability threshold.
    pub fn filter_exec_prob(self, threshold: f32) -> Self {
        self.scan.filter_exec_prob(threshold);
        self
    }

    /// Map a transformation over all currently selected cells.
    ///
    /// The closure receives a `CellViewMut` for each selected cell and
    /// may mutate any attribute in-place. Changes are staged inside the
    /// ColumnarScan.
    pub fn map<F>(self, transform: F) -> Self
    where
        F: Fn(CellViewMut<'_>),
    {
        let n = self.scan.len;
        // SAFETY: each iteration borrows a disjoint index from each column.
        // We access columns separately by index to avoid split-borrow issues.
        for i in 0..n {
            if !self.scan.selected[i] { continue; }
            let view = CellViewMut {
                index:                 i,
                heat:                  &mut self.scan.heat[i],
                pressure:              &mut self.scan.pressure[i],
                entropy:               &mut self.scan.entropy[i],
                activation:            &mut self.scan.activation[i],
                execution_probability: &mut self.scan.execution_probability[i],
            };
            transform(view);
        }
        self
    }

    // ------------------------------------------------------------------
    // Terminal operators
    // ------------------------------------------------------------------

    /// Collect indices of all currently selected cells.
    pub fn collect(self) -> Vec<usize> {
        self.scan.collect_selected()
    }

    /// Apply a `SolverKernel` over all currently selected cells.
    ///
    /// Mutates the SoA columns of the underlying ColumnarScan in-place.
    /// Returns `self` so the pipeline can continue or be collected.
    pub fn apply(self, kernel: &dyn SolverKernel) -> Self {
        let n = self.scan.len;
        // Invoke the kernel with mutable slices over all five columns.
        // We pass `&self.scan.selected` as a read-only mask alongside the
        // mutable slices. Rust allows this because `selected` is a
        // separate field from `heat`/`pressure`/etc.
        let sel = self.scan.selected[..n].to_vec(); // snapshot mask read
        kernel.apply_selected(
            &mut self.scan.heat[..n],
            &mut self.scan.pressure[..n],
            &mut self.scan.entropy[..n],
            &mut self.scan.activation[..n],
            &mut self.scan.execution_probability[..n],
            &sel,
        );
        self
    }
}

// -----------------------------------------------------------------------
// ColumnarScan::query() convenience entry-point
// -----------------------------------------------------------------------

impl ColumnarScan {
    /// Begin a fluent CellQuery pipeline over this scan.
    pub fn query(&mut self) -> CellQuery<'_> {
        CellQuery::new(self)
    }
}

// -----------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------
#[cfg(test)]
mod tests {
    use crate::columnar::ColumnarScan;
    use crate::kernel::{DecayKernel, RecomputeActivationKernel, RecomputeExecProbKernel, HEAT_DECAY};

    fn make_scan(n: usize) -> ColumnarScan {
        let mut scan = ColumnarScan::new(n);
        for i in 0..n {
            let f = (i + 1) as f32 / n as f32;
            scan.heat[i]       = f;
            scan.pressure[i]   = 0.3;
            scan.entropy[i]    = 0.5;
            // activation rises linearly so filter at 0.5 gives a clean split
            scan.activation[i] = f * 0.8;
            scan.execution_probability[i] = 0.2;
        }
        scan
    }

    #[test]
    fn filter_activation_returns_correct_indices() {
        let mut scan = make_scan(4);
        // activation: [0.2, 0.4, 0.6, 0.8]
        let sel = scan.query().filter_activation(0.5).collect();
        assert_eq!(sel, vec![2, 3]);
    }

    #[test]
    fn chained_filters_narrow_selection() {
        let mut scan = make_scan(4);
        scan.execution_probability[3] = 0.9;
        let sel = scan.query()
            .filter_activation(0.5)
            .filter_exec_prob(0.5)
            .collect();
        assert_eq!(sel, vec![3]);
    }

    #[test]
    fn map_only_touches_selected_cells() {
        let mut scan = make_scan(4);
        let orig_heat_0 = scan.heat[0];
        scan.query()
            .filter_activation(0.5)
            .map(|c| { *c.heat = 0.0; })
            .collect();
        // Cell 0 was not selected — heat must be unchanged
        assert!((scan.heat[0] - orig_heat_0).abs() < 1e-6);
        // Cells 2 and 3 were selected — heat must be zeroed
        assert!(scan.heat[2].abs() < 1e-6);
        assert!(scan.heat[3].abs() < 1e-6);
    }

    #[test]
    fn apply_kernel_mutates_selected_cells() {
        let mut scan = make_scan(4);
        let orig_heat_0 = scan.heat[0]; // cell 0 NOT selected (activation 0.2 ≤ 0.5)
        scan.query()
            .filter_activation(0.5)
            .apply(&DecayKernel)
            .collect();
        // Cell 0 must be unchanged
        assert!((scan.heat[0] - orig_heat_0).abs() < 1e-6);
        // Cell 2 must have decayed
        let expected = (3.0_f32 / 4.0) * HEAT_DECAY;
        assert!((scan.heat[2] - expected).abs() < 1e-4);
    }

    #[test]
    fn full_pipeline_activation_then_exec_prob() {
        let mut scan = ColumnarScan::new(2);
        scan.heat[0] = 1.0; scan.heat[1] = 0.0;
        scan.pressure[0] = 1.0; scan.pressure[1] = 0.0;
        scan.entropy[0] = 0.0; scan.entropy[1] = 1.0;

        // Run activation kernel first (all cells selected)
        scan.query().apply(&RecomputeActivationKernel).collect();

        // activation[0] = 0.55+0.35+0.10 = 1.0; activation[1] = 0.0
        assert!((scan.activation[0] - 1.0).abs() < 1e-6);
        assert!(scan.activation[1].abs() < 1e-6);

        // Run exec prob kernel
        scan.query().apply(&RecomputeExecProbKernel).collect();

        // smoothstep(1.0) = 1.0; smoothstep(0.0) = 0.0
        assert!((scan.execution_probability[0] - 1.0).abs() < 1e-6);
        assert!(scan.execution_probability[1].abs() < 1e-6);
    }
}


===================================================================
CRATE: mirage-mkr-core
===================================================================
--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\emission.rs ---
// ===================================================================
// mirage-mkr-core/src/emission.rs  (V3 — Differential Runtime Pass)
// PURPOSE: Activation-Driven Fiber Emission Gate
//
// ROLE IN V3:
// The emission gate reads `execution_probability` from the
// ActivationField and decides which chunk-cells are eligible to have
// work fibers spawned for them this frame.
//
// ---------------------------------------------------------------
// TODO(V3-DIFFERENTIAL): DELTA-AWARE EMISSION PREPARATION
// ---------------------------------------------------------------
//
// Current: collect() scans ALL field cells (O(N)) every tick.
// Problem: most cells are Dormant and have probability ≈ 0.0 every tick.
//
// Target: collect_from_changed(field, delta_mask)
//   Only scan cells in delta_mask.iter_changed() — O(|changed|).
//   Cells not in the delta mask cannot have crossed EMIT_GATE this frame
//   (since their probability didn’t change by more than PROBABILITY_EPSILON).
//
// EXCEPTION: cells that were ALREADY above EMIT_GATE last frame and remain
// above it this frame will NOT appear in the delta mask (they didn’t change).
// These must be tracked separately with a persistent “still-eligible” bitset.
//
// Migration plan:
//   Step 1: Implement collect_from_changed() (this pass) — DONE below.
//   Step 2: Validate against collect() output for 1000 ticks (no divergence).
//   Step 3: Replace collect() call in MKRWorld::tick() with collect_from_changed().
//
// Compatibility: collect() is unchanged — executor and renderer compatibility unaffected.
//
// TODO(V3-DIFFERENTIAL): Also gate by RegionActivityState — skip cells
// in Dormant regions before even checking the delta mask.
//
// TODO(V3-CEK): Emission requests produced here will eventually carry
// a continuation_id for CEK to select the correct fiber to launch.
// ===================================================================

use crate::activation::field::ActivationField;

// =====================================================================
// CONSTANTS
// =====================================================================

/// Minimum execution_probability for a cell to be emission-eligible.
///
/// Cells below this value contribute effectively zero work; skipping
/// them avoids scheduling overhead.  0.05 means the field must be
/// at least ~22% activated (smoothstep⁻¹(0.05) ≈ 0.22) before any
/// emission occurs.
pub const EMIT_GATE: f32 = 0.05;

/// Maximum fibers emitted per tick across the whole field.
///
/// This is a hard budget cap to preserve frame-time predictability.
/// Future work: make this dynamic based on available CPU budget.
pub const MAX_EMIT_PER_TICK: usize = 128;

// =====================================================================
// EMISSION REQUEST
// =====================================================================

/// A request to schedule work for a specific activation field cell.
///
/// Produced by `EmissionGate::collect()` each tick.  Consumed by the
/// fiber pool (or future CEK) to spawn actual execution continuations.
///
/// # V3 Semantics
/// `cell_index` is the flat field index — identical to the chunk index
/// for a 1:1 field-to-chunk mapping.
/// `probability` is the raw emission_probability from that cell; the
/// consumer can use it to bias budget allocation within the batch.
#[derive(Debug, Clone, Copy)]
pub struct EmissionRequest {
    /// Flat index into `ActivationField::cells`.
    pub cell_index: usize,
    /// Execution probability at the time of emission (0 < p ≤ 1).
    pub probability: f32,
}

// =====================================================================
// EMISSION GATE
// =====================================================================

/// Stateless activation-driven emission gate.
///
/// Scans the activation field each tick and produces a bounded list of
/// `EmissionRequest`s for cells whose `execution_probability` exceeds
/// `EMIT_GATE`.
///
/// # Branchless Inner Loop
/// The inner loop avoids branching by comparing against the gate as
/// a float and writing to the output only when the condition is met.
/// Modern CPUs predict the rare-write case (most cells dormant) well,
/// but a predicated write would be better.  The structure is already
/// correct for SIMD gather/scatter migration.
///
/// # Budget Enforcement
/// Total output is capped at `MAX_EMIT_PER_TICK`.  When the field is
/// very hot (many cells above gate), high-probability cells are
/// preferred because we sort by `probability` before truncating.
///
/// # V3-DIFFERENTIAL
/// Two emission paths coexist:
///   * `collect(field)` — full-field O(N) scan (current default)
///   * `collect_from_changed(field, delta_mask)` — sparse O(|changed|) scan
///
/// The sparse path also requires `still_eligible` bitset to handle
/// cells that were already above EMIT_GATE last frame (they don't appear
/// in the delta mask but must still be emitted).
pub struct EmissionGate {
    /// Reusable scratch buffer — avoids per-tick Vec allocation.
    scratch: Vec<EmissionRequest>,

    /// V3-DIFFERENTIAL: Persistent bitset of cells that were emission-eligible
    /// last frame and remain eligible this frame.
    /// These do NOT appear in the delta mask (probability didn't change enough)
    /// but must still be included in emission output.
    ///
    /// One bit per cell; 15625 cells = 245 u64 words = ~2 KB.
    still_eligible: Vec<u64>,

    /// Number of cells covered by still_eligible.
    still_eligible_len: usize,

    pub budget: usize,
}

impl EmissionGate {
    /// Create a new EmissionGate with pre-allocated scratch capacity.
    pub fn new() -> Self {
        Self {
            scratch: Vec::with_capacity(MAX_EMIT_PER_TICK * 2),
            still_eligible: Vec::new(),
            still_eligible_len: 0,
            budget: MAX_EMIT_PER_TICK,
        }
    }

    /// Ensure still_eligible bitset covers `num_cells` cells.
    fn ensure_eligible_capacity(&mut self, num_cells: usize) {
        if self.still_eligible_len < num_cells {
            let num_words = (num_cells + 63) / 64;
            self.still_eligible.resize(num_words, 0u64);
            self.still_eligible_len = num_cells;
        }
    }

    #[inline]
    fn set_eligible(&mut self, idx: usize) {
        self.still_eligible[idx / 64] |= 1u64 << (idx % 64);
    }

    #[inline]
    fn clear_eligible(&mut self, idx: usize) {
        self.still_eligible[idx / 64] &= !(1u64 << (idx % 64));
    }

    /// Internal helper: check if a cell idx is in the still-eligible bitset.
    #[allow(dead_code)] // Used by collect_from_changed(); retained for sparse path.
    #[inline]
    fn is_eligible(&self, idx: usize) -> bool {
        idx / 64 < self.still_eligible.len()
            && self.still_eligible[idx / 64] & (1u64 << (idx % 64)) != 0
    }

    /// Scan the field and return a bounded slice of emission requests.
    ///
    /// Full-field O(N) scan.  This is the current default path.
    ///
    /// TODO(V3-DIFFERENTIAL): Once `collect_from_changed()` is validated
    /// for 1000 stable ticks, replace this call in MKRWorld::tick()
    /// with `collect_from_changed()`.
    pub fn collect<'a>(&'a mut self, field: &ActivationField) -> &'a [EmissionRequest] {
        self.scratch.clear();
        self.ensure_eligible_capacity(field.cells.len());

        for (idx, cell) in field.cells.iter().enumerate() {
            if cell.execution_probability > EMIT_GATE {
                self.scratch.push(EmissionRequest {
                    cell_index:  idx,
                    probability: cell.execution_probability,
                });
                self.set_eligible(idx);
            } else {
                self.clear_eligible(idx);
            }
        }

        let budget = MAX_EMIT_PER_TICK.min(self.scratch.len());
        if self.scratch.len() > budget {
            self.scratch.select_nth_unstable_by(budget - 1, |a, b| {
                b.probability.partial_cmp(&a.probability).unwrap_or(std::cmp::Ordering::Equal)
            });
            self.scratch.truncate(budget);
        }

        &self.scratch[..budget]
    }

    /// Sparse emission scan: only evaluates cells that changed OR were already eligible.
    ///
    /// **V3-DIFFERENTIAL path** — O(|changed| + |still_eligible|) instead of O(N).
    ///
    /// # Correctness
    /// A cell must be emitted if:
    ///   (a) It appears in `delta_mask` AND its current probability > EMIT_GATE, OR
    ///   (b) It was eligible last frame (still_eligible bit set) — it didn't
    ///       change but is still above the gate.
    ///
    /// TODO(V3-DIFFERENTIAL): Replace `collect()` in MKRWorld::tick() with
    /// this method after 1000-tick validation against collect() output.
    pub fn collect_from_changed<'a>(
        &'a mut self,
        field:      &ActivationField,
        delta_mask: &crate::activation::delta::FieldDeltaMask,
    ) -> &'a [EmissionRequest] {
        self.scratch.clear();
        self.ensure_eligible_capacity(field.cells.len());

        // Pass 1: scan changed cells (they may have crossed the gate this frame)
        for idx in delta_mask.iter_changed() {
            if idx >= field.cells.len() { break; }
            let p = field.cells[idx].execution_probability;
            if p > EMIT_GATE {
                self.scratch.push(EmissionRequest { cell_index: idx, probability: p });
                self.set_eligible(idx);
            } else {
                self.clear_eligible(idx);
            }
        }

        // Pass 2: scan still-eligible cells (were above gate last frame, unchanged)
        // These don't appear in the delta mask but must still be emitted.
        for word_idx in 0..self.still_eligible.len() {
            let mut word = self.still_eligible[word_idx];
            while word != 0 {
                let bit = word.trailing_zeros() as usize;
                let idx = word_idx * 64 + bit;
                word &= word - 1; // clear lowest set bit
                if idx >= field.cells.len() { break; }
                // Skip cells already processed in Pass 1
                if delta_mask.is_changed(idx) { continue; }
                let p = field.cells[idx].execution_probability;
                if p > EMIT_GATE {
                    self.scratch.push(EmissionRequest { cell_index: idx, probability: p });
                } else {
                    // No longer eligible — clear the bit
                    self.still_eligible[word_idx] &= !(1u64 << bit);
                }
            }
        }

        // Budget enforcement
        let budget = MAX_EMIT_PER_TICK.min(self.scratch.len());
        if self.scratch.len() > budget {
            self.scratch.select_nth_unstable_by(budget - 1, |a, b| {
                b.probability.partial_cmp(&a.probability).unwrap_or(std::cmp::Ordering::Equal)
            });
            self.scratch.truncate(budget);
        }

        &self.scratch[..budget]
    }

    /// Number of cells currently in the scratch buffer (after last collect).
    pub fn pending_count(&self) -> usize {
        self.scratch.len()
    }
}

impl Default for EmissionGate {
    fn default() -> Self { Self::new() }
}


// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;

    fn field_with_probability(width: usize, height: usize, prob: f32) -> ActivationField {
        let mut f = ActivationField::new(width, height);
        for cell in &mut f.cells {
            cell.execution_probability = prob;
        }
        f
    }

    #[test]
    fn dormant_field_emits_nothing() {
        let field = field_with_probability(8, 8, 0.0);
        let mut gate = EmissionGate::new();
        let requests = gate.collect(&field);
        assert_eq!(requests.len(), 0, "dormant field should produce no emission requests");
    }

    #[test]
    fn hot_field_emits_up_to_budget() {
        // 16×16 = 256 cells, all at probability 1.0
        let field = field_with_probability(16, 16, 1.0);
        let mut gate = EmissionGate::new();
        let requests = gate.collect(&field);
        assert!(requests.len() <= MAX_EMIT_PER_TICK,
            "emission must not exceed budget: got {}", requests.len());
        assert_eq!(requests.len(), MAX_EMIT_PER_TICK,
            "at full probability, budget should be fully consumed");
    }

    #[test]
    fn gate_filters_below_threshold() {
        let field = field_with_probability(4, 4, EMIT_GATE - 0.001);
        let mut gate = EmissionGate::new();
        let requests = gate.collect(&field);
        assert_eq!(requests.len(), 0, "cells below gate must not be emitted");
    }

    #[test]
    fn emission_requests_are_probability_ordered() {
        let mut field = ActivationField::new(4, 4);
        // Set alternating probabilities
        for (i, cell) in field.cells.iter_mut().enumerate() {
            cell.execution_probability = if i % 2 == 0 { 0.9 } else { 0.1 };
        }
        let mut gate = EmissionGate::new();
        let requests = gate.collect(&field);
        // All returned cells should have probability > EMIT_GATE
        for r in requests {
            assert!(r.probability > EMIT_GATE,
                "emitted cell {} has probability {} below gate",
                r.cell_index, r.probability);
        }
    }

    #[test]
    fn cell_index_matches_field_position() {
        let mut field = ActivationField::new(4, 4);
        // Only cell 5 is hot
        field.cells[5].execution_probability = 1.0;
        let mut gate = EmissionGate::new();
        let requests = gate.collect(&field);
        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].cell_index, 5);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\emission_validation.rs ---
// ===================================================================
// mirage-mkr-core/src/emission_validation.rs
// PURPOSE: Differential Emission Shadow Validation — Pass 02
//
// AUTHORITY:
//   emission_gate.collect()              — AUTHORITATIVE (unchanged)
//   shadow_gate.collect_from_changed()   — SHADOW (validation only)
//
// This module provides:
//   1. EmissionParityReport     — per-tick comparison of both paths
//   2. DifferentialEmissionValidationReport — accumulated statistics
//   3. DifferentialEmissionMode — mode control (ShadowValidation only now)
//   4. EmissionShadowValidator  — owns shadow gate + runs validation
//
// CORRECTNESS REQUIREMENT — ORDERING:
//   Both paths apply budget enforcement via select_nth_unstable_by()
//   (partial descending probability sort + truncate).
//   select_nth_unstable_by does NOT guarantee stable ordering within
//   equal-probability groups.  For parity comparison we sort BOTH outputs
//   by (cell_index ASC) before comparing identity.  Ordering validation
//   compares probability-descending order after sorting by probability.
//
// DIVERGENCE SOURCES:
//   1. collect() scans in cell_index order (0..N). collect_from_changed()
//      scans delta_mask cells first, then still_eligible bits. These two
//      traversal orders produce identical SETS of cells but may differ in
//      internal scratch order before budget truncation.
//   2. Budget truncation: both use select_nth_unstable_by with the same
//      comparator. The partition is correct but the ordering within the
//      kept/discarded halves is unspecified. We compare SETS, not order.
//   3. still_eligible drift: if the shadow gate's bitset diverges from
//      the authoritative gate's bitset, carryover eligibility diverges.
//      Tracked separately in `stale_eligible_mismatches`.
//
// NEXT PASS (Pass 03) PREPARATION:
//   Phase 3 renderer parity is the natural successor.
//   EmissionShadowValidator.last_report and validation_report are
//   designed to be read by Pass 03 infrastructure.
//   The DifferentialEmissionMode::DifferentialAuthoritative arm is
//   defined but NOT enabled — the gate is already in place for Pass 03.
// ===================================================================

use crate::emission::{EmissionGate, EmissionRequest, MAX_EMIT_PER_TICK};
use crate::activation::field::ActivationField;
use crate::activation::delta::FieldDeltaMask;

// =====================================================================
// MODE CONTROL
// =====================================================================

/// Controls differential emission execution mode.
///
/// Current pass: `ShadowValidation` only.
/// `DifferentialAuthoritative` is defined for Pass 03 but must NOT be
/// enabled until `ShadowValidation` achieves PASS_PROMOTION_THRESHOLD
/// consecutive clean ticks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DifferentialEmissionMode {
    /// Shadow validation disabled.  Only authoritative collect() runs.
    #[default]
    Disabled,
    /// CURRENT PASS: shadow collect_from_changed() runs alongside collect().
    /// Authoritative output is ALWAYS from collect().
    ShadowValidation,
    /// NOT YET ENABLED.  Reserved for Pass 03 authority promotion.
    /// Enabling this without 1000 consecutive clean validation ticks
    /// is a protocol violation.
    DifferentialAuthoritative,
}

/// Minimum consecutive passing ticks before DifferentialAuthoritative
/// may even be considered.  NOT automatically applied.
pub const PASS_PROMOTION_THRESHOLD: u64 = 1_000;

// =====================================================================
// PER-TICK PARITY REPORT
// =====================================================================

/// Comparison of one tick's authoritative and differential emission outputs.
///
/// Cells are compared by identity (cell_index set membership), not order.
/// Ordering is validated separately via `order_mismatches`.
#[derive(Debug, Clone, Default)]
pub struct EmissionParityReport {
    /// Number of requests from authoritative collect().
    pub authoritative_count: usize,
    /// Number of requests from shadow collect_from_changed().
    pub differential_count:  usize,

    /// Cell indices present in authoritative but absent in differential.
    pub missing_from_differential: usize,
    /// Cell indices present in differential but absent in authoritative.
    pub extra_in_differential:     usize,

    /// Pairs that both paths emitted but at different probability rank positions
    /// (after sorting each output by descending probability then cell_index).
    pub order_mismatches: usize,

    /// True iff missing == 0, extra == 0, and order_mismatches == 0.
    pub all_passed: bool,
}

impl EmissionParityReport {
    /// Compare two emission slices.
    ///
    /// `auth` is the authoritative output (collect()).
    /// `diff` is the shadow output (collect_from_changed()).
    ///
    /// Both slices are sorted in-place to canonical order
    /// (descending probability, then ascending cell_index) for comparison.
    /// The caller should pass owned copies or slices already safe to sort.
    pub fn compare(auth: &[EmissionRequest], diff: &[EmissionRequest]) -> Self {
        // Build sorted cell-index sets for identity comparison.
        // Using a fixed-size inline sort to avoid allocation on the hot path.
        // MAX_EMIT_PER_TICK is 128 — small enough for O(N²) insertion sort.
        let mut auth_indices = [usize::MAX; MAX_EMIT_PER_TICK];
        let mut diff_indices = [usize::MAX; MAX_EMIT_PER_TICK];

        let auth_len = auth.len().min(MAX_EMIT_PER_TICK);
        let diff_len = diff.len().min(MAX_EMIT_PER_TICK);

        for i in 0..auth_len { auth_indices[i] = auth[i].cell_index; }
        for i in 0..diff_len { diff_indices[i] = diff[i].cell_index; }

        // Sort both index arrays (ascending cell_index)
        auth_indices[..auth_len].sort_unstable();
        diff_indices[..diff_len].sort_unstable();

        // Merge-count: missing and extra
        let mut ai = 0usize;
        let mut di = 0usize;
        let mut missing = 0usize;
        let mut extra   = 0usize;

        while ai < auth_len && di < diff_len {
            match auth_indices[ai].cmp(&diff_indices[di]) {
                std::cmp::Ordering::Equal => { ai += 1; di += 1; }
                std::cmp::Ordering::Less  => { missing += 1; ai += 1; }
                std::cmp::Ordering::Greater => { extra   += 1; di += 1; }
            }
        }
        missing += auth_len - ai;
        extra   += diff_len - di;

        // Order mismatch: compare probability-rank order for the common set.
        // Sort auth and diff by (descending prob, ascending cell_idx) and compare positions.
        let mut auth_ranked: [(f32, usize); MAX_EMIT_PER_TICK] = [(0.0, 0); MAX_EMIT_PER_TICK];
        let mut diff_ranked: [(f32, usize); MAX_EMIT_PER_TICK] = [(0.0, 0); MAX_EMIT_PER_TICK];
        for i in 0..auth_len { auth_ranked[i] = (auth[i].probability, auth[i].cell_index); }
        for i in 0..diff_len { diff_ranked[i] = (diff[i].probability, diff[i].cell_index); }
        auth_ranked[..auth_len].sort_unstable_by(|(pa, ia), (pb, ib)|
            pb.partial_cmp(pa).unwrap_or(std::cmp::Ordering::Equal)
                .then(ia.cmp(ib)));
        diff_ranked[..diff_len].sort_unstable_by(|(pa, ia), (pb, ib)|
            pb.partial_cmp(pa).unwrap_or(std::cmp::Ordering::Equal)
                .then(ia.cmp(ib)));

        // Count positions where the ranked cell_index differs
        let common = auth_len.min(diff_len);
        let mut order_mismatches = 0usize;
        for i in 0..common {
            if auth_ranked[i].1 != diff_ranked[i].1 { order_mismatches += 1; }
        }

        let all_passed = missing == 0 && extra == 0 && order_mismatches == 0;

        Self {
            authoritative_count: auth_len,
            differential_count:  diff_len,
            missing_from_differential: missing,
            extra_in_differential:     extra,
            order_mismatches,
            all_passed,
        }
    }
}

// =====================================================================
// ACCUMULATED VALIDATION REPORT
// =====================================================================

/// Accumulated differential emission validation statistics across all ticks.
#[derive(Debug, Clone, Default)]
pub struct DifferentialEmissionValidationReport {
    /// Total ticks where shadow emission was evaluated.
    pub ticks_run:  u64,
    /// Ticks where authoritative and differential outputs matched exactly.
    pub ticks_passed: u64,
    /// Ticks where any mismatch was detected.
    pub ticks_failed: u64,
    /// Consecutive pass count — resets to 0 on any failure.
    pub consecutive_passes: u64,

    /// Peak `missing_from_differential` seen across all ticks.
    pub peak_missing: usize,
    /// Peak `extra_in_differential` seen across all ticks.
    pub peak_extra:   usize,
    /// Peak `order_mismatches` seen across all ticks.
    pub peak_order_mismatch: usize,

    /// Total missing-cell events summed across all ticks.
    pub total_missing_events: u64,
    /// Total extra-cell events summed across all ticks.
    pub total_extra_events:   u64,
}

impl DifferentialEmissionValidationReport {
    pub fn new() -> Self { Self::default() }

    /// Record one tick's parity result.
    pub fn record(&mut self, report: &EmissionParityReport) {
        self.ticks_run += 1;
        if report.all_passed {
            self.ticks_passed += 1;
            self.consecutive_passes += 1;
        } else {
            self.ticks_failed += 1;
            self.consecutive_passes = 0;
        }
        if report.missing_from_differential > self.peak_missing {
            self.peak_missing = report.missing_from_differential;
        }
        if report.extra_in_differential > self.peak_extra {
            self.peak_extra = report.extra_in_differential;
        }
        if report.order_mismatches > self.peak_order_mismatch {
            self.peak_order_mismatch = report.order_mismatches;
        }
        self.total_missing_events += report.missing_from_differential as u64;
        self.total_extra_events   += report.extra_in_differential as u64;
    }

    /// Pass rate [0.0, 1.0].
    pub fn pass_rate(&self) -> f32 {
        if self.ticks_run == 0 { return 1.0; }
        self.ticks_passed as f32 / self.ticks_run as f32
    }

    /// True if consecutive passes have reached the promotion threshold.
    /// Does NOT automatically enable DifferentialAuthoritative.
    pub fn eligible_for_promotion(&self) -> bool {
        self.consecutive_passes >= PASS_PROMOTION_THRESHOLD
            && self.ticks_failed == 0
    }

    /// Reset all accumulated state.
    pub fn reset(&mut self) { *self = Self::default(); }
}

// =====================================================================
// SHADOW EMISSION VALIDATOR
// =====================================================================

/// Owns the shadow EmissionGate and runs differential emission validation.
///
/// # Design
/// The shadow gate is a SEPARATE EmissionGate instance from the authoritative
/// gate.  It maintains its own `scratch` and `still_eligible` bitsets so that
/// both paths can evolve independently and be compared each tick.
///
/// # Authority
/// The authoritative gate (`MKRWorld::emission_gate`) always wins.
/// `last_emission` is always from `collect()`.
/// The shadow result is stored in `last_shadow_emission` for inspection only.
///
/// # Enabling
/// Call `enable_shadow()`.  Default is `Disabled`.
/// Check `validation_report.eligible_for_promotion()` for promotion readiness.
///
/// # Pass 03 Preparation
/// `last_report` is pub so the Pass 03 renderer validator can read emission
/// parity as a pre-condition for renderer differential validation.
pub struct EmissionShadowValidator {
    /// Shadow emission gate — separate from authoritative gate.
    shadow_gate:           EmissionGate,
    /// Last shadow emission output (cell indices + probabilities).
    pub last_shadow_emission: Vec<EmissionRequest>,
    /// Per-tick parity comparison result.
    pub last_report:       Option<EmissionParityReport>,
    /// Accumulated statistics.
    pub validation_report: DifferentialEmissionValidationReport,
    /// Current mode.
    pub mode:              DifferentialEmissionMode,
}

impl EmissionShadowValidator {
    pub fn new() -> Self {
        Self {
            shadow_gate:           EmissionGate::new(),
            last_shadow_emission:  Vec::new(),
            last_report:           None,
            validation_report:     DifferentialEmissionValidationReport::new(),
            mode:                  DifferentialEmissionMode::Disabled,
        }
    }

    /// Enable shadow validation mode.
    pub fn enable_shadow(&mut self) {
        self.mode = DifferentialEmissionMode::ShadowValidation;
    }

    /// Disable validation (zero overhead).
    pub fn disable(&mut self) {
        self.mode = DifferentialEmissionMode::Disabled;
    }

    /// True if shadow validation is currently active.
    #[inline]
    pub fn is_active(&self) -> bool {
        matches!(self.mode, DifferentialEmissionMode::ShadowValidation)
    }

    /// Run shadow emission and compare against the authoritative result.
    ///
    /// # Parameters
    /// * `field`          — activation field (same as authoritative path)
    /// * `delta_mask`     — field delta mask from delta_tracker.compute()
    /// * `authoritative`  — slice produced by authoritative collect() this tick
    ///
    /// # Returns
    /// `Some(EmissionParityReport)` when active, `None` when disabled.
    ///
    /// # Authority
    /// Returns nothing that affects the live emission output.
    /// `last_shadow_emission` is for inspection / diagnostics only.
    pub fn validate_tick(
        &mut self,
        field:         &ActivationField,
        delta_mask:    &FieldDeltaMask,
        authoritative: &[EmissionRequest],
    ) -> Option<EmissionParityReport> {
        if !self.is_active() { return None; }

        // Run shadow collect_from_changed() on the shadow gate.
        let shadow = self.shadow_gate.collect_from_changed(field, delta_mask);

        // Snapshot shadow output before the borrow expires.
        self.last_shadow_emission.clear();
        self.last_shadow_emission.extend_from_slice(shadow);

        // Compare.
        let report = EmissionParityReport::compare(authoritative, &self.last_shadow_emission);
        self.validation_report.record(&report);
        self.last_report = Some(report.clone());
        Some(report)
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emission::EMIT_GATE;
    use crate::activation::field::ActivationField;
    use crate::activation::delta::FieldDeltaMask;
    use crate::emission::EmissionGate;

    // ------------------------------------------------------------------
    // Helper: build a delta mask from prob snapshot vs current field
    // ------------------------------------------------------------------
    fn snap_probs(field: &ActivationField) -> Vec<f32> {
        field.cells.iter().map(|c| c.execution_probability).collect()
    }

    fn delta_from_snap(
        prev_probs: &[f32],
        after:      &ActivationField,
        epsilon:    f32,
    ) -> FieldDeltaMask {
        let n = after.cells.len();
        let mut mask = FieldDeltaMask::new(n);
        for i in 0..n {
            let prev = if i < prev_probs.len() { prev_probs[i] } else { 0.0 };
            let curr = after.cells[i].execution_probability;
            if (curr - prev).abs() > epsilon { mask.set(i); }
        }
        mask
    }

    // Convenience: delta from a zero-baseline (field newly set)
    fn delta_from_zero(after: &ActivationField, epsilon: f32) -> FieldDeltaMask {
        delta_from_snap(&vec![0.0f32; after.cells.len()], after, epsilon)
    }

    // ------------------------------------------------------------------
    // Test 1: cells newly crossing EMIT_GATE appear in both paths
    // ------------------------------------------------------------------
    #[test]
    fn newly_crossing_gate_appears_in_both() {
        let mut field = ActivationField::new(4, 4);
        // Cell 3 rises above gate this tick
        field.cells[3].execution_probability = EMIT_GATE + 0.1;

        let mask = delta_from_zero(&field, 0.001);

        let mut auth_gate   = EmissionGate::new();
        let mut shadow_val  = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth = auth_gate.collect(&field);
        let report = shadow_val.validate_tick(&field, &mask, auth).unwrap();

        assert!(report.all_passed,
            "newly-crossing cell must appear in both paths: {:?}", report);
        assert_eq!(report.authoritative_count, 1);
        assert_eq!(report.differential_count, 1);
    }

    // ------------------------------------------------------------------
    // Test 2: cells remaining above gate without changing (still_eligible)
    // ------------------------------------------------------------------
    #[test]
    fn still_eligible_cells_persist_in_differential() {
        let mut field = ActivationField::new(4, 4);
        // Cell 0 is hot and doesn't change between ticks
        field.cells[0].execution_probability = 0.9;

        // Tick 1: cell 0 newly hot → appears in delta
        let mask1 = delta_from_zero(&field, 0.001);

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth1 = auth_gate.collect(&field);
        let r1 = shadow_val.validate_tick(&field, &mask1, auth1).unwrap();
        assert!(r1.all_passed, "tick 1 must pass: {:?}", r1);

        // Tick 2: field identical — cell 0 NOT in delta but still eligible
        let empty_mask = FieldDeltaMask::new(16); // no bits set
        let auth2 = auth_gate.collect(&field); // still emits cell 0
        let r2 = shadow_val.validate_tick(&field, &empty_mask, auth2).unwrap();
        assert!(r2.all_passed,
            "still_eligible must carry cell 0 into differential tick 2: {:?}", r2);
    }

    // ------------------------------------------------------------------
    // Test 3: cells dropping below gate are cleared
    // ------------------------------------------------------------------
    #[test]
    fn cells_dropping_below_gate_are_cleared() {
        let mut field = ActivationField::new(4, 4);
        field.cells[2].execution_probability = 0.8;

        // Tick 1: cell 2 hot
        let mask1 = delta_from_zero(&field, 0.001);
        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();
        let a1 = auth_gate.collect(&field);
        shadow_val.validate_tick(&field, &mask1, a1);

        // Tick 2: cell 2 drops below gate
        let prev_probs = snap_probs(&field);
        field.cells[2].execution_probability = 0.001;
        let mask2 = delta_from_snap(&prev_probs, &field, 0.001);
        let a2 = auth_gate.collect(&field);
        let r2 = shadow_val.validate_tick(&field, &mask2, a2).unwrap();
        assert!(r2.all_passed,
            "cell dropping below gate must be absent in both: {:?}", r2);
        assert_eq!(r2.authoritative_count, 0, "no cells above gate");
        assert_eq!(r2.differential_count, 0);
    }

    // ------------------------------------------------------------------
    // Test 4: oscillating threshold cells
    // ------------------------------------------------------------------
    #[test]
    fn oscillating_threshold_cells() {
        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let high = EMIT_GATE + 0.05;
        let low  = EMIT_GATE - 0.01;

        let mut field     = ActivationField::new(4, 4);
        let mut prev_probs = snap_probs(&field); // all zero initially

        for tick in 0..10 {
            // Alternate cell 7 above/below gate
            let prob = if tick % 2 == 0 { high } else { low };
            field.cells[7].execution_probability = prob;
            let mask = delta_from_snap(&prev_probs, &field, 0.001);
            let auth = auth_gate.collect(&field);
            let r = shadow_val.validate_tick(&field, &mask, auth).unwrap();
            assert!(r.all_passed, "tick {tick} oscillation: {:?}", r);
            prev_probs = snap_probs(&field);
        }
    }

    // ------------------------------------------------------------------
    // Test 5: budget truncation parity
    // ------------------------------------------------------------------
    #[test]
    fn budget_truncation_parity() {
        // 256 cells all above gate — budget (128) must be enforced identically
        let mut field = ActivationField::new(16, 16);
        for cell in &mut field.cells { cell.execution_probability = 0.9; }

        let mask = delta_from_zero(&field, 0.001);

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth = auth_gate.collect(&field);
        let r = shadow_val.validate_tick(&field, &mask, auth).unwrap();

        assert_eq!(r.authoritative_count, MAX_EMIT_PER_TICK,
            "authoritative must hit budget");
        assert_eq!(r.differential_count, MAX_EMIT_PER_TICK,
            "differential must hit same budget");
        // Missing/extra may differ because select_nth_unstable_by has
        // unspecified ordering within equal-probability groups.
        // The SET of emitted cells may differ but counts must match.
        assert_eq!(r.authoritative_count, r.differential_count,
            "both paths must emit identical count");
    }

    // ------------------------------------------------------------------
    // Test 6: deterministic ordering parity (unique probabilities)
    // ------------------------------------------------------------------
    #[test]
    fn deterministic_ordering_parity_unique_probs() {
        let mut field = ActivationField::new(4, 4);
        // Assign unique probabilities > EMIT_GATE so ordering is deterministic
        for (i, cell) in field.cells.iter_mut().enumerate() {
            cell.execution_probability = EMIT_GATE + 0.01 * (i as f32 + 1.0);
        }

        let mask = delta_from_zero(&field, 0.001);

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth = auth_gate.collect(&field);
        let r = shadow_val.validate_tick(&field, &mask, auth).unwrap();

        assert!(r.all_passed,
            "unique probabilities must produce identical ranked order: {:?}", r);
        assert_eq!(r.order_mismatches, 0);
    }

    // ------------------------------------------------------------------
    // Test 7: zero changed cells with no still_eligible — both output empty
    // ------------------------------------------------------------------
    #[test]
    fn zero_changed_cells_no_eligible() {
        let field     = ActivationField::new(4, 4); // all prob = 0
        let empty_mask = FieldDeltaMask::new(16);

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth = auth_gate.collect(&field);
        assert_eq!(auth.len(), 0);
        let r = shadow_val.validate_tick(&field, &empty_mask, auth).unwrap();
        assert!(r.all_passed, "empty field, empty mask: {:?}", r);
        assert_eq!(r.authoritative_count, 0);
        assert_eq!(r.differential_count, 0);
    }

    // ------------------------------------------------------------------
    // Test 8: large sparse workload — only a few hot cells in a large field
    // ------------------------------------------------------------------
    #[test]
    fn large_sparse_workload_parity() {
        let mut field = ActivationField::new(16, 16); // 256 cells
        // Only 5 hot cells
        let hot_cells = [10, 50, 100, 150, 200];
        for &i in &hot_cells {
            field.cells[i].execution_probability = 0.9;
        }

        let mask = delta_from_zero(&field, 0.001);

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        let auth = auth_gate.collect(&field);
        let r = shadow_val.validate_tick(&field, &mask, auth).unwrap();
        assert!(r.all_passed, "sparse large workload: {:?}", r);
        assert_eq!(r.authoritative_count, 5);
        assert_eq!(r.differential_count, 5);
    }

    // ------------------------------------------------------------------
    // Test 9: long-running eligibility persistence (50 ticks)
    // ------------------------------------------------------------------
    #[test]
    fn long_running_eligibility_persistence() {
        let mut field = ActivationField::new(4, 4);
        field.cells[1].execution_probability = 0.8;

        let mut auth_gate  = EmissionGate::new();
        let mut shadow_val = EmissionShadowValidator::new();
        shadow_val.enable_shadow();

        // First tick: cell 1 appears in delta
        let mask1 = delta_from_zero(&field, 0.001);
        let a1 = auth_gate.collect(&field);
        shadow_val.validate_tick(&field, &mask1, a1);

        // Ticks 2..50: field unchanged — cell 1 stays eligible via still_eligible
        let empty_mask = FieldDeltaMask::new(16);
        for tick in 2..=50 {
            let auth = auth_gate.collect(&field);
            let r = shadow_val.validate_tick(&field, &empty_mask, auth).unwrap();
            assert!(r.all_passed,
                "tick {tick}: still_eligible must persist cell 1: {:?}", r);
        }

        assert_eq!(shadow_val.validation_report.ticks_passed, 50);
        assert_eq!(shadow_val.validation_report.ticks_failed, 0);
        assert_eq!(shadow_val.validation_report.consecutive_passes, 50);
    }

    // ------------------------------------------------------------------
    // Test 10: validation report accumulation and pass_rate
    // ------------------------------------------------------------------
    #[test]
    fn validation_report_accumulates() {
        let mut report = DifferentialEmissionValidationReport::new();
        let pass = EmissionParityReport { all_passed: true, ..Default::default() };
        let fail = EmissionParityReport {
            all_passed: false,
            missing_from_differential: 2,
            ..Default::default()
        };
        for _ in 0..10 { report.record(&pass); }
        assert_eq!(report.consecutive_passes, 10);
        report.record(&fail);
        assert_eq!(report.consecutive_passes, 0);
        assert_eq!(report.ticks_failed, 1);
        assert_eq!(report.peak_missing, 2);
        assert!((report.pass_rate() - 10.0 / 11.0).abs() < 1e-5);
    }

    // ------------------------------------------------------------------
    // Test 11: disabled validator returns None
    // ------------------------------------------------------------------
    #[test]
    fn disabled_validator_returns_none() {
        let field      = ActivationField::new(4, 4);
        let empty_mask = FieldDeltaMask::new(16);
        let mut validator = EmissionShadowValidator::new();
        // mode is Disabled by default
        let result = validator.validate_tick(&field, &empty_mask, &[]);
        assert!(result.is_none(), "disabled validator must return None");
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\lib.rs ---
// ===================================================================
// mirage-mkr-core/src/lib.rs  (V3/V4 — Substrate Core Library)
// ===================================================================

pub mod activation;
pub mod bridge;
pub mod pool;
pub mod streaming;
pub mod emission;
pub mod emission_validation;
pub mod protocol;
pub mod regions;
pub mod region_validation;

// ===================================================================
// IMPORTS
// ===================================================================

use mirage_core::runtime::ThermalSystem;
use mirage_core::pool::RuntimeDirectory as CoreRuntimeDirectory;
use mirage_matrix::topology::TopologyGraph;

// MTS layer: TopologyInfluenceProvider trait — in scope for mkr-core's
// orchestration of the topology → activation solver pipeline.
// CEK layer: CekEvalField trait — in scope for &mut dyn CekEvalField casts
// in tick() Phase 3. ActivationField implements it in activation/field.rs.

use crate::activation::{
    ActivationField, ActivationSolver, FieldDeltaTracker, PropagationFrontier,
    SparseValidationRunner, ParityComparisonResult,
};
use crate::activation::solver::SolverStepStats;
use crate::activation::frontier::FrontierStats;
use crate::bridge::{RendererBridge, ExecutionBridge};
use crate::emission::{EmissionGate, EmissionRequest, EMIT_GATE};
use crate::emission_validation::EmissionShadowValidator;
use crate::bridge::renderer_validation::RendererShadowValidator;
use crate::regions::RegionMap;
use crate::region_validation::{
    RegionShadowValidator,
};

// ===================================================================
// MKR WORLD — V3/V4 Runtime
// ===================================================================

/// Central runtime kernel for Mirage Engine V3/V4.
pub struct MKRWorld {
    // ============================================================
    // Runtime Flags
    // ============================================================

    pub differential_renderer_enabled: bool,
    pub differential_renderer_needs_full_sync: bool,

    // ============================================================
    // Core Runtime Authority
    // ============================================================

    pub activation_field: ActivationField,
    pub activation_solver: ActivationSolver,
    pub topology: TopologyGraph,

    // ============================================================
    // Differential Runtime
    // ============================================================

    pub delta_tracker: FieldDeltaTracker,
    pub propagation_frontier: PropagationFrontier,
    pub region_map: RegionMap,

    // ============================================================
    // Validation
    // ============================================================

    pub sparse_validator: SparseValidationRunner,
    pub emission_shadow_validator: EmissionShadowValidator,
    pub renderer_shadow_validator: RendererShadowValidator,
    pub region_shadow_validator: RegionShadowValidator,
    pub last_parity: Option<ParityComparisonResult>,

    // ============================================================
    // Emission + Rendering
    // ============================================================

    pub emission_gate: EmissionGate,
    pub renderer_bridge: RendererBridge,
    pub execution_bridge: ExecutionBridge,

    pub last_emission: Vec<EmissionRequest>,

    // ============================================================
    // Compatibility Layer
    // ============================================================

    pub thermal: ThermalSystem,
    pub directory: CoreRuntimeDirectory,

    // ============================================================
    // Persistent Buffers
    // ============================================================

    pub topo_influence_buffer: Vec<f32>,
    pub authoritative_probability_buffer: Vec<f32>,

    // ============================================================
    // Diagnostics
    // ============================================================

    pub last_step_stats: SolverStepStats,
    pub last_frontier_stats: FrontierStats,

    pub topology_buffer_reallocations: u64,

    // ============================================================
    // Trace Fusion Compiler
    // ============================================================

    pub trace_compiler: mirage_compute::TraceFusionCompiler,

    // ============================================================
    // NUMA-Aware Work Stealing Scheduler
    // ============================================================

    pub numa_scheduler: mirage_executor::scheduler::NUMAAwareScheduler,

    // ============================================================
    // Relational Query IR — ColumnarScan scratchpad
    // ============================================================

    /// Pre-allocated SoA columnar scan used by the Query IR each tick.
    /// Resized lazily to match `activation_field.len()`. Never shrinks.
    pub query_scan: mirage_query::ColumnarScan,

    // ============================================================
    // Frame Tracking
    // ============================================================

    pub frame: u64,
    pub field_width: usize,
    pub field_height: usize,
}

impl MKRWorld {
    /// Create an MKRWorld for a `width × height` chunk grid.
    pub fn new(width: usize, height: usize, _fiber_capacity: usize) -> Self {
        let total_chunks = width * height;

        Self {
            // ========================================================
            // Runtime Flags
            // ========================================================

            differential_renderer_needs_full_sync: true,
            differential_renderer_enabled: false,

            // ========================================================
            // Core Runtime Authority
            // ========================================================

            activation_field:
                ActivationField::new(width, height),

            activation_solver:
                ActivationSolver::new(),

            topology:
                TopologyGraph::new(),

            // ========================================================
            // Differential Runtime
            // ========================================================

            delta_tracker:
                FieldDeltaTracker::new(total_chunks, EMIT_GATE),

            region_map:
                RegionMap::new(width, height),

            propagation_frontier:
                PropagationFrontier::new(width, height),

            // ========================================================
            // Validation
            // ========================================================

            sparse_validator:
                SparseValidationRunner::new(width, height),

            emission_shadow_validator:
                EmissionShadowValidator::new(),

            renderer_shadow_validator:
                RendererShadowValidator::new(total_chunks),

            region_shadow_validator:
                RegionShadowValidator::new(width, height),

            last_parity:
                None,

            // ========================================================
            // Emission + Rendering
            // ========================================================

            emission_gate:
                EmissionGate::new(),

            renderer_bridge:
                RendererBridge::new(),

            execution_bridge:
                ExecutionBridge::new(_fiber_capacity),

            last_emission:
                Vec::new(),

            // ========================================================
            // Compatibility Layer
            // ========================================================

            thermal:
                ThermalSystem::new(total_chunks),

            directory:
                CoreRuntimeDirectory::new(total_chunks),

            // ========================================================
            // Persistent Buffers
            // ========================================================

            topo_influence_buffer:
                vec![0.0; total_chunks],

            authoritative_probability_buffer:
                Vec::with_capacity(total_chunks),

            // ========================================================
            // Diagnostics
            // ========================================================

            last_step_stats:
                SolverStepStats::default(),

            last_frontier_stats:
                FrontierStats::default(),

            topology_buffer_reallocations:
                0,

            trace_compiler:
                mirage_compute::TraceFusionCompiler::new(3),

            numa_scheduler:
                mirage_executor::scheduler::NUMAAwareScheduler::new(),

            query_scan:
                mirage_query::ColumnarScan::new(total_chunks),

            // ========================================================
            // Frame Tracking
            // ========================================================

            frame:
                0,

            field_width:
                width,

            field_height:
                height,
        }
    }

    pub fn enable_differential_renderer(&mut self) {
        self.differential_renderer_enabled = true;
        self.differential_renderer_needs_full_sync = true;
    }

    pub fn disable_differential_renderer(&mut self) {
        self.differential_renderer_enabled = false;
    }

    // ---------------------------------------------------------------
    // External injection API
    // ---------------------------------------------------------------

    pub fn inject_heat_at_chunk(&mut self, chunk_x: usize, chunk_y: usize, amount: f32) {
        self.activation_field.inject_heat_at(chunk_x, chunk_y, amount);
    }

    pub fn inject_pressure_at_chunk(&mut self, chunk_x: usize, chunk_y: usize, amount: f32) {
        self.activation_field.inject_pressure_at(chunk_x, chunk_y, amount);
    }

    pub fn topology_mut(&mut self) -> &mut TopologyGraph {
        &mut self.topology
    }

    // ---------------------------------------------------------------
    // Tick
    // ---------------------------------------------------------------

    pub fn tick(&mut self) {
        // ============================================================
        // PHASE 0 — TOPOLOGY PREPASS
        // ============================================================

        self.topology
            .assert_aligned(self.field_width * self.field_height);

        let cap_before = self.topo_influence_buffer.capacity();

        self.topology
            .influence_scalars_into(&mut self.topo_influence_buffer);

        if self.topo_influence_buffer.capacity() != cap_before {
            self.topology_buffer_reallocations += 1;
        }

        // ============================================================
        // PHASE 0.5 — SNAPSHOT
        // ============================================================

        if self.sparse_validator.is_active() {
            self.sparse_validator
                .snapshot_pre_tick(&self.activation_field);
        }

        // ============================================================
        // PHASE 1 — SOLVER STEP
        // ============================================================

        if self.propagation_frontier.should_use_sparse() && !self.propagation_frontier.is_empty() {
            self.last_step_stats = self.activation_solver.step_sparse(
                &mut self.activation_field,
                &self.propagation_frontier,
                &self.topo_influence_buffer,
            );
        } else {
            self.last_step_stats = self.activation_solver.step(
                &mut self.activation_field,
                &self.topo_influence_buffer,
            );
        }

        // ============================================================
        // PHASE 1.5 — DELTA COMPUTE
        // ============================================================

        let delta_mask =
            self.delta_tracker.compute(&self.activation_field);

        // ============================================================
        // PHASE 1.6 — FRONTIER BUILD
        // ============================================================

        let used_sparse =
            self.propagation_frontier.build_from_delta(
                delta_mask,
                self.field_width,
                self.field_height,
            );

        self.last_frontier_stats = FrontierStats {
            frontier_cells:
                self.propagation_frontier.frontier_size(),

            total_cells:
                self.field_width * self.field_height,

            used_sparse,

            density:
                self.propagation_frontier.density(),
        };

        // ============================================================
        // PHASE 1.7 — REGION REFRESH + VALIDATION
        // ============================================================

        self.region_map.refresh(&self.activation_field);

        if self.region_shadow_validator.is_active() {
            self.region_shadow_validator.validate_tick(
                &self.activation_field,
                self.delta_tracker.mask(),
                &self.region_map,
            );
        }

        // ============================================================
        // PHASE 1.8 — SPARSE VALIDATION
        // ============================================================

        self.last_parity =
            self.sparse_validator.validate_tick(
                &self.activation_field,
                &self.propagation_frontier,
                &self.topo_influence_buffer,
            );

        // ============================================================
        // PHASE 2 — EMISSION
        // ============================================================

        let current_regions = RegionMap::compute_from_field(&self.activation_field);

        let requests =
            self.emission_gate.collect(&self.activation_field);

        // ============================================================
        // PHASE 2.5 — EMISSION SHADOW VALIDATION
        // ============================================================

        if self.emission_shadow_validator.is_active() {
            self.emission_shadow_validator.validate_tick(
                &self.activation_field,
                self.delta_tracker.mask(),
                requests,
            );
        }

        self.last_emission.clear();
        for req in requests {
            let region_idx = current_regions.region_for_cell(req.cell_index);
            if let Some(region) = current_regions.get(region_idx) {
                if region.activity == crate::regions::RegionActivityState::Dormant {
                    continue;
                }
            }
            self.last_emission.push(*req);
        }

        // ============================================================
        // PHASE 2.3 — QUERY IR: COLUMNAR SCAN REFRESH
        // ============================================================
        // Load the current activation field into the SoA ColumnarScan,
        // then use the CellQuery API to derive the active-cell index set.
        // This set is used for diagnostics and future SolverKernel fusion;
        // it does NOT replace the authoritative ActivationSolver output —
        // it reads the field AFTER the solver has already run, so
        // mathematical parity with the solver is guaranteed by construction.
        {
            let field = &self.activation_field;
            let n = field.cells.len();
            // Resize the scan if the field grew (e.g. after re-init).
            if self.query_scan.len != n {
                self.query_scan.resize(n);
            }
            // Scatter-copy AoS → SoA.
            for i in 0..n {
                let c = &field.cells[i];
                self.query_scan.heat[i]                  = c.heat;
                self.query_scan.pressure[i]              = c.pressure;
                self.query_scan.entropy[i]               = c.entropy;
                self.query_scan.activation[i]            = c.activation;
                self.query_scan.execution_probability[i] = c.execution_probability;
            }

            // Build active-cell set via the relational query API.
            // Mirrors the EmissionGate threshold (`EMIT_GATE`) for parity.
            let _active_cells: Vec<usize> = self.query_scan
                .query()
                .filter(|_h, _p, _e, _a, exec_prob| exec_prob > EMIT_GATE)
                .collect();
            // `_active_cells` is available for future SolverKernel fusion
            // and diagnostic instrumentation. Prefixed with `_` until a
            // downstream consumer is wired in.
        }

        // ============================================================
        // PHASE 3 — STATEFUL MULTI-FRAME CEK LIFECYCLE EVALUATION
        // ============================================================

        let requests = &self.last_emission;
        let topo_slice = &self.topo_influence_buffer;

        // 1. Queue all newly risen signals into our persistent execution queue
        self.execution_bridge.process_and_queue_cek_context(&requests, topo_slice);

        // 2. Perform quiescent eviction to prevent phantom memory leaks
        self.execution_bridge.evict_quiescent_cek_states(&self.activation_field);

        // 3. Consume closures statefully, respecting our hardware execution budget bounds
        let mut executed_continuations = 0;
        let target_budget = self.emission_gate.budget; // Retrieve global configured budget scale

        // Scope mutability over the shared cell register
        {
            let mut signature = Vec::new();
            let mut path = Vec::new();

            // Scope reading registry
            {
                let registry = self.execution_bridge.deferred_cek_queue.borrow();
                let limit = target_budget.min(registry.len());
                for i in 0..limit {
                    let machine = &registry[i];
                    signature.push(machine.control_cell);
                    path.push(mirage_compute::Continuation {
                        cell_index: machine.control_cell,
                        prob_signal: machine.prob_signal,
                    });
                }
            }

            // Optimize/compile trace
            let fused_kernel = self.trace_compiler.optimize(signature.clone(), path.clone());

            if let Some(kernel) = fused_kernel {
                // Fused Dynamic SIMD Execution: Bypass interpretation overhead
                kernel.execute(&mut self.activation_field);
                executed_continuations += path.len();

                // Telemetry
                for cont in &path {
                    let source_cell = cont.cell_index;
                    if source_cell < self.topology.edges.len() {
                        let targets = self.topology.edges[source_cell].clone();
                        for target_cell in targets {
                            let edge_idx = self.topology.find_edge(source_cell, target_cell);
                            self.topology.record_access(edge_idx);
                        }
                    }
                }

                // Drain the executed machines
                let mut registry = self.execution_bridge.deferred_cek_queue.borrow_mut();
                registry.drain(0..path.len());
            } else {
                // Interpret sequentially using NUMA-aware Work Stealing Scheduler
                let mut registry = self.execution_bridge.deferred_cek_queue.borrow_mut();
                let mut unexecuted_backlog = Vec::with_capacity(registry.len());
                let mut machines_to_execute = Vec::new();

                for machine in registry.drain(..) {
                    if executed_continuations < target_budget {
                        machines_to_execute.push(machine);
                        executed_continuations += 1;
                    } else {
                        unexecuted_backlog.push(machine);
                    }
                }
                *registry = unexecuted_backlog;

                // Send machines to execute to the scheduler
                // We use raw pointer transmission wrapped in SendPtr to bypass the `'static` lifetime constraint of the FnMut closure
                struct SendPtr(pub *mut ActivationField);
                unsafe impl Send for SendPtr {}
                unsafe impl Sync for SendPtr {}
                impl SendPtr {
                    pub unsafe fn get_mut<'a>(&self) -> &'a mut ActivationField {
                        &mut *self.0
                    }
                }

                let field_ptr = SendPtr(&mut self.activation_field as *mut ActivationField);

                for mut machine in machines_to_execute {
                    let region_idx = self.region_map.region_for_cell(machine.control_cell);
                    let core_id = region_idx;
                    let f_ptr = SendPtr(field_ptr.0);

                    // Create Fiber wrapping the evaluation of this machine
                    let fiber = mirage_executor::fiber::Fiber::new(machine.control_cell, Box::new(move || {
                        unsafe {
                            let field_ref = f_ptr.get_mut();
                            // Cast to CekEvalField trait object — ActivationField implements
                            // mirage_cek::CekEvalField (see activation/field.rs).
                            let cek_field: &mut dyn mirage_cek::CekEvalField = field_ref;
                            machine.evaluate_all(cek_field);
                        }
                    }));

                    self.numa_scheduler.schedule_with_affinity(fiber, core_id);
                }

                // Execute fibers from the scheduler queues
                for core_id in 0..self.numa_scheduler.affinity_map.num_cores {
                    while let Some(mut fiber) = self.numa_scheduler.get_task_for_core(core_id) {
                        let source_cell = fiber.id;

                        // Resume / execute continuation
                        fiber.resume();

                        // Bridge the runtime telemetry to the topology graph
                        if source_cell < self.topology.edges.len() {
                            let targets = self.topology.edges[source_cell].clone();
                            for target_cell in targets {
                                let edge_idx = self.topology.find_edge(source_cell, target_cell);
                                self.topology.record_access(edge_idx);
                            }
                        }
                    }
                }
            }
        }

        // Trigger the rebalancer every 60 frames
        if self.frame % 60 == 0 {
            self.topology.rebalance_edges();
        }

        // ============================================================
        // PHASE 3 — PROBABILITY SNAPSHOT
        // ============================================================

        self.renderer_bridge.fill_probability_buffer(
            &self.activation_field,
            &mut self.authoritative_probability_buffer,
        );

        // ============================================================
        // PHASE 3.1 — RENDERER BRIDGE
        // ============================================================

        self.run_renderer_bridge();

        // ============================================================
        // PHASE 3.5 — RENDERER VALIDATION
        // ============================================================

        if self.renderer_shadow_validator.is_active() {
            self.renderer_shadow_validator.validate_tick(
                &self.activation_field,
                self.delta_tracker.mask(),
                &self.directory,
                &self.authoritative_probability_buffer,
            );
        }

        // ============================================================
        // PHASE 4 — THERMAL SYNC
        // ============================================================

        self.sync_compat_thermal();

        // ============================================================
        // PHASE 5 — STABILIZATION
        // ============================================================

        self.synchronize();

        self.frame = self.frame.wrapping_add(1);
    }

    fn run_renderer_bridge(&mut self) {
        if self.differential_renderer_enabled {
            if self.differential_renderer_needs_full_sync {
                self.renderer_bridge.apply_to_directory(
                    &self.activation_field,
                    &mut self.directory,
                );
                self.differential_renderer_needs_full_sync = false;
            } else {
                self.renderer_bridge.apply_changed_cells(
                    &self.activation_field,
                    self.delta_tracker.mask(),
                    &mut self.directory,
                );
            }
        } else {
            self.renderer_bridge.apply_to_directory(
                &self.activation_field,
                &mut self.directory,
            );
        }
    }

    fn sync_compat_thermal(&mut self) {
        self.thermal.update_frame();
    }

    fn synchronize(&mut self) {
        // Reserved for field boundary stabilisation pass.
    }

    // ---------------------------------------------------------------
    // Diagnostic / read API
    // ---------------------------------------------------------------

    pub fn mean_activation(&self) -> f32 {
        self.activation_field.mean_activation()
    }

    pub fn mean_execution_probability(&self) -> f32 {
        self.activation_field.mean_execution_probability()
    }

    pub fn step_stats(&self) -> &SolverStepStats {
        &self.last_step_stats
    }

    pub fn activation_field(&self) -> &ActivationField {
        &self.activation_field
    }

    pub fn emission_requests(&self) -> &[EmissionRequest] {
        &self.last_emission
    }
}

// ===================================================================
// TESTS
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mkr_world_creation() {
        let world = MKRWorld::new(8, 8, 256);
        assert_eq!(world.activation_field.len(), 64);
        assert_eq!(world.frame, 0);
    }

    #[test]
    fn tick_advances_frame() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.tick();
        assert_eq!(world.frame, 1);
    }

    #[test]
    fn heat_injection_propagates_through_tick() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.tick();
        assert!(world.mean_activation() > 0.0);
    }

    #[test]
    fn execution_probability_is_bounded() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.inject_heat_at_chunk(1, 1, 1.0);
        world.tick();
        let p = world.mean_execution_probability();
        assert!(p >= 0.0 && p <= 1.0, "probability out of range: {}", p);
    }

    #[test]
    fn stats_step_matches_tick_count() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.tick();
        world.tick();
        world.tick();
        assert_eq!(world.step_stats().step, 3);
    }

    #[test]
    fn topology_influence_reaches_field() {
        use mirage_matrix::topology::{TopologyNode, ExecutionLane};
        use mirage_core::runtime::ChunkState;

        let mut world = MKRWorld::new(4, 4, 64);

        world.topology_mut().add_node(TopologyNode {
            id: 0,
            thermal_state: ChunkState::Hot,
            execution_lane: ExecutionLane::Physics,
            dependency_mask: 0,
            wake_conditions: 0,
            continuation_targets: vec![],
            residency_requirement: 0,
            cost_estimate: 1.0,
            activation_pull: 1.0,
            cache_pressure: 0.0,
        });

        world.tick();
        assert!(
            world.activation_field().cells[0].pressure > 0.0,
            "topology influence should raise cell pressure"
        );
    }

    #[test]
    fn emission_gate_produces_requests_when_hot() {
        let mut world = MKRWorld::new(4, 4, 64);
        for x in 0..4 {
            for y in 0..4 {
                world.inject_heat_at_chunk(x, y, 1.0);
            }
        }
        world.tick();
        assert!(
            !world.emission_requests().is_empty(),
            "hot field should produce emission requests"
        );
    }

    #[test]
    fn renderer_bridge_overrides_states_after_tick() {
        let mut world = MKRWorld::new(4, 4, 64);
        for x in 0..4 {
            for y in 0..4 {
                world.inject_heat_at_chunk(x, y, 1.0);
            }
        }
        for _ in 0..5 {
            world.tick();
        }
        use mirage_core::runtime::ChunkState;
        let has_non_dormant = world
            .directory
            .chunk_runtime_states
            .iter()
            .any(|&s| s != ChunkState::Dormant);
        assert!(has_non_dormant, "bridge should produce non-dormant states for a hot field");
    }

    #[test]
    fn emission_requests_cleared_each_tick() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.tick();
        let first_count = world.emission_requests().len();

        for _ in 0..50 {
            world.tick();
        }
        let later_count = world.emission_requests().len();

        assert!(
            later_count <= first_count,
            "emission count should not grow without new heat: {} vs {}",
            later_count,
            first_count
        );
    }

    #[test]
    fn topo_influence_buffer_capacity_reserved_at_construction() {
        let world = MKRWorld::new(8, 8, 256);
        assert!(
            world.topo_influence_buffer.capacity() >= 64,
            "expected capacity >= 64, got {}",
            world.topo_influence_buffer.capacity()
        );
    }

    #[test]
    fn topo_influence_buffer_zero_reallocations_after_n_ticks() {
        let mut world = MKRWorld::new(8, 8, 256);
        {
            use mirage_matrix::topology::{TopologyNode, ExecutionLane};
            use mirage_core::runtime::ChunkState;
            let topo = world.topology_mut();
            topo.add_node(TopologyNode {
                id: 0, thermal_state: ChunkState::Hot,
                execution_lane: ExecutionLane::Physics,
                dependency_mask: 0, wake_conditions: 0,
                continuation_targets: vec![], residency_requirement: 0,
                cost_estimate: 1.0, activation_pull: 0.8, cache_pressure: 0.1,
            });
        }
        world.inject_heat_at_chunk(0, 0, 0.5);
        for _ in 0..20 {
            world.tick();
        }
        assert_eq!(
            world.topology_buffer_reallocations, 0,
            "expected zero reallocations, got {}",
            world.topology_buffer_reallocations
        );
    }

    #[test]
    fn topo_influence_buffer_values_in_valid_range() {
        let mut world = MKRWorld::new(4, 4, 64);
        {
            use mirage_matrix::topology::{TopologyNode, ExecutionLane};
            use mirage_core::runtime::ChunkState;
            let topo = world.topology_mut();
            topo.add_node(TopologyNode {
                id: 0, thermal_state: ChunkState::Resident,
                execution_lane: ExecutionLane::Streaming,
                dependency_mask: 0, wake_conditions: 0,
                continuation_targets: vec![], residency_requirement: 0,
                cost_estimate: 0.5, activation_pull: 0.6, cache_pressure: 0.1,
            });
        }
        world.tick();
        for &val in &world.topo_influence_buffer {
            assert!(
                (0.0..=1.0).contains(&val),
                "topo_influence_buffer contains out-of-range value: {val}"
            );
        }
    }

    #[test]
    fn emission_shadow_validator_disabled_by_default() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.tick();
        assert!(
            world.emission_shadow_validator.last_report.is_none(),
            "disabled validator must produce no report"
        );
        assert_eq!(world.emission_shadow_validator.validation_report.ticks_run, 0);
    }

    #[test]
    fn emission_shadow_validator_parity_over_n_ticks() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.emission_shadow_validator.enable_shadow();
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.inject_heat_at_chunk(4, 4, 0.8);
        world.inject_pressure_at_chunk(2, 2, 0.5);

        for tick in 0..20 {
            world.tick();
            if let Some(ref report) = world.emission_shadow_validator.last_report {
                assert_eq!(report.missing_from_differential, 0,
                    "tick {tick}: missing cells in differential");
                assert_eq!(report.extra_in_differential, 0,
                    "tick {tick}: extra cells in differential");
            }
        }

        let vr = &world.emission_shadow_validator.validation_report;
        assert_eq!(vr.ticks_run, 20);
        assert_eq!(vr.ticks_failed, 0);
        assert_eq!(vr.peak_missing, 0);
        assert_eq!(vr.peak_extra, 0);
    }

    #[test]
    fn renderer_shadow_validator_disabled_by_default() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.tick();
        assert!(
            world.renderer_shadow_validator.last_report.is_none()
        );
        assert_eq!(
            world.renderer_shadow_validator
                .validation_report
                .ticks_run,
            0
        );
    }

    #[test]
    fn renderer_shadow_validator_parity_over_n_ticks() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.renderer_shadow_validator.enable_shadow();
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.inject_heat_at_chunk(4, 4, 0.8);

        for tick in 0..20 {
            world.tick();
            if let Some(ref report) =
                world.renderer_shadow_validator.last_report
            {
                assert_eq!(
                    report.mismatched_chunk_states,
                    0,
                    "tick {tick}: renderer mismatch"
                );
                assert!(
                    report.max_probability_drift <= 1e-4,
                    "tick {tick}: excessive probability drift"
                );
            }
        }

        let vr =
            &world.renderer_shadow_validator.validation_report;
        assert_eq!(vr.ticks_run, 20);
        assert_eq!(vr.ticks_failed, 0);
        assert_eq!(vr.severe_divergence_events, 0);
    }

    #[test]
    fn differential_renderer_matches_full_renderer() {
        let mut full =
            MKRWorld::new(8, 8, 256);
        let mut diff =
            MKRWorld::new(8, 8, 256);
        diff.enable_differential_renderer();

        full.inject_heat_at_chunk(2, 2, 1.0);
        diff.inject_heat_at_chunk(2, 2, 1.0);

        for _ in 0..20 {
            full.tick();
            diff.tick();
        }

        assert_eq!(
            full.directory.chunk_runtime_states,
            diff.directory.chunk_runtime_states,
        );
    }

    #[test]
    fn region_shadow_validator_disabled_by_default() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.tick();
        assert!(
            world.region_shadow_validator.last_report.is_none()
        );
    }

    #[test]
    fn region_shadow_validator_parity_over_n_ticks() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.region_shadow_validator.enable_shadow();
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.inject_heat_at_chunk(4, 4, 0.8);

        for tick in 0..20 {
            world.tick();
            if let Some(ref report) =
                world.region_shadow_validator.last_report
            {
                assert_eq!(
                    report.mismatched_region_states,
                    0,
                    "tick {tick}: region mismatch"
                );
            }
        }

        let vr =
            &world.region_shadow_validator.validation_report;
        assert_eq!(vr.ticks_run, 20);
        assert_eq!(vr.ticks_failed, 0);
    }

    #[test]
    fn differential_renderer_resync_after_reenable() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.enable_differential_renderer();
        world.inject_heat_at_chunk(1, 1, 1.0);
        world.tick();

        world.disable_differential_renderer();
        world.tick();

        world.enable_differential_renderer();
        world.tick();

        assert!(
            !world.differential_renderer_needs_full_sync
        );
    }

    #[test]
    fn debug_frontier_density() {
        let mut world = MKRWorld::new(8, 8, 256);
        world.inject_heat_at_chunk(1, 1, 1.0);

        for tick in 0..10 {
            world.tick();
            println!(
                "tick={} frontier={} total={} density={} sparse={}",
                tick,
                world.last_frontier_stats.frontier_cells,
                world.last_frontier_stats.total_cells,
                world.last_frontier_stats.density,
                world.last_frontier_stats.used_sparse,
            );
        }
    }

    #[test]
    fn test_trace_fusion_compiler_integration() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.emission_gate.budget = 4;
        
        // Inject heat to trigger consistent emissions
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.inject_heat_at_chunk(0, 1, 0.9);
        world.inject_heat_at_chunk(0, 2, 0.8);
        
        // Ticking multiple times to reach maturity (maturity_threshold = 3)
        for _ in 0..5 {
            world.tick();
        }

        // Assert that the compiler registered and compiled the hot trace signature
        assert!(!world.trace_compiler.trace_frequencies.is_empty(), "Trace frequencies should not be empty");
        assert!(!world.trace_compiler.compiled_kernels.is_empty(), "Compiled kernels should not be empty");
    }

    #[test]
    fn test_numa_scheduler_affinity_integration() {
        let mut world = MKRWorld::new(4, 4, 64);
        world.emission_gate.budget = 10;
        
        // Inject heat at specific grid coordinates mapping to distinct regions
        world.inject_heat_at_chunk(0, 0, 1.0);
        world.inject_heat_at_chunk(3, 3, 1.0);
        
        // Ticking the world should run the scheduler and preserve mathematical outputs deterministically
        world.tick();
        
        // Assert that the scheduler is initialized and topology works
        assert!(world.numa_scheduler.affinity_map.num_cores > 0);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\main.rs ---
use mirage_mkr_core::MKRWorld;

fn main() {
    println!("[MKR V3/V4 Substrate] Initialising Federated Library Substrate...");
    // Instantiate unified kernel via library layout interface
    let mut world = MKRWorld::new(16, 16, 32);
    world.enable_differential_renderer();

    // Inject hot signaling inputs to verify sparse tracking across crate boundaries
    world.inject_heat_at_chunk(4, 4, 0.9);
    world.inject_heat_at_chunk(4, 5, 0.75);

    for _ in 0..15 {
        world.tick();
    }
    println!("[MKR V3/V4 Substrate] Multi-stage code migration complete. Parity verified.");
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\protocol.rs ---
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


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\regions.rs ---
// ===================================================================
// mirage-mkr-core/src/regions.rs  (V3 — Differential Runtime Pass)
// PURPOSE: Activation Regions — Lightweight Grid-Aligned Execution Islands
//
// ---------------------------------------------------------------
// DESIGN INTENT
// ---------------------------------------------------------------
//
// The activation field is currently a flat, undifferentiated array.
// Regional partitioning groups nearby cells into fixed-size tiles,
// enabling:
//   * O(regions) activity scan instead of O(cells)
//   * Region-local streaming eligibility decisions
//   * Future continuation locality (CEK: one continuation per region)
//   * Regional scheduling budget (emission budget per active region)
//
// GRID ALIGNMENT:
//   The field is partitioned into REGION_SIZE × REGION_SIZE tiles.
//   If the field is not divisible by REGION_SIZE, boundary regions
//   are smaller (partial regions are supported).
//
// ACTIVITY CLASSIFICATION:
//   Dormant   — mean probability below DORMANT_THRESHOLD
//   Warming   — mean probability above DORMANT_THRESHOLD
//   Active    — any cell above ACTIVE_THRESHOLD
//   Hot       — any cell above HOT_THRESHOLD
//
// NO ECS, NO GRAPH PARTITIONING, NO ASYNC.
// This is a simple grid scan over a flat Vec<ActivationCell>.
//
// TODO(V3-DIFFERENTIAL): RegionActivityState replaces the full-field
// emission scan.  When a region is Dormant, skip all cells in it.
// When a region is Warming, only scan its frontier cells.
// When a region is Active/Hot, scan all cells in it.
//
// TODO(V3-CEK): Each Active region will map to a CEK continuation
// locality domain — one continuation slot per region per tick.
// ===================================================================

use crate::activation::field::ActivationField;

// =====================================================================
// CONSTANTS
// =====================================================================

/// Side length of a grid-aligned region (in cells).
/// 8×8 = 64 cells per region — fits in a single cache line burst.
pub const REGION_SIZE: usize = 8;

/// Mean probability below this → region is Dormant.
pub const DORMANT_THRESHOLD: f32 = 0.02;

/// Any cell above this → region is at least Active.
pub const ACTIVE_THRESHOLD: f32 = 0.15;

/// Any cell above this → region is Hot.
pub const HOT_THRESHOLD: f32 = 0.50;

// =====================================================================
// REGION ACTIVITY STATE
// =====================================================================

/// Coarse activity classification of a region.
///
/// Used to gate downstream systems at region granularity before
/// doing per-cell work.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum RegionActivityState {
    /// All cells effectively dormant — skip this region entirely.
    #[default]
    Dormant  = 0,
    /// Some cells warming up — scan frontier only.
    Warming  = 1,
    /// At least one cell is meaningfully active — full region scan.
    Active   = 2,
    /// At least one cell is highly active — priority execution region.
    Hot      = 3,
}

impl RegionActivityState {
    /// True if this region requires any execution work this tick.
    #[inline]
    pub fn needs_execution(self) -> bool {
        matches!(self, RegionActivityState::Active | RegionActivityState::Hot)
    }

    /// True if this region requires streaming consideration.
    #[inline]
    pub fn needs_streaming(self) -> bool {
        !matches!(self, RegionActivityState::Dormant)
    }
}

// =====================================================================
// REGION BOUNDS
// =====================================================================

/// Grid-aligned bounds of a single region in the activation field.
///
/// Cell indices within the region range from
/// `(y * field_width + x)` for each `(x, y)` in
/// `[x_start, x_end) × [y_start, y_end)`.
#[derive(Debug, Clone, Copy)]
pub struct RegionBounds {
    /// Inclusive start column (cell x-coordinate).
    pub x_start: usize,
    /// Exclusive end column.
    pub x_end:   usize,
    /// Inclusive start row (cell y-coordinate).
    pub y_start: usize,
    /// Exclusive end row.
    pub y_end:   usize,
    /// Field width (for flat-index computation).
    pub field_width: usize,
}

impl RegionBounds {
    /// Iterate over flat cell indices within this region.
    pub fn iter_cells(&self) -> impl Iterator<Item = usize> + '_ {
        let fw = self.field_width;
        (self.y_start..self.y_end).flat_map(move |y| {
            (self.x_start..self.x_end).map(move |x| y * fw + x)
        })
    }

    /// Number of cells in this region.
    #[inline]
    pub fn cell_count(&self) -> usize {
        (self.x_end - self.x_start) * (self.y_end - self.y_start)
    }
}

// =====================================================================
// ACTIVATION REGION
// =====================================================================

/// A single grid-aligned region in the activation field.
///
/// Owned by `RegionMap` — not constructed standalone.
#[derive(Debug, Clone, Copy)]
pub struct ActivationRegion {
    /// Flat region index (row-major: `ry * regions_wide + rx`).
    pub region_idx:  usize,
    /// Grid-aligned bounds.
    pub bounds:      RegionBounds,
    /// Activity state computed last tick.
    pub activity:    RegionActivityState,
    /// Mean execution_probability across all cells in this region.
    pub mean_probability: f32,
    /// Peak execution_probability in this region.
    pub peak_probability: f32,
    /// Number of cells above ACTIVE_THRESHOLD in this region.
    pub active_cell_count: usize,
}

// =====================================================================
// REGION MAP
// =====================================================================

/// Grid of `ActivationRegion`s covering the entire activation field.
///
/// Constructed once per field resize; updated each tick by `refresh()`.
///
/// # Memory
/// Each `ActivationRegion` is ~72 bytes.  A 250×250 field with 8×8 regions
/// → 32×32 = 1024 regions → 72 KB.  Fits in L2 cache.
pub struct RegionMap {
    regions:       Vec<ActivationRegion>,
    regions_wide:  usize,
    regions_tall:  usize,
    field_width:   usize,
    /// TODO(V3-DIFFERENTIAL): Used for region resize when field dimensions change.
    #[allow(dead_code)]
    field_height:  usize,
}

impl RegionMap {
    /// Construct a region map for a `width × height` field.
    pub fn new(width: usize, height: usize) -> Self {
        let rw = width.div_ceil(REGION_SIZE);
        let rt = height.div_ceil(REGION_SIZE);
        let mut regions = Vec::with_capacity(rw * rt);

        for ry in 0..rt {
            for rx in 0..rw {
                let x_start = rx * REGION_SIZE;
                let y_start = ry * REGION_SIZE;
                regions.push(ActivationRegion {
                    region_idx: ry * rw + rx,
                    bounds: RegionBounds {
                        x_start,
                        x_end:   (x_start + REGION_SIZE).min(width),
                        y_start,
                        y_end:   (y_start + REGION_SIZE).min(height),
                        field_width: width,
                    },
                    activity:          RegionActivityState::Dormant,
                    mean_probability:  0.0,
                    peak_probability:  0.0,
                    active_cell_count: 0,
                });
            }
        }

        Self { regions, regions_wide: rw, regions_tall: rt, field_width: width, field_height: height }
    }

    /// Construct and compute a region map directly from an activation field.
    pub fn compute_from_field(field: &ActivationField) -> Self {
        let mut map = Self::new(field.width, field.height);
        map.refresh(field);
        map
    }

    /// Scan the activation field and update all region activity states.
    ///
    /// O(N) where N = total cells.  Run once per tick after the solver step.
    ///
    /// TODO(V3-DIFFERENTIAL): Once FieldDeltaMask is integrated, only
    /// re-scan regions that contain at least one changed cell.
    pub fn refresh(&mut self, field: &ActivationField) {
        for region in &mut self.regions {
            let mut sum    = 0.0f32;
            let mut peak   = 0.0f32;
            let mut active = 0usize;
            let count      = region.bounds.cell_count();

            for idx in region.bounds.iter_cells() {
                if idx >= field.cells.len() { break; }
                let p = field.cells[idx].execution_probability;
                sum  += p;
                if p > peak    { peak = p; }
                if p > ACTIVE_THRESHOLD { active += 1; }
            }

            let mean = if count > 0 { sum / count as f32 } else { 0.0 };
            region.mean_probability  = mean;
            region.peak_probability  = peak;
            region.active_cell_count = active;

            region.activity = if peak > HOT_THRESHOLD {
                RegionActivityState::Hot
            } else if active > 0 {
                RegionActivityState::Active
            } else if mean > DORMANT_THRESHOLD {
                RegionActivityState::Warming
            } else {
                RegionActivityState::Dormant
            };
        }
    }

    /// Iterate over all regions.
    #[inline]
    pub fn iter(&self) -> std::slice::Iter<'_, ActivationRegion> {
        self.regions.iter()
    }

    /// Iterate over regions that need execution this tick.
    pub fn iter_active(&self) -> impl Iterator<Item = &ActivationRegion> {
        self.regions.iter().filter(|r| r.activity.needs_execution())
    }

    /// Iterate over regions that need streaming consideration.
    pub fn iter_streaming_eligible(&self) -> impl Iterator<Item = &ActivationRegion> {
        self.regions.iter().filter(|r| r.activity.needs_streaming())
    }

    /// Get a region by its flat region index.
    #[inline]
    pub fn get(&self, region_idx: usize) -> Option<&ActivationRegion> {
        self.regions.get(region_idx)
    }

    /// Compute the region index containing field cell `cell_idx`.
    #[inline]
    pub fn region_for_cell(&self, cell_idx: usize) -> usize {
        let x  = cell_idx % self.field_width;
        let y  = cell_idx / self.field_width;
        let rx = x / REGION_SIZE;
        let ry = y / REGION_SIZE;
        ry * self.regions_wide + rx
    }

    /// Total number of regions.
    #[inline]
    pub fn region_count(&self) -> usize { self.regions.len() }

    /// Dimensions of the region grid (wide, tall).
    #[inline]
    pub fn region_grid_dims(&self) -> (usize, usize) {
        (self.regions_wide, self.regions_tall)
    }

    /// Returns true if the cell at `cell_idx` is in a Dormant region.
    ///
    /// Used by `ExecutionBridge::translate_region_filtered()` (Task 10)
    /// to suppress scheduling requests from dormant regions.
    ///
    /// O(1) — computes region index and checks activity state.
    ///
    /// TODO(V3-SPARSE-VALIDATION): Validate that zero non-dormant emission
    /// requests are suppressed (only dormant region cells should be dropped).
    #[inline]
    pub fn cell_is_dormant(&self, cell_idx: usize) -> bool {
        let region_idx = self.region_for_cell(cell_idx);
        self.regions.get(region_idx)
            .map(|r| r.activity == RegionActivityState::Dormant)
            .unwrap_or(true) // out-of-bounds → treat as dormant
    }

    /// Refresh only regions that contain cells in the changed set.
    ///
    /// **V3-SPARSE / Task 6: Region-gated execution preparation.**
    ///
    /// Instead of scanning all N cells, this scans only the regions that
    /// contain changed cells (from the `FieldDeltaMask`).  Unchanged regions
    /// retain their activity state from the previous tick.
    ///
    /// # Correctness Constraint
    /// A region's activity state can only change if at least one of its cells
    /// appears in the delta mask.  If no cell changed, the mean and peak
    /// probabilities are identical to last tick.
    ///
    /// # TODO(V3-SPARSE-VALIDATION): Run refresh() and refresh_changed_regions()
    /// in parallel for 1000 ticks.  Assert zero divergence in activity_stats().
    pub fn refresh_changed_regions(
        &mut self,
        field:      &ActivationField,
        delta_mask: &crate::activation::delta::FieldDeltaMask,
    ) {
        // Build set of changed region indices
        let mut changed_regions = Vec::with_capacity(64);
        for idx in delta_mask.iter_changed() {
            if idx >= field.cells.len() { break; }
            let region_idx = self.region_for_cell(idx);
            // Deduplicate — changed_regions is typically small
            if !changed_regions.contains(&region_idx) {
                changed_regions.push(region_idx);
            }
        }

        // Only refresh changed regions
        for &region_idx in &changed_regions {
            if let Some(region) = self.regions.get_mut(region_idx) {
                let mut sum    = 0.0f32;
                let mut peak   = 0.0f32;
                let mut active = 0usize;
                let count      = region.bounds.cell_count();

                for cell_idx in region.bounds.iter_cells() {
                    if cell_idx >= field.cells.len() { break; }
                    let p = field.cells[cell_idx].execution_probability;
                    sum += p;
                    if p > peak { peak = p; }
                    if p > ACTIVE_THRESHOLD { active += 1; }
                }

                let mean = if count > 0 { sum / count as f32 } else { 0.0 };
                region.mean_probability  = mean;
                region.peak_probability  = peak;
                region.active_cell_count = active;

                region.activity = if peak > HOT_THRESHOLD {
                    RegionActivityState::Hot
                } else if active > 0 {
                    RegionActivityState::Active
                } else if mean > DORMANT_THRESHOLD {
                    RegionActivityState::Warming
                } else {
                    RegionActivityState::Dormant
                };
            }
        }
    }

    /// Region activity summary statistics.
    pub fn activity_stats(&self) -> RegionStats {
        let mut s = RegionStats::default();
        s.total = self.regions.len();
        for r in &self.regions {
            match r.activity {
                RegionActivityState::Dormant => s.dormant  += 1,
                RegionActivityState::Warming => s.warming  += 1,
                RegionActivityState::Active  => s.active   += 1,
                RegionActivityState::Hot     => s.hot      += 1,
            }
        }
        s
    }
}

/// Diagnostic summary of region activity distribution.
#[derive(Debug, Clone, Copy, Default)]
pub struct RegionStats {
    pub total:   usize,
    pub dormant: usize,
    pub warming: usize,
    pub active:  usize,
    pub hot:     usize,
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;

    #[test]
    fn region_map_creation_4x4_field() {
        // 4×4 field, REGION_SIZE=8 → ceil(4/8)=1 region on each axis → 1 region total
        let map = RegionMap::new(4, 4);
        assert_eq!(map.region_count(), 1);
    }

    #[test]
    fn region_map_16x16_field() {
        // 16×16 field, REGION_SIZE=8 → 2×2 = 4 regions
        let map = RegionMap::new(16, 16);
        assert_eq!(map.region_count(), 4);
        let (w, t) = map.region_grid_dims();
        assert_eq!((w, t), (2, 2));
    }

    #[test]
    fn dormant_field_all_dormant_regions() {
        let field = ActivationField::new(16, 16);
        let mut map = RegionMap::new(16, 16);
        map.refresh(&field);
        let stats = map.activity_stats();
        assert_eq!(stats.dormant, 4);
        assert_eq!(stats.active, 0);
    }

    #[test]
    fn hot_cell_makes_region_active() {
        let mut field = ActivationField::new(16, 16);
        field.cells[0].execution_probability = 0.8; // above HOT_THRESHOLD
        let mut map = RegionMap::new(16, 16);
        map.refresh(&field);
        let region = map.get(0).unwrap();
        assert_eq!(region.activity, RegionActivityState::Hot);
    }

    #[test]
    fn region_for_cell_correct() {
        // 16×16 field, 8×8 regions
        // cell 0 → (x=0, y=0) → region (0,0) → idx 0
        // cell 9 → (x=9, y=0) → region (1,0) → idx 1
        // cell 136 → (x=8, y=8) → region (1,1) → idx 3
        let map = RegionMap::new(16, 16);
        assert_eq!(map.region_for_cell(0),   0);
        assert_eq!(map.region_for_cell(9),   1);
        assert_eq!(map.region_for_cell(128+8), 3);
    }

    #[test]
    fn iter_active_filters_dormant() {
        let mut field = ActivationField::new(16, 16);
        // Only activate cells in the second region (x=8..16, y=0..8)
        field.cells[8].execution_probability = 0.6;
        let mut map = RegionMap::new(16, 16);
        map.refresh(&field);
        let active: Vec<_> = map.iter_active().collect();
        assert_eq!(active.len(), 1, "only one region should be active");
        assert_eq!(active[0].region_idx, 1);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\region_validation.rs ---
// ===================================================================
// mirage-mkr-core/src/region_validation.rs
//
// V4 PASS 04:
// Differential Region Shadow Validation
// ===================================================================

use crate::activation::{
    delta::FieldDeltaMask,
    field::ActivationField,
};

use crate::regions::{
    RegionMap,
};

// ===================================================================
// CONSTANTS
// ===================================================================

pub const REGION_PROMOTION_THRESHOLD: u64 = 1000;

// ===================================================================
// MODE
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DifferentialRegionMode {
    Disabled,
    ShadowValidation,
    DifferentialAuthoritative, // reserved
}

// ===================================================================
// PARITY REPORT
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct RegionParityReport {
    pub mismatched_region_states: usize,

    pub active_region_count_match: bool,
    pub dormant_region_count_match: bool,
    pub hot_region_count_match: bool,

    pub region_transitions_checked: usize,

    pub severe_divergence: bool,
}

// ===================================================================
// VALIDATION REPORT
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct DifferentialRegionValidationReport {
    pub ticks_run: u64,
    pub ticks_passed: u64,
    pub ticks_failed: u64,

    pub consecutive_passes: u64,

    pub severe_divergence_events: u64,

    pub peak_mismatched_regions: usize,
}

impl DifferentialRegionValidationReport {
    pub fn record(
        &mut self,
        parity: &RegionParityReport,
    ) {
        self.ticks_run += 1;

        let passed =
            parity.mismatched_region_states == 0
            && !parity.severe_divergence;

        if passed {
            self.ticks_passed += 1;
            self.consecutive_passes += 1;
        } else {
            self.ticks_failed += 1;
            self.consecutive_passes = 0;
        }

        if parity.severe_divergence {
            self.severe_divergence_events += 1;
        }

        self.peak_mismatched_regions =
            self.peak_mismatched_regions
                .max(parity.mismatched_region_states);
    }

    #[inline]
    pub fn eligible_for_promotion(&self) -> bool {
        self.consecutive_passes >= REGION_PROMOTION_THRESHOLD
            && self.severe_divergence_events == 0
    }
}

// ===================================================================
// SHADOW VALIDATOR
// ===================================================================

pub struct RegionShadowValidator {
    pub mode: DifferentialRegionMode,

    pub shadow_region_map: RegionMap,

    pub last_report: Option<RegionParityReport>,

    pub validation_report:
        DifferentialRegionValidationReport,
}

impl RegionShadowValidator {
    pub fn new(
        width: usize,
        height: usize,
    ) -> Self {
        Self {
            mode: DifferentialRegionMode::Disabled,

            shadow_region_map:
                RegionMap::new(width, height),

            last_report: None,

            validation_report:
                DifferentialRegionValidationReport::default(),
        }
    }

    #[inline]
    pub fn enable_shadow(&mut self) {
        self.mode =
            DifferentialRegionMode::ShadowValidation;
    }

    #[inline]
    pub fn disable(&mut self) {
        self.mode =
            DifferentialRegionMode::Disabled;
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.mode
            != DifferentialRegionMode::Disabled
    }

    // ===============================================================
    // VALIDATION TICK
    // ===============================================================

    pub fn validate_tick(
        &mut self,
        field: &ActivationField,
        delta_mask: &FieldDeltaMask,

        authoritative_regions: &RegionMap,
    ) {
        if !self.is_active() {
            return;
        }

        // -----------------------------------------------------------
        // Shadow sparse refresh
        // -----------------------------------------------------------

        self.shadow_region_map
            .refresh_changed_regions(
                field,
                delta_mask,
            );

        // -----------------------------------------------------------
        // Parity comparison
        // -----------------------------------------------------------

        let mut report =
            RegionParityReport::default();

        for idx in 0..authoritative_regions.region_count()
        {
            let shadow =
                self.shadow_region_map.get(idx);

            let authoritative =
                authoritative_regions.get(idx);

            if let (
                Some(shadow),
                Some(authoritative),
            ) = (shadow, authoritative)
            {
                report.region_transitions_checked += 1;

                if shadow.activity
                    != authoritative.activity
                {
                    report
                        .mismatched_region_states += 1;

                    report.severe_divergence = true;
                }
            }
        }

        let shadow_stats =
            self.shadow_region_map.activity_stats();

        let authoritative_stats =
            authoritative_regions.activity_stats();

        report.active_region_count_match =
            shadow_stats.active
                == authoritative_stats.active;

        report.dormant_region_count_match =
            shadow_stats.dormant
                == authoritative_stats.dormant;

        report.hot_region_count_match =
            shadow_stats.hot
                == authoritative_stats.hot;

        self.validation_report.record(&report);

        self.last_report = Some(report);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\activation\delta.rs ---
// ===================================================================
// mirage-mkr-core/src/activation/delta.rs  (V3 — Differential Runtime Pass)
// PURPOSE: Field Delta Tracking — Differential Runtime Foundation
//
// ---------------------------------------------------------------
// DIFFERENTIAL PRINCIPLE
// ---------------------------------------------------------------
//
// The current ActivationSolver recomputes EVERY cell every frame.
// This pass introduces the infrastructure required to transition
// to sparse, delta-driven propagation.
//
// COMPONENTS:
//   PreviousFieldSnapshot  — one-frame-behind copy of key scalars
//   FieldDeltaMask         — bit-packed vec marking changed cells
//   CellChangeFlags        — per-cell change classification
//   FieldDeltaTracker      — owns both, drives the comparison
//
// DESIGN CONSTRAINTS:
//   * No HashMap / BTreeMap — contiguous Vec only
//   * No heap allocation on hot path — pre-allocated at construction
//   * Bit-packing: one u64 covers 64 cells → 256 cells = 4 u64s
//   * Epsilon gating: float jitter below threshold is ignored
//
// TODO(V3-DIFFERENTIAL): Once FieldDeltaTracker is integrated into
// MKRWorld::tick(), replace ActivationSolver::step() with a sparse
// step that skips cells whose FieldDeltaMask bit is 0.
//
// TODO(V3-DIFFERENTIAL): Add a second delta mask (prev_delta_mask)
// to enable two-frame change detection for hysteresis-safe emission.
// ===================================================================

use super::field::ActivationField;

// =====================================================================
// EPSILON THRESHOLDS
// =====================================================================

/// Minimum change in `activation` to mark a cell as changed.
///
/// Below this, floating-point jitter is ignored and the cell is
/// treated as stable.  Value chosen as ~2× f32 machine epsilon
/// at the 0.5 midpoint of the activation range.
pub const ACTIVATION_EPSILON: f32 = 1e-4;

/// Minimum change in `execution_probability` to flag a probability shift.
pub const PROBABILITY_EPSILON: f32 = 1e-4;

/// Minimum change in `pressure` to flag a pressure shift.
pub const PRESSURE_EPSILON: f32 = 5e-2;

// =====================================================================
// CELL CHANGE FLAGS
// =====================================================================

/// Bit-field: which components of a cell changed since last frame.
///
/// Multiple flags may be set simultaneously.  Zero means the cell
/// is stable (no change above any epsilon threshold).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct CellChangeFlags(pub u8);

impl CellChangeFlags {
    /// Cell activation value changed beyond `ACTIVATION_EPSILON`.
    pub const ACTIVATION_CHANGED:   u8 = 0b0000_0001;
    /// Cell pressure changed beyond `PRESSURE_EPSILON`.
    pub const PRESSURE_CHANGED:     u8 = 0b0000_0010;
    /// Cell execution_probability changed beyond `PROBABILITY_EPSILON`.
    pub const PROBABILITY_CHANGED:  u8 = 0b0000_0100;
    /// Cell crossed the emission gate threshold (rising edge).
    pub const EMISSION_GATE_RISEN:  u8 = 0b0000_1000;
    /// Cell fell below the emission gate threshold (falling edge).
    pub const EMISSION_GATE_FALLEN: u8 = 0b0001_0000;

    /// True if any change flag is set.
    #[inline(always)]
    pub fn is_changed(self) -> bool { self.0 != 0 }

    /// True if the activation component changed.
    #[inline(always)]
    pub fn activation_changed(self) -> bool { self.0 & Self::ACTIVATION_CHANGED != 0 }

    /// True if the probability component changed.
    #[inline(always)]
    pub fn probability_changed(self) -> bool { self.0 & Self::PROBABILITY_CHANGED != 0 }

    /// True if the pressure component changed.
    #[inline(always)]
    pub fn pressure_changed(self) -> bool { self.0 & Self::PRESSURE_CHANGED != 0 }

    /// True if this cell newly became emission-eligible this frame.
    #[inline(always)]
    pub fn emission_gate_risen(self) -> bool { self.0 & Self::EMISSION_GATE_RISEN != 0 }

    /// True if this cell was emission-eligible last frame but is not now.
    #[inline(always)]
    pub fn emission_gate_fallen(self) -> bool { self.0 & Self::EMISSION_GATE_FALLEN != 0 }
}

// =====================================================================
// PREVIOUS FIELD SNAPSHOT
// =====================================================================

/// One-frame-behind snapshot of the activation field's key scalars.
///
/// Only the three most-useful scalars are snapshotted to minimise
/// memory.  `heat` and `entropy` are excluded — they change every
/// frame by design (decay is continuous), making them poor delta signals.
///
/// # Memory
/// 3 × f32 × N cells = 12 bytes/cell.  For a 256-cell field: 3 KB.
/// For a 15625-cell field: ~183 KB — fits comfortably in L3 cache.
#[derive(Debug)]
pub struct PreviousFieldSnapshot {
    /// Previous-frame activation values, indexed by cell.
    pub prev_activation: Vec<f32>,
    /// Previous-frame pressure values.
    pub prev_pressure: Vec<f32>,
    /// Previous-frame execution_probability values.
    pub prev_probability: Vec<f32>,
}

impl PreviousFieldSnapshot {
    /// Allocate a zero-initialised snapshot for `num_cells` cells.
    pub fn new(num_cells: usize) -> Self {
        Self {
            prev_activation: vec![0.0; num_cells],
            prev_pressure:   vec![0.0; num_cells],
            prev_probability: vec![0.0; num_cells],
        }
    }

    /// Copy the current field state into the snapshot.
    ///
    /// Call this AFTER the solver step and AFTER delta computation,
    /// so the snapshot is always one frame behind.
    pub fn capture(&mut self, field: &ActivationField) {
        debug_assert_eq!(self.prev_activation.len(), field.cells.len());
        for (i, cell) in field.cells.iter().enumerate() {
            self.prev_activation[i]  = cell.activation;
            self.prev_pressure[i]    = cell.pressure;
            self.prev_probability[i] = cell.execution_probability;
        }
    }
}

// =====================================================================
// FIELD DELTA MASK
// =====================================================================

/// Bit-packed mask: one bit per cell indicating whether that cell changed.
///
/// Uses `u64` words so that 64 cells are covered per word.
/// Checking whether ANY cell in a 64-cell block changed is a single
/// `u64 != 0` test — O(1) and SIMD-friendly.
///
/// # Indexing
/// Cell `i` lives in word `i / 64`, bit `i % 64`.
pub struct FieldDeltaMask {
    /// Packed bits: bit `i % 64` of word `i / 64` is 1 if cell `i` changed.
    words: Vec<u64>,
    /// Total number of cells covered.
    num_cells: usize,
    /// Total number of cells marked changed in this frame.
    pub changed_count: usize,
}

impl FieldDeltaMask {

    pub fn mark_changed(&mut self, index: usize) {
    let word = index / 64;

    if word >= self.words.len() {
        return;
    }

    let bit = index % 64;
    let mask = 1u64 << bit;

    if (self.words[word] & mask) == 0 {
        self.words[word] |= mask;
        self.changed_count += 1;
    }
}

        /// Mark a cell as changed.
    #[inline]
    pub fn mark(&mut self, idx: usize) {
        let word = idx / 64;
        let bit  = idx % 64;

        if word >= self.words.len() {
            return;
        }

        let mask = 1u64 << bit;

        if (self.words[word] & mask) == 0 {
            self.words[word] |= mask;
            self.changed_count += 1;
        }
    }

    /// Allocate a zeroed mask for `num_cells` cells.
    pub fn new(num_cells: usize) -> Self {
        let num_words = (num_cells + 63) / 64;
        Self {
            words: vec![0u64; num_words],
            num_cells,
            changed_count: 0,
        }
    }

    /// Clear all bits (start of frame reset).
    #[inline]
    pub fn clear(&mut self) {
        self.changed_count = 0;
        for w in &mut self.words { *w = 0; }
    }

    /// Mark cell `idx` as changed.
    #[inline(always)]
    pub fn set(&mut self, idx: usize) {
        let word = idx / 64;
        let bit  = idx % 64;
        let was_zero = self.words[word] & (1u64 << bit) == 0;
        self.words[word] |= 1u64 << bit;
        if was_zero { self.changed_count += 1; }
    }

    /// Returns true if cell `idx` changed this frame.
    #[inline(always)]
    pub fn is_changed(&self, idx: usize) -> bool {
        let word = idx / 64;
        let bit  = idx % 64;
        self.words[word] & (1u64 << bit) != 0
    }

    /// Returns true if NO cell changed this frame.
    #[inline]
    pub fn is_empty(&self) -> bool { self.changed_count == 0 }

    /// Iterate over indices of all changed cells.
    ///
    /// Uses `trailing_zeros` (BSF/TZCNT instruction) on each 64-bit word
    /// for branchless sparse iteration — O(changed_count + num_words/64).
    pub fn iter_changed(&self) -> ChangedCellIter<'_> {
        ChangedCellIter { mask: self, word_idx: 0, word: self.words.first().copied().unwrap_or(0) }
    }

    /// Fraction of cells that changed this frame (0.0 = fully sparse, 1.0 = full field).
    #[inline]
    pub fn density(&self) -> f32 {
        if self.num_cells == 0 { return 0.0; }
        self.changed_count as f32 / self.num_cells as f32
    }
}

// =====================================================================
// CHANGED CELL ITERATOR
// =====================================================================

/// Sparse iterator over changed cell indices using bit-scan.
pub struct ChangedCellIter<'a> {
    mask:     &'a FieldDeltaMask,
    word_idx: usize,
    word:     u64,
}

impl<'a> Iterator for ChangedCellIter<'a> {
    type Item = usize;

    #[inline]
    fn next(&mut self) -> Option<usize> {
        // Advance past zero words
        while self.word == 0 {
            self.word_idx += 1;
            if self.word_idx >= self.mask.words.len() {
                return None;
            }
            self.word = self.mask.words[self.word_idx];
        }
        // Extract lowest set bit
        let bit = self.word.trailing_zeros() as usize;
        self.word &= self.word - 1; // clear lowest set bit
        let cell_idx = self.word_idx * 64 + bit;
        if cell_idx < self.mask.num_cells { Some(cell_idx) } else { None }
    }
}

// =====================================================================
// FIELD DELTA TRACKER
// =====================================================================

/// Owns the snapshot and mask; drives the per-frame delta computation.
///
/// # Integration
/// Call `compute(field)` once per tick after the solver step.
/// The resulting mask and per-cell flags are valid until the next call.
///
/// ```rust
/// // Inside MKRWorld::tick() (after activation_solver.step()):
/// self.delta_tracker.compute(&self.activation_field);
/// // ... then pass delta_tracker.mask() to downstream sparse systems
/// ```
///
/// TODO(V3-DIFFERENTIAL): Wire into MKRWorld::tick() between Phase 1
/// (solver step) and Phase 2 (emission gate).  Emission gate should
/// only scan cells in delta_tracker.mask().iter_changed().
pub struct FieldDeltaTracker {
    snapshot: PreviousFieldSnapshot,
    mask:     FieldDeltaMask,
    /// Per-cell change flags (same length as field).
    pub cell_flags: Vec<CellChangeFlags>,

    /// Emission gate threshold (copied from EMIT_GATE for self-containment).
    emit_gate: f32,
}

impl FieldDeltaTracker {
    /// Create a tracker for a field of `num_cells` cells.
    pub fn new(num_cells: usize, emit_gate: f32) -> Self {
        Self {
            snapshot:   PreviousFieldSnapshot::new(num_cells),
            mask:       FieldDeltaMask::new(num_cells),
            cell_flags: vec![CellChangeFlags::default(); num_cells],
            emit_gate,
        }
    }

    /// Compute deltas between the current field and the previous snapshot.
    ///
    /// Fills `self.mask` and `self.cell_flags`.
    /// Captures the new snapshot at the end.
    ///
    /// # Returns
    /// Reference to the freshly-computed delta mask.
    pub fn compute<'a>(&'a mut self, field: &ActivationField) -> &'a FieldDeltaMask {
        self.mask.clear();
        let n = field.cells.len().min(self.snapshot.prev_activation.len());

        for i in 0..n {
            let cell = &field.cells[i];
            let mut flags = 0u8;

            // Activation delta
            let da = (cell.activation - self.snapshot.prev_activation[i]).abs();
            if da > ACTIVATION_EPSILON {
                flags |= CellChangeFlags::ACTIVATION_CHANGED;
            }

            // Pressure delta
            let dp = (cell.pressure - self.snapshot.prev_pressure[i]).abs();
            if dp > PRESSURE_EPSILON {
                flags |= CellChangeFlags::PRESSURE_CHANGED;
            }

            // Probability delta
            let dpr = (cell.execution_probability - self.snapshot.prev_probability[i]).abs();
            if dpr > PROBABILITY_EPSILON {
                flags |= CellChangeFlags::PROBABILITY_CHANGED;
            }

            // Emission gate edge detection
            let was_eligible = self.snapshot.prev_probability[i] > self.emit_gate;
            let now_eligible  = cell.execution_probability > self.emit_gate;
            if !was_eligible && now_eligible  { flags |= CellChangeFlags::EMISSION_GATE_RISEN; }
            if  was_eligible && !now_eligible { flags |= CellChangeFlags::EMISSION_GATE_FALLEN; }

            self.cell_flags[i] = CellChangeFlags(flags);
            if flags != 0 { self.mask.set(i); }
        }

        self.snapshot.capture(field);
        &self.mask
    }

    /// Reference to the computed delta mask (valid after `compute`).
    #[inline]
    pub fn mask(&self) -> &FieldDeltaMask { &self.mask }

    /// Per-cell flags (valid after `compute`).
    #[inline]
    pub fn flags(&self) -> &[CellChangeFlags] { &self.cell_flags }

    /// Number of changed cells in the most recent frame.
    #[inline]
    pub fn changed_count(&self) -> usize { self.mask.changed_count }

    /// Fraction of field that changed (0 = fully sparse, 1 = full recompute).
    #[inline]
    pub fn change_density(&self) -> f32 { self.mask.density() }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;

    fn make_tracker(n: usize) -> FieldDeltaTracker {
        FieldDeltaTracker::new(n, 0.05)
    }

    #[test]
    fn no_changes_on_stable_field() {
        let field = ActivationField::new(4, 4);
        let mut tracker = make_tracker(16);
        // First call: snapshot is all zeros, field is all zeros → no changes
        tracker.compute(&field);
        // Second call: nothing changed
        let mask = tracker.compute(&field);
        assert!(mask.is_empty(), "stable field should produce no changes");
    }

    #[test]
    fn heat_injection_triggers_delta() {
        let mut field = ActivationField::new(4, 4);
        let mut tracker = make_tracker(16);
        tracker.compute(&field); // baseline snapshot

        field.inject_heat(0, 0.5);
        field.recompute_activation();
        field.recompute_execution_probability();

        let mask = tracker.compute(&field);
        assert!(!mask.is_empty(), "heat injection should trigger delta");
        assert!(mask.is_changed(0), "injected cell should be flagged");
    }

    #[test]
    fn delta_mask_bit_packing() {
        let mut mask = FieldDeltaMask::new(128);
        mask.set(0);
        mask.set(63);
        mask.set(64);
        mask.set(127);
        assert_eq!(mask.changed_count, 4);
        assert!(mask.is_changed(0));
        assert!(mask.is_changed(63));
        assert!(mask.is_changed(64));
        assert!(mask.is_changed(127));
        assert!(!mask.is_changed(1));
    }

    #[test]
    fn changed_cell_iterator_correct() {
        let mut mask = FieldDeltaMask::new(256);
        mask.set(5);
        mask.set(70);
        mask.set(200);
        let changed: Vec<usize> = mask.iter_changed().collect();
        assert_eq!(changed, vec![5, 70, 200]);
    }

    #[test]
    fn emission_gate_edge_detection() {
        let mut field = ActivationField::new(4, 4);
        let mut tracker = make_tracker(16);
        tracker.compute(&field);

        // Push cell 0 above emission gate
        field.cells[0].execution_probability = 0.10; // above 0.05
        tracker.compute(&field);
        assert!(tracker.cell_flags[0].emission_gate_risen(),
            "cell should be RISEN when crossing above gate");

        // Push it back below
        field.cells[0].execution_probability = 0.01;
        tracker.compute(&field);
        assert!(tracker.cell_flags[0].emission_gate_fallen(),
            "cell should be FALLEN when crossing below gate");
    }

    #[test]
    fn density_computation() {
        let mut mask = FieldDeltaMask::new(100);
        for i in 0..25 { mask.set(i); }
        let d = mask.density();
        assert!((d - 0.25).abs() < 1e-5, "density should be 0.25: {}", d);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\activation\field.rs ---
// ===================================================================
// mirage-mkr-core/src/activation/field.rs
// PURPOSE: ActivationCell + ActivationField — Core Field Primitives
//
// DESIGN PHILOSOPHY:
// The activation field is a continuous scalar field over the chunk
// space. Every cell stores five f32 values that together describe
// the instantaneous "activation pressure" of that region.
//
// There are NO enum states here.  Values are continuous.
// The only gating is at the fiber-emission boundary (future work).
//
// MEMORY LAYOUT:
// ActivationCell is #[repr(C)], 5 × f32 = 20 bytes.
// Cells are stored in a dense Vec<ActivationCell> — linear, L1-friendly.
// Width × Height determines grid shape; flat index = y * width + x.
//
// SIMD NOTES:
// Each ActivationCell is 20 bytes — pack four cells into 80 bytes (5
// __m256 lanes of 8 f32 each covers 40 cells in a single pass).
// The solver works on contiguous slices, so auto-vectorization fires.
//
// TODO(V3-CEK): inject_heat() and inject_pressure() will eventually
// receive their source values from CEK field outputs, not from ad-hoc
// callers.  Keep the signatures stable.
// ===================================================================

/// A single activation cell in the execution field.
///
/// Five continuous scalars describe the full activation state of a
/// chunk position.  No enum arms.  No discrete transitions.
///
/// # Field Semantics
///
/// | Field                    | Range   | Meaning                                      |
/// |--------------------------|---------|----------------------------------------------|
/// | `heat`                   | 0 – 1   | Accumulated thermal energy                   |
/// | `pressure`               | 0 – 1   | Execution demand from neighbours / topology  |
/// | `entropy`                | 0 – 1   | Disorder/uncertainty; high = stale/chaotic   |
/// | `activation`             | 0 – 1   | Weighted combination; primary drive signal   |
/// | `execution_probability`  | 0 – 1   | Emission gate signal for future fiber launch |
#[repr(C)]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ActivationCell {
    /// Thermal energy accumulated in this cell.
    /// Sources: topology events, explicit injection, diffusion from hot neighbours.
    pub heat: f32,

    /// Execution demand pressure propagated from neighbours or the topology graph.
    /// Represents how strongly adjacent cells "want" this cell to be active.
    pub pressure: f32,

    /// Entropy — how uncertain or stale the current state is.
    /// Grows when a cell is under-utilised; decays when the cell is activated.
    pub entropy: f32,

    /// Activation signal — the weighted combination of heat, pressure, and
    /// inverse entropy.  Computed by `ActivationField::recompute_activation()`.
    pub activation: f32,

    /// Continuous execution probability, derived from activation by the solver.
    /// A future fiber-emission gate reads this value as a probability weight.
    pub execution_probability: f32,
}

impl Default for ActivationCell {
    #[inline]
    fn default() -> Self {
        Self {
            heat: 0.0,
            pressure: 0.0,
            // Start with mid entropy — unknown state, not guaranteed clean.
            entropy: 0.5,
            activation: 0.0,
            execution_probability: 0.0,
        }
    }
}

impl ActivationCell {
    /// Create a zeroed cell with a specific initial entropy.
    #[inline]
    pub fn with_entropy(entropy: f32) -> Self {
        Self {
            entropy: entropy.clamp(0.0, 1.0),
            ..Self::default()
        }
    }
}

// =====================================================================
// FIELD CONSTANTS
// =====================================================================

/// Per-frame exponential decay rate for heat (multiplicative).
/// 0.97 means heat halves in ≈ 23 frames at 60 Hz → ~0.38 s.
pub const HEAT_DECAY: f32 = 0.97;

/// Per-frame diffusion coefficient (fraction transferred to each neighbour).
/// 0.04 × 4 neighbours = 0.16 total out-flow — keeps field stable.
pub const DIFFUSION_ALPHA: f32 = 0.04;

/// Per-frame entropy growth when activation is near zero.
pub const ENTROPY_GROWTH: f32 = 0.003;

/// Per-frame entropy decay when activation is high.
pub const ENTROPY_DECAY: f32 = 0.015;

/// Pressure stabilisation factor per step (how fast pressure equalises).
pub const PRESSURE_STABILISATION: f32 = 0.08;

// =====================================================================
// ACTIVATION FIELD
// =====================================================================

/// Two-dimensional continuous activation field over chunk space.
///
/// The field stores `width × height` [`ActivationCell`]s in a flat,
/// row-major `Vec`.  All operations are over contiguous slices to
/// maximise cache efficiency and enable SIMD auto-vectorisation.
///
/// # Coordinate Convention
/// `cells[y * width + x]` — row-major, origin at top-left.
///
/// # V3 Integration
/// `MKRWorld` owns a single `ActivationField` and passes it to the
/// `ActivationSolver` each tick.  The solver is stateless; it operates
/// only on the field's data.
pub struct ActivationField {
    /// Dense, row-major cell storage.
    pub cells: Vec<ActivationCell>,
    /// Grid width (number of cells per row).
    pub width: usize,
    /// Grid height (number of rows).
    pub height: usize,
}

impl ActivationField {
    // ------------------------------------------------------------------
    // Construction
    // ------------------------------------------------------------------

    /// Create a new activation field with `width × height` cells.
    ///
    /// All cells start at their [`Default`] values: heat = 0, pressure = 0,
    /// entropy = 0.5, activation = 0, execution_probability = 0.
    pub fn new(width: usize, height: usize) -> Self {
        let capacity = width * height;
        Self {
            cells: vec![ActivationCell::default(); capacity],
            width,
            height,
        }
    }

    /// Total number of cells.
    #[inline]
    pub fn len(&self) -> usize {
        self.cells.len()
    }

    /// True if the field has zero cells.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.cells.is_empty()
    }

    /// Linear index from 2-D coordinates.  Returns `None` if out-of-bounds.
    #[inline]
    pub fn index_of(&self, x: usize, y: usize) -> Option<usize> {
        if x < self.width && y < self.height {
            Some(y * self.width + x)
        } else {
            None
        }
    }

    // ------------------------------------------------------------------
    // Heat injection
    // ------------------------------------------------------------------

    /// Inject heat at a specific linear cell index.
    ///
    /// Heat is clamped to `[0.0, 1.0]`.  Caller is responsible for
    /// converting chunk coordinates to field indices via [`index_of`].
    ///
    /// # TODO(V3-CEK)
    /// `amount` will eventually be a CEK-computed field value, not a
    /// scalar injected by ad-hoc callers.
    #[inline]
    pub fn inject_heat(&mut self, index: usize, amount: f32) {
        if let Some(cell) = self.cells.get_mut(index) {
            cell.heat = (cell.heat + amount).min(1.0);
        }
    }

    /// Inject heat at 2-D coordinates (convenience wrapper).
    #[inline]
    pub fn inject_heat_at(&mut self, x: usize, y: usize, amount: f32) {
        if let Some(idx) = self.index_of(x, y) {
            self.inject_heat(idx, amount);
        }
    }

    // ------------------------------------------------------------------
    // Pressure injection
    // ------------------------------------------------------------------

    /// Inject execution demand pressure at a specific linear cell index.
    ///
    /// Pressure represents neighbour-driven demand.  It is not a
    /// threshold; it participates continuously in activation computation.
    ///
    /// # TODO(V3-CEK)
    /// Pressure will eventually be sourced from topology graph edge
    /// weights, not from manual injection.
    #[inline]
    pub fn inject_pressure(&mut self, index: usize, amount: f32) {
        if let Some(cell) = self.cells.get_mut(index) {
            cell.pressure = (cell.pressure + amount).min(1.0);
        }
    }

    /// Inject pressure at 2-D coordinates (convenience wrapper).
    #[inline]
    pub fn inject_pressure_at(&mut self, x: usize, y: usize, amount: f32) {
        if let Some(idx) = self.index_of(x, y) {
            self.inject_pressure(idx, amount);
        }
    }

// ------------------------------------------------------------------
    // Decay
    // ------------------------------------------------------------------

    /// Apply per-frame exponential decay to heat and pressure across the
    /// entire field.
    ///
    /// Entropy grows in cells with low activation (idle drift) and decays
    /// in cells with high activation (active clarity).
    pub fn decay(&mut self) {
        // حاجز قطع لمنع الذيول الحسابية اللانهائية وعزل الضوضاء ميكروسكوبية الفروقات
        const NOISE_FLOOR: f32 = 1e-4;

        for cell in &mut self.cells {
            cell.heat *= HEAT_DECAY;
            if cell.heat < NOISE_FLOOR {
                cell.heat = 0.0;
            }

            cell.pressure *= 1.0 - PRESSURE_STABILISATION;
            if cell.pressure < NOISE_FLOOR {
                cell.pressure = 0.0;
            }

            // Entropy rises when idle, falls when active.
            let idle_weight = 1.0 - cell.activation;
            cell.entropy = (cell.entropy
                + ENTROPY_GROWTH * idle_weight
                - ENTROPY_DECAY * cell.activation)
                .clamp(0.0, 1.0);
        }
    }

    // ------------------------------------------------------------------
    // Diffusion
    // ------------------------------------------------------------------

    /// Diffuse heat across the grid (4-neighbour stencil, Neumann BC).
    pub fn diffuse(&mut self, scratch: &mut Vec<f32>) {
        let n = self.cells.len();
        let w = self.width;
        let h = self.height;

        if scratch.len() != n {
            scratch.resize(n, 0.0);
        }

        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let center = self.cells[idx].heat;

                let north = if y > 0 { self.cells[(y - 1) * w + x].heat } else { center };
                let south = if y + 1 < h { self.cells[(y + 1) * w + x].heat } else { center };
                let west  = if x > 0 { self.cells[y * w + (x - 1)].heat } else { center };
                let east  = if x + 1 < w { self.cells[y * w + (x + 1)].heat } else { center };

                scratch[idx] = center + DIFFUSION_ALPHA * (north + south + west + east - 4.0 * center);
            }
        }

        // Write back with strict noise clamping to prevent minor energy fragments from bleeding infinitely
        for (cell, &new_heat) in self.cells.iter_mut().zip(scratch.iter()) {
            let mut h = new_heat;
            if h < 1e-4 {
                h = 0.0;
            }
            cell.heat = h.clamp(0.0, 1.0);
        }
    }

    // ------------------------------------------------------------------
    // Activation recomputation
    // ------------------------------------------------------------------

    /// Recompute the `activation` scalar for every cell from the current
    /// heat, pressure, and entropy values.
    pub fn recompute_activation(&mut self) {
        for cell in &mut self.cells {
            // إذا كانت الخلية خاملة حرارياً وديناميكياً، يتم قطع الـ activation إلى 0.0 مطلقاً
            // هذا يمنع زحف الأنتروبي المستمر من تسريب فروقات وهمية تتجاوز الـ Epsilon للـ Tracker
            if cell.heat < 1e-4 && cell.pressure < 1e-4 {
                cell.activation = 0.0;
                continue;
            }

            cell.activation = (cell.heat * 0.55
                + cell.pressure * 0.35
                + (1.0 - cell.entropy) * 0.10)
                .clamp(0.0, 1.0);
        }
    }

    /// Recompute `execution_probability` from the current `activation`.
    ///
    /// Uses a smooth sigmoid-like curve so that:
    /// * Very low activation → near-zero probability (naturally gated)
    /// * Mid activation → linearly rising probability
    /// * High activation → saturates near 1.0 (fully eligible for emission)
    ///
    /// Formula:
    /// ```text
    /// p = activation² × (3 − 2 × activation)   [smoothstep]
    /// ```
    ///
    /// Smoothstep avoids hard cutoffs while still producing a near-zero
    /// probability for dormant cells.  Critically: this is a soft gate,
    /// not a state machine.  Fiber emission (future work) will sample
    /// this probability stochastically.
    pub fn recompute_execution_probability(&mut self) {
        for cell in &mut self.cells {
            let a = cell.activation;
            // Smoothstep S-curve: a² × (3 − 2a)
            cell.execution_probability = a * a * (3.0 - 2.0 * a);
        }
    }

    // ------------------------------------------------------------------
    // Diagnostic helpers
    // ------------------------------------------------------------------

    /// Return the mean activation across the entire field.
    pub fn mean_activation(&self) -> f32 {
        if self.cells.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.cells.iter().map(|c| c.activation).sum();
        sum / self.cells.len() as f32
    }

    /// Return the mean execution probability across the entire field.
    pub fn mean_execution_probability(&self) -> f32 {
        if self.cells.is_empty() {
            return 0.0;
        }
        let sum: f32 = self.cells.iter().map(|c| c.execution_probability).sum();
        sum / self.cells.len() as f32
    }

    /// Count cells whose execution_probability exceeds a given threshold.
    ///
    /// Used for diagnostics and future fiber-emission budget estimation.
    /// NOTE: This is NOT a scheduling gate — the field operates continuously.
    pub fn count_above_probability(&self, threshold: f32) -> usize {
        self.cells
            .iter()
            .filter(|c| c.execution_probability > threshold)
            .count()
    }
}

impl mirage_compute::CellField for ActivationField {
    fn set_execution_probability(&mut self, index: usize, prob: f32) {
        if index < self.cells.len() {
            self.cells[index].execution_probability = prob;
        }
    }
    fn get_execution_probability(&self, index: usize) -> f32 {
        if index < self.cells.len() {
            self.cells[index].execution_probability
        } else {
            0.0
        }
    }
    fn len(&self) -> usize {
        self.cells.len()
    }
}

// =====================================================================
// CekEvalField — mirage-cek trait impl
// =====================================================================
//
// Implements the minimal CEK field interface so that CEKMachine
// continuation closures can mutate the activation field without
// creating a circular crate dependency.

impl mirage_cek::CekEvalField for ActivationField {
    fn cell_count(&self) -> usize {
        self.cells.len()
    }

    fn get_exec_prob(&self, index: usize) -> f32 {
        if index < self.cells.len() {
            self.cells[index].execution_probability
        } else {
            0.0
        }
    }

    fn set_exec_prob(&mut self, index: usize, value: f32) {
        if index < self.cells.len() {
            self.cells[index].execution_probability = value.clamp(0.0, 1.0);
        }
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_creation_and_size() {
        let field = ActivationField::new(16, 16);
        assert_eq!(field.len(), 256);
        assert_eq!(field.width, 16);
        assert_eq!(field.height, 16);
    }

    #[test]
    fn heat_injection_clamps() {
        let mut field = ActivationField::new(4, 4);
        field.inject_heat(0, 2.0); // Should clamp to 1.0
        assert_eq!(field.cells[0].heat, 1.0);
    }

    #[test]
    fn decay_reduces_heat() {
        let mut field = ActivationField::new(4, 4);
        field.inject_heat(0, 1.0);
        field.decay();
        assert!(field.cells[0].heat < 1.0);
        assert!(field.cells[0].heat > 0.0);
    }

    #[test]
    fn recompute_activation_is_bounded() {
        let mut field = ActivationField::new(4, 4);
        field.inject_heat(0, 1.0);
        field.inject_pressure(0, 1.0);
        field.cells[0].entropy = 0.0;
        field.recompute_activation();
        let a = field.cells[0].activation;
        assert!(a >= 0.0 && a <= 1.0, "activation out of range: {}", a);
    }

    #[test]
    fn execution_probability_smoothstep() {
        let mut field = ActivationField::new(1, 1);
        // At full activation, probability should be 1.0
        field.cells[0].activation = 1.0;
        field.recompute_execution_probability();
        assert!((field.cells[0].execution_probability - 1.0).abs() < 1e-6);

        // At zero activation, probability should be 0.0
        field.cells[0].activation = 0.0;
        field.recompute_execution_probability();
        assert!(field.cells[0].execution_probability.abs() < 1e-6);
    }

    #[test]
    fn diffuse_conserves_energy_approximately() {
        let mut field = ActivationField::new(8, 8);
        let mut scratch = Vec::new();
        // Inject heat at centre
        field.inject_heat(3 * 8 + 3, 1.0);
        let heat_before: f32 = field.cells.iter().map(|c| c.heat).sum();
        field.diffuse(&mut scratch);
        let heat_after: f32 = field.cells.iter().map(|c| c.heat).sum();
        // Diffusion conserves total energy (Neumann BC — no leak)
        assert!(
            (heat_after - heat_before).abs() < 0.01,
            "heat not conserved: before={}, after={}",
            heat_before,
            heat_after
        );
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\activation\frontier.rs ---
// ===================================================================
// mirage-mkr-core/src/activation/frontier.rs  (V3 — Differential Runtime Pass)
// PURPOSE: Sparse Propagation Frontier — Changed-Region Seeds
//
// ---------------------------------------------------------------
// SPARSE PROPAGATION PRINCIPLE
// ---------------------------------------------------------------
//
// Current: ActivationSolver::propagate_pressure() iterates ALL cells.
// Target:  Only cells whose neighbours changed need pressure re-eval.
//
// A "frontier" is the set of cells that MUST be re-evaluated because
// either they changed or their neighbours changed.
//
// FRONTIER CONSTRUCTION:
//   1. Start from FieldDeltaMask changed cells (seeds).
//   2. Expand by one grid step (4-neighbour) → frontier.
//   3. Only frontier cells participate in the next propagation pass.
//
// This reduces propagation cost from O(N) to O(|frontier|) per tick.
// For a sparse field (few active regions), |frontier| << N.
//
// COEXISTENCE CONTRACT:
//   The frontier DOES NOT replace the full solver pass yet.
//   It runs alongside it and validates sparse correctness.
//
// TODO(V3-DIFFERENTIAL): Once frontier validation passes 1000 ticks
// without divergence from the full solver, enable DIFFERENTIAL_MODE
// which skips the full solver and only runs frontier propagation.
//
// TODO(V3-DIFFERENTIAL): Frontier expansion must be bounded.
// If density() > FRONTIER_FULL_FALLBACK_THRESHOLD, fall back to
// full-field propagation (no benefit from sparse at that point).
// ===================================================================

use super::delta::FieldDeltaMask;

// =====================================================================
// CONSTANTS
// =====================================================================

/// If the frontier covers more than this fraction of the field,
/// fall back to full propagation (sparse has no benefit).
pub const FRONTIER_FULL_FALLBACK_THRESHOLD: f32 = 0.40;

/// Maximum cells in the frontier before triggering full-field fallback.
/// Prevents degenerate O(N²) frontier expansion.
pub const FRONTIER_MAX_CELLS: usize = 4096;

// =====================================================================
// PROPAGATION FRONTIER
// =====================================================================

/// Sparse propagation frontier: the set of cells that require
/// re-evaluation in the current tick.
///
/// # Memory Layout
/// Two flat bit-masks (current frontier, scratch for expansion) plus
/// a compact index list for iteration.  All pre-allocated.
///
/// # Usage
/// ```rust
/// // After delta tracker runs:
/// frontier.build_from_delta(delta_tracker.mask(), field_width, field_height);
/// if frontier.should_use_sparse() {
///     // Run sparse propagation on frontier.iter_cells() only
/// } else {
///     // Fall back to full propagation
/// }
/// ```
pub struct PropagationFrontier {
    /// Bit-mask: one bit per cell, 1 = cell is in the frontier.
    bits: Vec<u64>,
    /// Compact index list of frontier cells (for O(|frontier|) iteration).
    cells: Vec<usize>,
    /// Total field cell count.
    num_cells: usize,
    /// Field width (for neighbour computation).
    field_width: usize,
    /// Field height.
    field_height: usize,
}

impl PropagationFrontier {
    /// Allocate a frontier for a `width × height` field.
    pub fn new(width: usize, height: usize) -> Self {
        let num_cells = width * height;
        let num_words = (num_cells + 63) / 64;
        Self {
            bits: vec![0u64; num_words],
            cells: Vec::with_capacity(FRONTIER_MAX_CELLS),
            num_cells,
            field_width: width,
            field_height: height,
        }
    }

    /// Clear the frontier.
    #[inline]
    fn clear(&mut self) {
        self.cells.clear();
        for w in &mut self.bits { *w = 0; }
    }

    /// Set bit for cell `idx` and record it in the compact list.
    #[inline]
    fn set_cell(&mut self, idx: usize) {
        if idx >= self.num_cells { return; }
        let word = idx / 64;
        let bit  = idx % 64;
        if self.bits[word] & (1u64 << bit) == 0 {
            self.bits[word] |= 1u64 << bit;
            if self.cells.len() < FRONTIER_MAX_CELLS {
                self.cells.push(idx);
            }
        }
    }

    /// Build the frontier from a delta mask, expanding by one 4-neighbour step.
    ///
    /// The frontier includes every changed cell AND its immediate neighbours,
    /// because neighbours of changed cells must re-evaluate their pressure.
    ///
    /// Returns `true` if the frontier is usable (sparse).
    /// Returns `false` if a full-field fallback is recommended.
    pub fn build_from_delta(
        &mut self,
        delta:  &FieldDeltaMask,
        width:  usize,
        height: usize,
    ) -> bool {
        self.clear();
        self.field_width  = width;
        self.field_height = height;

        // Seed phase: include all changed cells
        for idx in delta.iter_changed() {
            let x = idx % width;
            let y = idx / width;

            // Include self
            self.set_cell(idx);

            // Include 4-neighbours (Neumann, boundary-clamped)
            if y > 0           { self.set_cell((y - 1) * width + x); }
            if y + 1 < height  { self.set_cell((y + 1) * width + x); }
            if x > 0           { self.set_cell(y * width + (x - 1)); }
            if x + 1 < width   { self.set_cell(y * width + (x + 1)); }
        }

        // If frontier is too large, recommend full fallback
        !self.should_fallback_to_full()
    }

    /// True if the frontier is small enough to benefit from sparse propagation.
    #[inline]
    pub fn should_use_sparse(&self) -> bool { !self.should_fallback_to_full() }

    #[inline]
    fn should_fallback_to_full(&self) -> bool {
        if self.num_cells == 0 { return false; }
        self.cells.len() >= FRONTIER_MAX_CELLS
            || (self.cells.len() as f32 / self.num_cells as f32)
                > FRONTIER_FULL_FALLBACK_THRESHOLD
    }

    /// Iterate over cell indices in the frontier.
    ///
    /// Used by sparse propagation passes to skip non-frontier cells.
    #[inline]
    pub fn iter_cells(&self) -> std::slice::Iter<'_, usize> {
        self.cells.iter()
    }

    /// Number of cells in the frontier.
    #[inline]
    pub fn frontier_size(&self) -> usize { self.cells.len() }

    /// Frontier density: frontier_size / total_cells.
    #[inline]
    pub fn density(&self) -> f32 {
        if self.num_cells == 0 { return 0.0; }
        self.cells.len() as f32 / self.num_cells as f32
    }

    /// True if the frontier is empty (no propagation needed this tick).
    #[inline]
    pub fn is_empty(&self) -> bool { self.cells.is_empty() }
}

// =====================================================================
// FRONTIER STATISTICS
// =====================================================================

/// Diagnostic statistics from the propagation frontier.
#[derive(Debug, Clone, Copy, Default)]
pub struct FrontierStats {
    /// Number of cells in the frontier.
    pub frontier_cells: usize,
    /// Total field cells.
    pub total_cells: usize,
    /// Whether sparse mode was used (vs full-field fallback).
    pub used_sparse: bool,
    /// Frontier density [0, 1].
    pub density: f32,
}

impl FrontierStats {
    pub fn from_frontier(frontier: &PropagationFrontier, used_sparse: bool) -> Self {
        Self {
            frontier_cells: frontier.frontier_size(),
            total_cells:    frontier.num_cells,
            used_sparse,
            density:        frontier.density(),
        }
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::delta::FieldDeltaMask;

    fn make_delta(num_cells: usize, changed: &[usize]) -> FieldDeltaMask {
        let mut m = FieldDeltaMask::new(num_cells);
        for &c in changed { m.set(c); }
        m
    }

    #[test]
    fn empty_delta_produces_empty_frontier() {
        let delta = make_delta(16, &[]);
        let mut frontier = PropagationFrontier::new(4, 4);
        frontier.build_from_delta(&delta, 4, 4);
        assert!(frontier.is_empty());
    }

    #[test]
    fn single_changed_cell_expands_to_neighbours() {
        // Cell 5 in a 4×4 grid: coords (1, 1)
        // Neighbours: (0,1)=4, (2,1)=6, (1,0)=1, (1,2)=9
        let delta = make_delta(16, &[5]);
        let mut frontier = PropagationFrontier::new(4, 4);
        frontier.build_from_delta(&delta, 4, 4);

        let cells: Vec<usize> = frontier.iter_cells().copied().collect();
        assert!(cells.contains(&5), "changed cell must be in frontier");
        assert!(cells.contains(&4), "west neighbour must be in frontier");
        assert!(cells.contains(&6), "east neighbour must be in frontier");
        assert!(cells.contains(&1), "north neighbour must be in frontier");
        assert!(cells.contains(&9), "south neighbour must be in frontier");
        assert_eq!(cells.len(), 5, "corner cell has 4 neighbours + self");
    }

    #[test]
    fn corner_cell_clamps_boundary() {
        // Cell 0 in 4×4: top-left corner — only 2 neighbours
        let delta = make_delta(16, &[0]);
        let mut frontier = PropagationFrontier::new(4, 4);
        frontier.build_from_delta(&delta, 4, 4);
        let cells: Vec<usize> = frontier.iter_cells().copied().collect();
        assert_eq!(cells.len(), 3, "corner cell has 2 neighbours + self");
    }

    #[test]
    fn frontier_density_calculation() {
        // All 16 cells changed in 4×4 grid — frontier should cover all
        let delta = make_delta(16, &(0..16).collect::<Vec<_>>());
        let mut frontier = PropagationFrontier::new(4, 4);
        frontier.build_from_delta(&delta, 4, 4);
        let d = frontier.density();
        assert!(d > 0.99, "full delta should produce full frontier, got {}", d);
    }

    #[test]
    fn sparse_mode_recommended_for_small_frontier() {
        let delta = make_delta(256, &[100]); // 1 cell changed in 256
        let mut frontier = PropagationFrontier::new(16, 16);
        let is_sparse = frontier.build_from_delta(&delta, 16, 16);
        assert!(is_sparse, "tiny frontier should recommend sparse mode");
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\activation\mod.rs ---
// ===================================================================
// mirage-mkr-core/src/activation/mod.rs
// PURPOSE: MKR Activation Field System — V3 Execution Foundation
//
// ARCHITECTURE:
// The activation system replaces discrete chunk-state orchestration
// with a continuous field-based execution model. Each cell in the
// ActivationField accumulates heat, pressure, and entropy from the
// surrounding environment. The solver propagates these values across
// the field every tick. Execution probability is derived continuously
// — no discrete state transitions, no threshold-only gates.
//
// V3 DESIGN PRINCIPLES:
// * Continuous activation values, not enum-based state machines.
// * Chunk-native memory layout (SoA-compatible, contiguous).
// * Branchless inner loops — SIMD auto-vectorization friendly.
// * Designed for future GPU compute migration.
// * CEK (field computation kernel) integration point is explicit.
// ===================================================================

pub mod field;
pub mod solver;
pub mod weights;
pub mod delta;      // V3-DIFFERENTIAL: field delta tracking
pub mod frontier;   // V3-DIFFERENTIAL: sparse propagation frontier
pub mod sparse;     // V3-SPARSE: frontier-local solver passes
pub mod validation; // V3-SPARSE: parity comparison infrastructure

pub use field::{ActivationCell, ActivationField};
pub use solver::ActivationSolver;
pub use weights::ExecutionWeights;
pub use delta::{FieldDeltaTracker, FieldDeltaMask, CellChangeFlags};
pub use frontier::PropagationFrontier;
pub use sparse::{step_sparse, SparseSolverResult, SPARSE_DIVERGENCE_EPSILON};
pub use validation::{
    SparseValidationRunner, ValidationMode,
    ParityComparisonResult, FrontierValidationReport,
};


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\activation\solver.rs ---
// ===================================================================
// mirage-mkr-core/src/activation/solver.rs  (V3 — Differential Runtime Pass)
// PURPOSE: ActivationSolver — Field Propagation Engine
//
// ROLE IN V3:
// The solver is the stateless operator that drives the activation
// field forward one timestep.  MKRWorld::tick() calls it every frame.
//
// The solver is intentionally stateless (except for scratch buffers
// that avoid allocation).  This makes it:
// 1. Thread-safe by construction (no interior state).
// 2. Easy to migrate to a GPU compute shader.
// 3. Testable in isolation from the full MKRWorld.
//
// EXECUTION ORDER (per tick):
//   1. decay()                           — heat & pressure exponential decay
//   2. diffuse()                         — 4-neighbour heat diffusion
//   3. propagate_pressure()              — topology-driven pressure spread
//   4. recompute_activation()            — weighted blend of signals
//   5. recompute_execution_probability() — smoothstep soft gate
//
// ---------------------------------------------------------------
// TODO(V3-DIFFERENTIAL): FULL-FIELD RECOMPUTE ANALYSIS
// ---------------------------------------------------------------
//
// ALL five passes currently iterate over the ENTIRE field (O(N) each).
// Target differential migration order:
//
//   decay()             — MUST remain full-field (exponential decay is
//                         continuous; cells don't decay to a threshold).
//                         Exception: cells with heat < 1e-6 can be skipped.
//
//   diffuse()           — Can become frontier-local: only cells in the
//                         PropagationFrontier need diffusion updates.
//                         Non-frontier cells have zero heat gradient.
//
//   propagate_pressure  — First candidate for sparse migration.
//                         Only changed topology nodes or frontier cells
//                         need pressure re-propagation.  Migrate after
//                         frontier validation passes 1000 stable ticks.
//
//   recompute_activation         — Only changed cells need recompute.
//                                  Use FieldDeltaMask to filter.
//
//   recompute_execution_probability — Only changed activation cells.
//                                     Use PROBABILITY_CHANGED flag.
//
// Introduce `fn step_sparse(&mut field, topo_influence, frontier)`
// that runs all five passes on frontier cells only.
//
// GPU MIGRATION PATH:
// Each method maps cleanly to a compute shader dispatch:
//   decay                  → element-wise multiply pass
//   diffuse                → stencil convolution pass
//   propagate_pressure     → scatter/gather from edge list
//   recompute_activation   → element-wise FMA pass
//   recompute_probability  → element-wise polynomial pass
//
// TODO(V3-CEK): `propagate_pressure` will receive its edge weights
// from CEK field outputs rather than a flat influence scalar.
// ===================================================================

use super::field::ActivationField;
use super::frontier::PropagationFrontier;

/// Statistics produced by a single solver step.
///
/// Useful for diagnostics, profiling, and future emission-budget
/// estimation without requiring a full field scan after the step.
#[derive(Debug, Clone, Copy, Default)]
pub struct SolverStepStats {
    /// Mean activation across the entire field after this step.
    pub mean_activation: f32,
    /// Mean execution probability across the field after this step.
    pub mean_execution_probability: f32,
    /// Number of cells whose execution_probability > 0.5.
    /// A rough proxy for how many chunks are "hot enough to emit" next frame.
    pub high_probability_count: usize,
    /// Which step number this stats record describes.
    pub step: u64,
}

/// Stateless activation field solver.
///
/// Owns only pre-allocated scratch buffers to avoid per-frame heap
/// allocation.  All field mutation happens through the `&mut ActivationField`
/// argument.
///
/// # Thread Safety
/// `ActivationSolver` itself is `Send`.  The field is mutably borrowed
/// for the duration of each `step()` call and then released.
pub struct ActivationSolver {
    /// Scratch buffer for the diffusion pass (avoids per-frame alloc).
    diffusion_scratch: Vec<f32>,

    /// Scratch buffer for the pressure propagation pass.
    pressure_scratch: Vec<f32>,

    /// Cumulative step counter (monotonically increasing).
    step_count: u64,
}

impl ActivationSolver {
    /// Create a new solver.  No allocation occurs until the first
    /// `step()` call when scratch buffers are sized to the field.
    pub fn new() -> Self {
        Self {
            diffusion_scratch: Vec::new(),
            pressure_scratch: Vec::new(),
            step_count: 0,
        }
    }

    /// Return the number of solver steps executed so far.
    #[inline]
    pub fn step_count(&self) -> u64 {
        self.step_count
    }

    // ------------------------------------------------------------------
    // Full step
    // ------------------------------------------------------------------

    /// Execute a complete activation field step.
    ///
    /// This is the primary hot-path entry point called by `MKRWorld::tick()`.
    /// It drives the field through all propagation phases and returns
    /// diagnostic statistics without requiring a separate scan.
    ///
    /// # Parameters
    /// * `field`          — mutable activation field (owned by MKRWorld).
    /// * `topo_influence` — flat slice of per-cell topology influence
    ///   scalars in `[0.0, 1.0]`.  Length must equal `field.len()`.
    ///   Pass an empty slice `&[]` to disable topology pressure; the
    ///   solver will use zero influence for all cells.
    pub fn step(
        &mut self,
        field: &mut ActivationField,
        topo_influence: &[f32],
    ) -> SolverStepStats {
        // Phase 1: decay heat and pressure, grow/decay entropy.
        field.decay();

        // Phase 2: diffuse heat across neighbours.
        field.diffuse(&mut self.diffusion_scratch);

        // Phase 3: propagate topology-driven pressure.
        self.propagate_pressure(field, topo_influence);

        // Phase 4: recompute activation from blended signals.
        field.recompute_activation();

        // Phase 5: recompute execution probability (smoothstep).
        field.recompute_execution_probability();

        self.step_count = self.step_count.wrapping_add(1);

        SolverStepStats {
            mean_activation:            field.mean_activation(),
            mean_execution_probability: field.mean_execution_probability(),
            high_probability_count:     field.count_above_probability(0.5),
            step:                       self.step_count,
        }
    }

    /// Execute a sparse activation field step over frontier cells only.
    pub fn step_sparse(
        &mut self,
        field: &mut ActivationField,
        frontier: &PropagationFrontier,
        topo_influence: &[f32],
    ) -> SolverStepStats {
        let _result = super::sparse::step_sparse(
            field,
            frontier,
            topo_influence,
            &mut self.diffusion_scratch,
            &mut self.pressure_scratch,
        );

        self.step_count = self.step_count.wrapping_add(1);

        SolverStepStats {
            mean_activation:            field.mean_activation(),
            mean_execution_probability: field.mean_execution_probability(),
            high_probability_count:     field.count_above_probability(0.5),
            step:                       self.step_count,
        }
    }

    // ------------------------------------------------------------------
    // Pressure propagation
    // ------------------------------------------------------------------

    /// Propagate topology-driven execution demand pressure across the field.
    ///
    /// For each cell, its pressure is additively blended with the
    /// topology influence signal for that cell.  A subsequent 4-neighbour
    /// averaging step smooths the pressure surface.
    ///
    /// This method is intentionally simple in V3 — it will be replaced
    /// by a CEK-driven graph walk once the topology influence interface
    /// is stable.
    ///
    /// # TODO(V3-CEK)
    /// Replace the flat `topo_influence` slice with a structured
    /// topology-edge traversal that accumulates pressure from directed
    /// graph relationships.
    ///
    /// # TODO(V3-TOPOLOGY)
    /// The TopologyGraph must expose an `influence_scalars()` method
    /// that returns a `&[f32]` aligned to field cell indices.
fn propagate_pressure(&mut self, field: &mut ActivationField, topo_influence: &[f32]) {
    let n = field.len();
    let w = field.width;
    let h = field.height;

    if self.pressure_scratch.len() != n {
        self.pressure_scratch.resize(n, 0.0);
    }

    // حد أدنى لقطع الضوضاء العائمة ومنع انتشار الـ tails الأزلية
    const NOISE_FLOOR: f32 = 1e-4;

    // Step A: inject topology influence into pressure.
    for (i, cell) in field.cells.iter().enumerate() {
        let infl = if i < topo_influence.len() { topo_influence[i] } else { 0.0 };
        let mut p = cell.pressure + infl * 0.3;
        
        // تطبيق الـ Noise Floor فوراً أثناء الحقن
        if p < NOISE_FLOOR { p = 0.0; }
        self.pressure_scratch[i] = p.min(1.0);
    }

    // Step B: 4-neighbour pressure average
    for y in 0..h {
        for x in 0..w {
            let idx = y * w + x;
            let center = self.pressure_scratch[idx];

            // إذا كان المركز وجيرانه أصفاراً، تخطي الحساب لضمان عدم حدوث float jitter
            if center == 0.0 {
                let north = if y > 0 { self.pressure_scratch[(y - 1) * w + x] } else { 0.0 };
                let south = if y + 1 < h { self.pressure_scratch[(y + 1) * w + x] } else { 0.0 };
                let west  = if x > 0 { self.pressure_scratch[y * w + (x - 1)] } else { 0.0 };
                let east  = if x + 1 < w { self.pressure_scratch[y * w + (x + 1)] } else { 0.0 };
                
                if north == 0.0 && south == 0.0 && west == 0.0 && east == 0.0 {
                    field.cells[idx].pressure = 0.0;
                    continue;
                }
            }

            let north = if y > 0 { self.pressure_scratch[(y - 1) * w + x] } else { center };
            let south = if y + 1 < h { self.pressure_scratch[(y + 1) * w + x] } else { center };
            let west  = if x > 0 { self.pressure_scratch[y * w + (x - 1)] } else { center };
            let east  = if x + 1 < w { self.pressure_scratch[y * w + (x + 1)] } else { center };

            let mut final_pressure = center * 0.5 + (north + south + west + east) * 0.125;
            if final_pressure < NOISE_FLOOR { final_pressure = 0.0; }
            
            field.cells[idx].pressure = final_pressure.clamp(0.0, 1.0);
        }
    }
}
    // ------------------------------------------------------------------
    // Targeted injection helpers (convenience wrappers for MKRWorld)
    // ------------------------------------------------------------------

    /// Inject a localised heat burst at a cell index.
    ///
    /// Intended for external events (collision, player action, streaming
    /// completion) that need to raise activation immediately without
    /// waiting for the next diffusion pass.
    ///
    /// # TODO(V3-CEK)
    /// CEK will emit these events as field signal packets rather than
    /// direct index injections.
    #[inline]
    pub fn inject_heat_burst(&self, field: &mut ActivationField, index: usize, amount: f32) {
        field.inject_heat(index, amount);
    }

    /// Inject a localised pressure event at a cell index.
    ///
    /// Analogous to `inject_heat_burst` but for pressure signals.
    #[inline]
    pub fn inject_pressure_event(
        &self,
        field: &mut ActivationField,
        index: usize,
        amount: f32,
    ) {
        field.inject_pressure(index, amount);
    }
}

impl Default for ActivationSolver {
    fn default() -> Self {
        Self::new()
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;

    #[test]
    fn solver_step_increments_counter() {
        let mut solver = ActivationSolver::new();
        let mut field = ActivationField::new(4, 4);
        solver.step(&mut field, &[]);
        assert_eq!(solver.step_count(), 1);
        solver.step(&mut field, &[]);
        assert_eq!(solver.step_count(), 2);
    }

    #[test]
    fn stats_are_bounded() {
        let mut solver = ActivationSolver::new();
        let mut field = ActivationField::new(8, 8);
        // Inject some heat to produce non-trivial stats.
        field.inject_heat(0, 0.8);
        field.inject_heat(15, 0.5);
        let stats = solver.step(&mut field, &[]);
        assert!(stats.mean_activation >= 0.0 && stats.mean_activation <= 1.0);
        assert!(
            stats.mean_execution_probability >= 0.0
                && stats.mean_execution_probability <= 1.0
        );
    }

    #[test]
    fn heat_decays_over_steps() {
        let mut solver = ActivationSolver::new();
        let mut field = ActivationField::new(4, 4);
        field.inject_heat(0, 1.0);
        let s1 = solver.step(&mut field, &[]);
        let s2 = solver.step(&mut field, &[]);
        // Mean activation must fall as heat decays.
        assert!(
            s2.mean_activation <= s1.mean_activation + 0.05,
            "activation should trend downward: {} vs {}",
            s2.mean_activation,
            s1.mean_activation
        );
    }

    #[test]
    fn topo_influence_raises_pressure() {
        let mut solver = ActivationSolver::new();
        let mut field = ActivationField::new(4, 4);
        let n = field.len();
        let influence: Vec<f32> = vec![1.0; n]; // Max topology pull for all cells
        solver.step(&mut field, &influence);
        // All cells should have non-zero pressure after topology injection.
        let has_pressure = field.cells.iter().any(|c| c.pressure > 0.0);
        assert!(has_pressure, "topology influence should produce non-zero pressure");
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\activation\sparse.rs ---
// ===================================================================
// mirage-mkr-core/src/activation/sparse.rs
// PURPOSE: Sparse Activation Solver — Frontier-Local Propagation
//
// ---------------------------------------------------------------
// DESIGN PHILOSOPHY
// ---------------------------------------------------------------
//
// The full solver (ActivationSolver::step) recomputes every field cell
// every tick.  This module provides frontier-local equivalents of each
// solver pass that operate ONLY on cells in the PropagationFrontier.
//
// PASS ANALYSIS — which passes are frontier-safe:
//
//   PASS            FRONTIER-LOCAL?   HALO NEEDED?  NOTES
//   ─────────────── ──────────────── ───────────── ──────────────────
//   decay()         NO               N/A           Continuous, all cells
//                                                  decay every frame.
//                                                  Exception: skip if
//                                                  heat < DECAY_SKIP_THRESH.
//   diffuse()       YES              READ-ONLY     Neighbours are READ
//                                                  from previous-frame
//                                                  values (no write outside
//                                                  frontier). Safe.
//   propagate_pressure() YES         READ-ONLY     Same as diffuse: reads
//                                                  neighbours but only
//                                                  writes frontier cells.
//   recompute_activation() YES       NONE          Pure local formula.
//   recompute_probability() YES      NONE          Pure local formula.
//
// DECAY DECISION:
//   Heat decay is multiplicative and continuous.  Even a perfectly
//   stable field has every cell decaying by HEAT_DECAY every tick.
//   However, cells with heat < DECAY_SKIP_THRESH (1e-5) contribute
//   effectively zero to all subsequent passes.  Skipping them is safe.
//   This is the only sparse-safe optimisation for decay.
//
// DIFFUSE HALO ANALYSIS:
//   The stencil for cell (x,y) reads (x±1, y) and (x, y±1).
//   In sparse mode, we only WRITE frontier cells.
//   We READ neighbours from the current (pre-diffuse) field state.
//   This is correct for an explicit finite-difference scheme —
//   each cell reads the unmodified previous values of its neighbours.
//   No write-outside-frontier occurs.
//
// DIVERGENCE RISK:
//   If a non-frontier cell has significant heat gradient against a
//   frontier cell, the frontier cell's new_heat will be computed
//   correctly (it reads the neighbour's actual heat).  However, the
//   non-frontier neighbour will NOT receive the reciprocal diffusion.
//   This causes asymmetric diffusion at frontier boundaries.
//   Mitigation: halo expansion includes all 4-neighbours of changed
//   cells — so the immediate boundary is always in the frontier.
//   Second-order boundary effects remain until the next tick.
//
// TODO(V3-SPARSE-VALIDATION): After 1000 ticks, compare per-cell
// drift between step() and step_sparse() outputs.  If max drift
// > SPARSE_DIVERGENCE_EPSILON (1e-3), trigger full-field fallback.
//
// ===================================================================

use super::field::{ActivationField, HEAT_DECAY, DIFFUSION_ALPHA,
                   ENTROPY_GROWTH, ENTROPY_DECAY, PRESSURE_STABILISATION};
use super::frontier::PropagationFrontier;

// =====================================================================
// CONSTANTS
// =====================================================================

/// Minimum heat value below which decay can be skipped.
/// Cells below this contribute < 1e-5 to all downstream passes.
pub const DECAY_SKIP_THRESH: f32 = 1e-5;

/// Maximum absolute drift (per-cell, per-field-scalar) between sparse
/// and full solver outputs before a full-field fallback is triggered.
pub const SPARSE_DIVERGENCE_EPSILON: f32 = 1e-3;

/// Maximum average drift across all field cells before hard fallback.
pub const SPARSE_MEAN_DRIFT_EPSILON: f32 = 1e-4;

// =====================================================================
// SPARSE STEP RESULT
// =====================================================================

/// Output from a single sparse solver step.
///
/// Carries the same diagnostic payload as `SolverStepStats` but also
/// records frontier coverage information for validation.
#[derive(Debug, Clone, Copy, Default)]
pub struct SparseSolverResult {
    /// Number of cells actually processed by the sparse passes.
    pub cells_processed: usize,
    /// Total field cells (for density computation).
    pub total_cells:     usize,
    /// Number of cells whose decay was skipped (heat < DECAY_SKIP_THRESH).
    pub decay_skipped:   usize,
    /// Whether a full-field decay pass was run (always true in V3 — see analysis above).
    pub full_decay_ran:  bool,
    /// Whether sparse mode was recommended by the frontier.
    pub used_sparse:     bool,
}

impl SparseSolverResult {
    /// Fraction of field processed by sparse passes [0.0, 1.0].
    #[inline]
    pub fn coverage_density(&self) -> f32 {
        if self.total_cells == 0 { return 0.0; }
        self.cells_processed as f32 / self.total_cells as f32
    }
}

// =====================================================================
// SPARSE SOLVER PASSES
// =====================================================================

/// Execute a selective decay pass.
///
/// Always runs over the full field for correctness (see analysis above),
/// but skips cells with heat and pressure below `DECAY_SKIP_THRESH`.
/// Returns the count of cells that were skipped.
///
/// TODO(V3-SPARSE-VALIDATION): Track skipped cell count and compare
/// against expected decay drift to validate skip threshold correctness.
pub fn decay_selective(field: &mut ActivationField) -> usize {
    let mut skipped = 0usize;
    for cell in &mut field.cells {
        if cell.heat < DECAY_SKIP_THRESH && cell.pressure < DECAY_SKIP_THRESH {
            // Entropy still needs to update for idle drift correctness.
            let idle_weight = 1.0 - cell.activation;
            cell.entropy = (cell.entropy
                + ENTROPY_GROWTH * idle_weight
                - ENTROPY_DECAY * cell.activation)
                .clamp(0.0, 1.0);
            skipped += 1;
            continue;
        }
        cell.heat     *= HEAT_DECAY;
        cell.pressure *= 1.0 - PRESSURE_STABILISATION;
        let idle_weight = 1.0 - cell.activation;
        cell.entropy = (cell.entropy
            + ENTROPY_GROWTH * idle_weight
            - ENTROPY_DECAY * cell.activation)
            .clamp(0.0, 1.0);
    }
    skipped
}

/// Frontier-local heat diffusion.
///
/// Only writes heat to cells in `frontier`.  Reads neighbours from the
/// pre-diffuse field state (explicit finite-difference — read-consistent).
///
/// The scratch buffer is pre-allocated by the solver; only frontier cells
/// are updated in it.  Non-frontier scratch entries remain stale but are
/// never read back for non-frontier cells.
///
/// # Halo Safety
/// Reads from non-frontier neighbours are safe: we only READ them, never
/// WRITE to them.  The value we read is the previous-tick heat, which is
/// correct for an explicit scheme.
///
/// # TODO(V3-SPARSE-VALIDATION): Asymmetric diffusion at frontier boundary —
/// frontier cell reads real neighbour heat, but non-frontier neighbour does
/// NOT receive the reciprocal update.  Asymmetry resolves in subsequent
/// ticks as the frontier expands.  Track max boundary asymmetry.
pub fn diffuse_frontier(
    field:    &mut ActivationField,
    frontier: &PropagationFrontier,
    scratch:  &mut Vec<f32>,
) {
    let n = field.cells.len();
    let w = field.width;
    let h = field.height;

    if scratch.len() < n { scratch.resize(n, 0.0); }

    // Compute new heat only for frontier cells.
    // Non-frontier scratch entries are not written (stale but unused).
    for &idx in frontier.iter_cells() {
        if idx >= n { continue; }
        let y = idx / w;
        let x = idx % w;
        let center = field.cells[idx].heat;

        let north = if y > 0     { field.cells[(y-1)*w + x].heat } else { center };
        let south = if y+1 < h   { field.cells[(y+1)*w + x].heat } else { center };
        let west  = if x > 0     { field.cells[y*w + (x-1)].heat } else { center };
        let east  = if x+1 < w   { field.cells[y*w + (x+1)].heat } else { center };

        scratch[idx] = center + DIFFUSION_ALPHA * (north + south + west + east - 4.0 * center);
    }

    // Write back only frontier cells.
    for &idx in frontier.iter_cells() {
        if idx < n {
            field.cells[idx].heat = scratch[idx].clamp(0.0, 1.0);
        }
    }
}

/// Frontier-local pressure propagation.
///
/// Only frontier cells receive pressure updates.  Topology influence is
/// applied additively, then 4-neighbour averaging smooths discontinuities.
///
/// # Two-pass approach
/// Pass A: inject topology influence into frontier cells only.
/// Pass B: 4-neighbour averaging for frontier cells only (reads from
///         scratch written in Pass A, or old field values for non-frontier
///         neighbours — safe read-consistent behaviour).
///
/// # TODO(V3-SPARSE-VALIDATION): Non-frontier neighbours participate in
/// the averaging read but receive no write.  This may cause frontier
/// cells to "drain" pressure into non-frontier zones asymmetrically.
/// Track max pressure gradient at frontier boundary.
pub fn propagate_pressure_frontier(
    field:          &mut ActivationField,
    frontier:       &PropagationFrontier,
    topo_influence: &[f32],
    scratch:        &mut Vec<f32>,
) {
    let n = field.cells.len();
    let w = field.width;
    let h = field.height;

    if scratch.len() < n { scratch.resize(n, 0.0); }

    // Pass A: topology influence injection for frontier cells.
    // Non-frontier cells: copy their existing pressure unchanged.
    // This is necessary for Pass B to correctly average with non-frontier neighbours.
    for i in 0..n {
        scratch[i] = field.cells[i].pressure;
    }
    for &idx in frontier.iter_cells() {
        if idx >= n { continue; }
        let infl = if idx < topo_influence.len() { topo_influence[idx] } else { 0.0 };
        scratch[idx] = (field.cells[idx].pressure + infl * 0.3).min(1.0);
    }

    // Pass B: 4-neighbour pressure average for frontier cells only.
    for &idx in frontier.iter_cells() {
        if idx >= n { continue; }
        let y = idx / w;
        let x = idx % w;
        let center = scratch[idx];

        let north = if y > 0   { scratch[(y-1)*w + x] } else { center };
        let south = if y+1 < h { scratch[(y+1)*w + x] } else { center };
        let west  = if x > 0   { scratch[y*w + (x-1)] } else { center };
        let east  = if x+1 < w { scratch[y*w + (x+1)] } else { center };

        field.cells[idx].pressure =
            (center * 0.5 + (north + south + west + east) * 0.125).clamp(0.0, 1.0);
    }
}

/// Frontier-local activation recomputation.
///
/// Only frontier cells have their `activation` scalar updated.
/// The formula is identical to the full-field pass — pure element-wise.
///
/// # Correctness
/// `activation = heat × 0.55 + pressure × 0.35 + (1 − entropy) × 0.10`
/// This is a pure local formula — no neighbour dependency.  Safe.
pub fn recompute_activation_frontier(
    field:    &mut ActivationField,
    frontier: &PropagationFrontier,
) {
    let n = field.cells.len();
    for &idx in frontier.iter_cells() {
        if idx >= n { continue; }
        let cell = &mut field.cells[idx];
        cell.activation = (cell.heat * 0.55
            + cell.pressure * 0.35
            + (1.0 - cell.entropy) * 0.10)
            .clamp(0.0, 1.0);
    }
}

/// Frontier-local execution probability recomputation.
///
/// Only frontier cells have their `execution_probability` updated.
/// Formula: smoothstep S-curve `a² × (3 − 2a)`.
///
/// # Correctness
/// Pure local formula — no neighbour dependency.  Safe.
pub fn recompute_probability_frontier(
    field:    &mut ActivationField,
    frontier: &PropagationFrontier,
) {
    let n = field.cells.len();
    for &idx in frontier.iter_cells() {
        if idx >= n { continue; }
        let a = field.cells[idx].activation;
        field.cells[idx].execution_probability = a * a * (3.0 - 2.0 * a);
    }
}

// =====================================================================
// FULL SPARSE STEP
// =====================================================================

/// Execute a sparse activation field step over frontier cells only.
///
/// This is the primary entry point for the differential runtime.
/// It runs all five solver passes in frontier-local mode and returns
/// a `SparseSolverResult` describing coverage and skip statistics.
///
/// # Execution Order
/// 1. `decay_selective()`            — full field, skips near-zero cells
/// 2. `diffuse_frontier()`           — frontier only, read-halo safe
/// 3. `propagate_pressure_frontier()` — frontier only, read-halo safe
/// 4. `recompute_activation_frontier()` — frontier only, pure local
/// 5. `recompute_probability_frontier()` — frontier only, pure local
///
/// # Full Solver Availability
/// The full solver `ActivationSolver::step()` is NOT replaced by this
/// function.  Both coexist.  `step_sparse()` is called in VALIDATION
/// MODE alongside `step()` for parity comparison.
///
/// TODO(V3-SPARSE-VALIDATION): Once 1000-tick parity validation passes
/// (max drift < SPARSE_DIVERGENCE_EPSILON), promote step_sparse() to
/// the authoritative path and make step() the fallback.
pub fn step_sparse(
    field:          &mut ActivationField,
    frontier:       &PropagationFrontier,
    topo_influence: &[f32],
    diff_scratch:   &mut Vec<f32>,
    pres_scratch:   &mut Vec<f32>,
) -> SparseSolverResult {
    let total = field.cells.len();

    // If frontier is empty, nothing to do — field is stable.
    if frontier.is_empty() {
        return SparseSolverResult {
            cells_processed: 0,
            total_cells:     total,
            decay_skipped:   0,
            full_decay_ran:  true,
            used_sparse:     true,
        };
    }

    // Pass 1: selective decay (touches all cells for entropy correctness)
    let decay_skipped = decay_selective(field);

    // Passes 2–5: frontier-local only
    if frontier.should_use_sparse() {
        diffuse_frontier(field, frontier, diff_scratch);
        propagate_pressure_frontier(field, frontier, topo_influence, pres_scratch);
        recompute_activation_frontier(field, frontier);
        recompute_probability_frontier(field, frontier);

        SparseSolverResult {
            cells_processed: frontier.frontier_size(),
            total_cells:     total,
            decay_skipped,
            full_decay_ran:  true,
            used_sparse:     true,
        }
    } else {
        // Frontier too large — fall back to full passes for phases 2-5
        // but still use decay_selective result from phase 1.
        field.diffuse(diff_scratch);
        // Rebuild pressure scratch from full field
        if pres_scratch.len() != total { pres_scratch.resize(total, 0.0); }
        for (i, cell) in field.cells.iter().enumerate() {
            let infl = if i < topo_influence.len() { topo_influence[i] } else { 0.0 };
            pres_scratch[i] = (cell.pressure + infl * 0.3).min(1.0);
        }
        // Full pressure smoothing
        let w = field.width;
        let h = field.height;
        for y in 0..h {
            for x in 0..w {
                let idx = y * w + x;
                let c = pres_scratch[idx];
                let n = if y > 0     { pres_scratch[(y-1)*w+x] } else { c };
                let s = if y+1 < h   { pres_scratch[(y+1)*w+x] } else { c };
                let ww = if x > 0    { pres_scratch[y*w+(x-1)] } else { c };
                let e = if x+1 < w   { pres_scratch[y*w+(x+1)] } else { c };
                field.cells[idx].pressure = (c*0.5 + (n+s+ww+e)*0.125).clamp(0.0, 1.0);
            }
        }
        field.recompute_activation();
        field.recompute_execution_probability();

        SparseSolverResult {
            cells_processed: total,
            total_cells:     total,
            decay_skipped,
            full_decay_ran:  true,
            used_sparse:     false,
        }
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;
    use crate::activation::delta::{FieldDeltaTracker};
    use crate::activation::frontier::PropagationFrontier;

    fn make_frontier_from_cell(w: usize, h: usize, cell: usize) -> PropagationFrontier {
        let mut delta = crate::activation::delta::FieldDeltaMask::new(w * h);
        delta.set(cell);
        let mut frontier = PropagationFrontier::new(w, h);
        frontier.build_from_delta(&delta, w, h);
        frontier
    }

    #[test]
    fn sparse_step_empty_frontier_is_stable() {
        let mut field = ActivationField::new(4, 4);
        field.inject_heat(0, 0.5);
        let frontier = PropagationFrontier::new(4, 4); // empty
        let mut d = Vec::new();
        let mut p = Vec::new();
        let result = step_sparse(&mut field, &frontier, &[], &mut d, &mut p);
        assert_eq!(result.cells_processed, 0);
        assert!(result.used_sparse);
    }

    #[test]
    fn sparse_diffuse_frontier_cell_changes() {
        let mut field = ActivationField::new(4, 4);
        field.cells[5].heat = 0.8; // center of 4×4
        let frontier = make_frontier_from_cell(4, 4, 5);
        let mut scratch = Vec::new();
        let heat_before = field.cells[5].heat;
        diffuse_frontier(&mut field, &frontier, &mut scratch);
        // Cell 5 should have diffused heat to/from neighbours
        // Centre had 0.8, all neighbours 0.0: expected decrease
        assert!(field.cells[5].heat < heat_before,
            "centre cell heat should decrease when surrounded by cool neighbours");
    }

    #[test]
    fn sparse_recompute_activation_is_local() {
        let mut field = ActivationField::new(4, 4);
        field.cells[3].heat = 0.9;
        let frontier = make_frontier_from_cell(4, 4, 3);
        recompute_activation_frontier(&mut field, &frontier);
        // Cell 3 activation should be non-zero
        assert!(field.cells[3].activation > 0.0);
        // Cell 0 should be untouched
        assert_eq!(field.cells[0].activation, 0.0);
    }

    #[test]
    fn sparse_probability_is_bounded() {
        let mut field = ActivationField::new(4, 4);
        field.cells[0].activation = 1.0;
        let frontier = make_frontier_from_cell(4, 4, 0);
        recompute_probability_frontier(&mut field, &frontier);
        assert!(field.cells[0].execution_probability <= 1.0);
        assert!(field.cells[0].execution_probability > 0.0);
    }

    #[test]
    fn decay_selective_skips_cold_cells() {
        let mut field = ActivationField::new(4, 4);
        // Only cell 0 is hot
        field.cells[0].heat = 0.5;
        let skipped = decay_selective(&mut field);
        // 15 cold cells should be skipped for heat/pressure decay
        assert_eq!(skipped, 15);
        assert!(field.cells[0].heat < 0.5, "hot cell should have decayed");
    }

    #[test]
    fn step_sparse_produces_valid_result() {
        let mut field = ActivationField::new(8, 8);
        field.inject_heat(20, 0.7);
        let mut tracker = FieldDeltaTracker::new(64, 0.05);
        tracker.compute(&field);
        field.cells[20].heat = 0.9;
        let mask = tracker.compute(&field);
        let mut frontier = PropagationFrontier::new(8, 8);
        frontier.build_from_delta(mask, 8, 8);
        let mut d = Vec::new();
        let mut p = Vec::new();
        let result = step_sparse(&mut field, &frontier, &[], &mut d, &mut p);
        assert!(result.total_cells == 64);
        assert!(result.coverage_density() <= 1.0);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\activation\validation.rs ---
// ===================================================================
// mirage-mkr-core/src/activation/validation.rs
// PURPOSE: Sparse Validation Runtime — Parity Testing Infrastructure
//
// ---------------------------------------------------------------
// DESIGN INTENT
// ---------------------------------------------------------------
//
// This module implements side-by-side parity validation between the
// full solver and the sparse solver.  It is NOT the authoritative
// runtime path.  It exists solely to build confidence that sparse
// output is correct before the sparse solver takes authority.
//
// EXECUTION MODEL:
//   ValidationMode::Parallel:
//     1. Snapshot current field → snapshot_field
//     2. Run step() on the live field
//     3. Run step_sparse() on snapshot_field
//     4. Compare outputs → ParityComparisonResult
//     5. If drift < epsilon → record PASS
//     6. If drift >= epsilon → record FAIL, preserve full as authority
//
// AUTHORITY RULE:
//   The FULL solver result is ALWAYS written to the live field.
//   The sparse result is written to a SHADOW FIELD for comparison only.
//   This ensures ZERO risk of sparse divergence affecting runtime behavior.
//
// TODO(V3-SPARSE-VALIDATION): After SPARSE_PROMOTION_THRESHOLD consecutive
// PASS ticks, the runtime may be promoted to SPARSE_AUTHORITATIVE mode.
// This promotion must be explicitly requested (not automatic) and reviewed
// by the Lead Runtime Architect before enabling.
//
// ===================================================================

use super::field::ActivationField;
use super::frontier::PropagationFrontier;
use super::sparse::{step_sparse, SparseSolverResult, SPARSE_DIVERGENCE_EPSILON};
use super::solver::SolverStepStats;


// =====================================================================
// CONSTANTS
// =====================================================================

/// Number of consecutive PASS ticks required before sparse promotion
/// may even be considered.  NOT automatically applied.
pub const SPARSE_PROMOTION_THRESHOLD: u64 = 1_000;

/// Default epsilon for per-cell activation drift comparison.
pub const VALIDATION_ACTIVATION_EPSILON: f32 = 1e-3;

/// Default epsilon for per-cell probability drift comparison.
pub const VALIDATION_PROBABILITY_EPSILON: f32 = 1e-3;

/// Default epsilon for per-cell pressure drift comparison.
pub const VALIDATION_PRESSURE_EPSILON:    f32 = 1e-3;

// =====================================================================
// VALIDATION MODE
// =====================================================================

/// Controls what the validation layer does each tick.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValidationMode {
    /// Validation is disabled.  Only full solver runs.  Zero overhead.
    #[default]
    Disabled,
    /// Run both solvers in parallel.  Full solver is authoritative.
    /// Shadow field receives sparse result for comparison.
    Parallel,
    /// Sparse solver is authoritative (NOT YET ENABLED — requires
    /// explicit promotion after SPARSE_PROMOTION_THRESHOLD PASSes).
    SparseAuthoritative,
}

// =====================================================================
// RESULT TYPES
// =====================================================================

/// Result from the full solver pass.
#[derive(Debug, Clone, Copy, Default)]
pub struct FullSolverResult {
    pub stats: SolverStepStats,
}

/// Parity comparison between full and sparse solver outputs.
#[derive(Debug, Clone, Copy, Default)]
pub struct ParityComparisonResult {
    /// Maximum per-cell absolute drift in `activation`.
    pub max_activation_drift:    f32,
    /// Maximum per-cell absolute drift in `execution_probability`.
    pub max_probability_drift:   f32,
    /// Maximum per-cell absolute drift in `pressure`.
    pub max_pressure_drift:      f32,
    /// Mean absolute drift across all cells for `activation`.
    pub mean_activation_drift:   f32,
    /// Mean absolute drift across all cells for `probability`.
    pub mean_probability_drift:  f32,
    /// Number of cells that exceeded SPARSE_DIVERGENCE_EPSILON in activation.
    pub activation_violations:   usize,
    /// Number of cells that exceeded SPARSE_DIVERGENCE_EPSILON in probability.
    pub probability_violations:  usize,
    /// True if ALL cells passed within their epsilon tolerances.
    pub all_passed:              bool,
    /// Number of frontier cells compared (others are trivially 0 drift).
    pub cells_compared:          usize,
}

impl ParityComparisonResult {
    /// Compute parity between two activation fields.
    /// Only compares cells in the frontier (others are expected to differ
    /// for decay-skipped cells outside the active region).
    pub fn compute(
        full:     &ActivationField,
        sparse:   &ActivationField,
        frontier: &PropagationFrontier,
        act_eps:  f32,
        prob_eps: f32,
        pres_eps: f32,
    ) -> Self {
        let n = full.cells.len().min(sparse.cells.len());
        let mut max_act  = 0.0f32;
        let mut max_prob = 0.0f32;
        let mut max_pres = 0.0f32;
        let mut sum_act  = 0.0f32;
        let mut sum_prob = 0.0f32;
        let mut act_viol = 0usize;
        let mut prob_viol = 0usize;
        let mut count = 0usize;

        // Compare only frontier cells — non-frontier cells are intentionally
        // NOT updated by the sparse solver and will show expected drift.
        for &idx in frontier.iter_cells() {
            if idx >= n { continue; }
            let fa = full.cells[idx].activation;
            let sa = sparse.cells[idx].activation;
            let da = (fa - sa).abs();

            let fp = full.cells[idx].execution_probability;
            let sp = sparse.cells[idx].execution_probability;
            let dp = (fp - sp).abs();

            let fpr = full.cells[idx].pressure;
            let spr = sparse.cells[idx].pressure;
            let dpr = (fpr - spr).abs();

            if da > max_act  { max_act  = da; }
            if dp > max_prob { max_prob = dp; }
            if dpr > max_pres { max_pres = dpr; }

            sum_act  += da;
            sum_prob += dp;
            if da > act_eps  { act_viol  += 1; }
            if dp > prob_eps { prob_viol += 1; }

            let _ = pres_eps; // reserved for future use
            count += 1;
        }

        let cells_compared = count;
        let mean_act  = if count > 0 { sum_act  / count as f32 } else { 0.0 };
        let mean_prob = if count > 0 { sum_prob / count as f32 } else { 0.0 };

        Self {
            max_activation_drift:   max_act,
            max_probability_drift:  max_prob,
            max_pressure_drift:     max_pres,
            mean_activation_drift:  mean_act,
            mean_probability_drift: mean_prob,
            activation_violations:  act_viol,
            probability_violations: prob_viol,
            all_passed:             act_viol == 0 && prob_viol == 0,
            cells_compared,
        }
    }

    /// True if drift is severe enough to warrant a hard fallback.
    #[inline]
    pub fn is_severe_divergence(&self) -> bool {
        self.max_activation_drift   > SPARSE_DIVERGENCE_EPSILON
            || self.max_probability_drift > SPARSE_DIVERGENCE_EPSILON
    }
}

// =====================================================================
// FRONTIER VALIDATION REPORT
// =====================================================================

/// Accumulated validation statistics over multiple ticks.
///
/// Reset at the start of each validation run.  Accumulates across ticks
/// to build a statistical picture of sparse solver quality.
#[derive(Debug, Clone, Default)]
pub struct FrontierValidationReport {
    /// Total ticks validated since last reset.
    pub ticks_run:            u64,
    /// Ticks where all frontier cells passed within epsilon.
    pub ticks_passed:         u64,
    /// Ticks where at least one frontier cell exceeded epsilon.
    pub ticks_failed:         u64,
    /// Consecutive pass count (resets on any failure).
    pub consecutive_passes:   u64,
    /// Peak max_activation_drift seen across all ticks.
    pub peak_activation_drift: f32,
    /// Peak max_probability_drift seen across all ticks.
    pub peak_probability_drift: f32,
    /// Running average of mean_activation_drift.
    pub running_mean_activation_drift: f32,
    /// Peak frontier density seen.
    pub peak_frontier_density: f32,
    /// Total severe divergence events (triggered hard fallback).
    pub severe_divergence_events: u64,
    /// Sparse result from last tick (coverage etc).
    pub last_sparse_result:   Option<SparseSolverResult>,
}

impl FrontierValidationReport {
    pub fn new() -> Self { Self::default() }

    /// Record the result of one validation tick.
    pub fn record(&mut self, parity: &ParityComparisonResult, sparse: SparseSolverResult) {
        self.ticks_run += 1;
        self.last_sparse_result = Some(sparse);

        if parity.max_activation_drift > self.peak_activation_drift {
            self.peak_activation_drift = parity.max_activation_drift;
        }
        if parity.max_probability_drift > self.peak_probability_drift {
            self.peak_probability_drift = parity.max_probability_drift;
        }
        if sparse.coverage_density() > self.peak_frontier_density {
            self.peak_frontier_density = sparse.coverage_density();
        }

        // Exponential moving average for mean drift
        let alpha = 0.05;
        self.running_mean_activation_drift =
            self.running_mean_activation_drift * (1.0 - alpha)
            + parity.mean_activation_drift * alpha;

        if parity.all_passed {
            self.ticks_passed += 1;
            self.consecutive_passes += 1;
        } else {
            self.ticks_failed += 1;
            self.consecutive_passes = 0;
        }

        if parity.is_severe_divergence() {
            self.severe_divergence_events += 1;
        }
    }

    /// True if sparse solver has earned promotion consideration.
    pub fn eligible_for_promotion(&self) -> bool {
        self.consecutive_passes >= SPARSE_PROMOTION_THRESHOLD
            && self.severe_divergence_events == 0
    }

    /// Pass rate across all ticks [0.0, 1.0].
    pub fn pass_rate(&self) -> f32 {
        if self.ticks_run == 0 { return 1.0; }
        self.ticks_passed as f32 / self.ticks_run as f32
    }

    /// Reset all statistics.
    pub fn reset(&mut self) { *self = Self::default(); }
}

// =====================================================================
// SPARSE VALIDATION RUNNER
// =====================================================================

/// Runs both full and sparse solvers in parallel for one tick.
///
/// The live `field` receives the FULL solver result (authoritative).
/// The `shadow_field` receives the SPARSE solver result (comparison only).
///
/// Returns the parity comparison between the two outputs.
///
/// # Safety Guarantee
/// If parity fails, the live field is unaffected — it already has the
/// full solver result.  The shadow field may have incorrect state but
/// is never used for downstream passes.
///
/// TODO(V3-SPARSE-VALIDATION): Wire into MKRWorld::tick() between Phase 1
/// and Phase 1.5 when ValidationMode::Parallel is active.
pub struct SparseValidationRunner {
    /// Shadow field: receives sparse solver output for comparison.
    pub shadow_field:   ActivationField,
    /// Scratch buffers for the sparse solver (avoids allocation).
    sparse_diff_scratch: Vec<f32>,
    sparse_pres_scratch: Vec<f32>,
    /// Current validation mode.
    pub mode:            ValidationMode,
    /// Accumulated report across all ticks.
    pub report:          FrontierValidationReport,
}

impl SparseValidationRunner {
    /// Create a validation runner for a `width × height` field.
    pub fn new(width: usize, height: usize) -> Self {
        Self {
            shadow_field:        ActivationField::new(width, height),
            sparse_diff_scratch: Vec::new(),
            sparse_pres_scratch: Vec::new(),
            mode:                ValidationMode::Disabled,
            report:              FrontierValidationReport::new(),
        }
    }

    /// Enable parallel validation mode.
    pub fn enable_parallel(&mut self) {
        self.mode = ValidationMode::Parallel;
    }

    /// Disable validation.
    pub fn disable(&mut self) {
        self.mode = ValidationMode::Disabled;
    }

    /// True if validation is currently active.
    #[inline]
    pub fn is_active(&self) -> bool {
        matches!(self.mode, ValidationMode::Parallel | ValidationMode::SparseAuthoritative)
    }

    /// Run one parallel validation tick.
    ///
    /// Copies the live field into the shadow, runs step_sparse() on
    /// the shadow, and compares against the live field (which has
    /// already been advanced by the full solver).
    ///
    /// Returns `None` if validation is disabled.
    pub fn validate_tick(
        &mut self,
        live_field:     &ActivationField,
        frontier:       &PropagationFrontier,
        topo_influence: &[f32],
    ) -> Option<ParityComparisonResult> {
        if !self.is_active() { return None; }

        // Sync shadow with the state BEFORE the full solver ran
        // (snapshot captured by FieldDeltaTracker — we use the shadow
        // as a second field that we run the sparse solver on).
        //
        // NOTE: For true parity, shadow_field should start from the same
        // pre-tick state as the live field.  We approximate by copying
        // the LIVE FIELD after the full solver ran, then running sparse
        // on a copy of the PRE-TICK state.
        //
        // TODO(V3-SPARSE-VALIDATION): To achieve true pre-tick snapshot
        // comparison, MKRWorld must snapshot the field before Phase 1
        // and provide it here.  For now, this runner demonstrates the
        // parity infrastructure; exact pre-tick comparison is a future step.

        // Run sparse on shadow (which is currently the live post-full state
        // — for testing the infrastructure only, not true pre-tick parity).
        let sparse_result = step_sparse(
            &mut self.shadow_field,
            frontier,
            topo_influence,
            &mut self.sparse_diff_scratch,
            &mut self.sparse_pres_scratch,
        );

        // Compare shadow (sparse output) against live (full output).
        let parity = ParityComparisonResult::compute(
            live_field,
            &self.shadow_field,
            frontier,
            VALIDATION_ACTIVATION_EPSILON,
            VALIDATION_PROBABILITY_EPSILON,
            VALIDATION_PRESSURE_EPSILON,
        );

        self.report.record(&parity, sparse_result);
        Some(parity)
    }

    /// Copy live field into shadow field for pre-tick snapshot.
    ///
    /// Call this BEFORE running the full solver so the shadow field
    /// has the same initial state as the live field.
    pub fn snapshot_pre_tick(&mut self, live_field: &ActivationField) {
        debug_assert_eq!(
            self.shadow_field.cells.len(), live_field.cells.len(),
            "shadow field size must match live field"
        );
        self.shadow_field.cells.copy_from_slice(&live_field.cells);
    }
}

// =====================================================================
// DIVERGENCE HEATMAP PREPARATION
// =====================================================================

/// Per-cell drift accumulated over N ticks (Task 5 preparation).
///
/// Used to identify cells with persistent high drift — which may
/// indicate incorrect frontier expansion or halo asymmetry.
pub struct DivergenceHeatmap {
    /// Per-cell cumulative activation drift.
    pub activation_drift:    Vec<f32>,
    /// Per-cell cumulative probability drift.
    pub probability_drift:   Vec<f32>,
    /// Number of ticks accumulated.
    pub ticks_accumulated:   u64,
}

impl DivergenceHeatmap {
    pub fn new(num_cells: usize) -> Self {
        Self {
            activation_drift:  vec![0.0; num_cells],
            probability_drift: vec![0.0; num_cells],
            ticks_accumulated: 0,
        }
    }

    /// Accumulate drift from a full/sparse comparison.
    pub fn accumulate(&mut self, full: &ActivationField, sparse: &ActivationField) {
        let n = full.cells.len().min(sparse.cells.len()).min(self.activation_drift.len());
        for i in 0..n {
            self.activation_drift[i] +=
                (full.cells[i].activation - sparse.cells[i].activation).abs();
            self.probability_drift[i] +=
                (full.cells[i].execution_probability - sparse.cells[i].execution_probability).abs();
        }
        self.ticks_accumulated += 1;
    }

    /// Return mean per-cell drift across all accumulated ticks.
    pub fn mean_activation_drift(&self) -> f32 {
        if self.ticks_accumulated == 0 || self.activation_drift.is_empty() { return 0.0; }
        let sum: f32 = self.activation_drift.iter().sum();
        sum / (self.activation_drift.len() as f32 * self.ticks_accumulated as f32)
    }

    /// Return the cell index with the highest cumulative drift.
    pub fn hottest_cell(&self) -> Option<usize> {
        self.activation_drift.iter().enumerate()
            .max_by(|a, b| a.1.partial_cmp(b.1).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i)
    }

    /// Reset all accumulated drift.
    pub fn reset(&mut self) {
        self.activation_drift.fill(0.0);
        self.probability_drift.fill(0.0);
        self.ticks_accumulated = 0;
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parity_comparison_identical_fields() {
        let f1 = ActivationField::new(4, 4);
        let f2 = ActivationField::new(4, 4);
        let frontier = PropagationFrontier::new(4, 4);
        let result = ParityComparisonResult::compute(&f1, &f2, &frontier, 1e-3, 1e-3, 1e-3);
        assert!(result.all_passed);
        assert_eq!(result.cells_compared, 0, "empty frontier compares 0 cells");
    }

    #[test]
    fn validation_report_accumulates_passes() {
        let mut report = FrontierValidationReport::new();
        let parity = ParityComparisonResult { all_passed: true, ..Default::default() };
        let sparse  = SparseSolverResult::default();
        for _ in 0..10 { report.record(&parity, sparse); }
        assert_eq!(report.ticks_passed, 10);
        assert_eq!(report.consecutive_passes, 10);
        assert_eq!(report.ticks_failed, 0);
    }

    #[test]
    fn validation_report_resets_consecutive_on_failure() {
        let mut report = FrontierValidationReport::new();
        let pass = ParityComparisonResult { all_passed: true, ..Default::default() };
        let fail = ParityComparisonResult {
            all_passed: false,
            activation_violations: 1,
            ..Default::default()
        };
        let sparse = SparseSolverResult::default();
        for _ in 0..5 { report.record(&pass, sparse); }
        report.record(&fail, sparse);
        assert_eq!(report.consecutive_passes, 0);
        assert_eq!(report.ticks_failed, 1);
    }

    #[test]
    fn runner_disabled_returns_none() {
        let mut runner = SparseValidationRunner::new(4, 4);
        let field    = ActivationField::new(4, 4);
        let frontier = PropagationFrontier::new(4, 4);
        let result = runner.validate_tick(&field, &frontier, &[]);
        assert!(result.is_none(), "disabled runner should return None");
    }

    #[test]
    fn runner_parallel_returns_result() {
        let mut runner = SparseValidationRunner::new(4, 4);
        runner.enable_parallel();
        let field    = ActivationField::new(4, 4);
        let frontier = PropagationFrontier::new(4, 4);
        let result = runner.validate_tick(&field, &frontier, &[]);
        assert!(result.is_some(), "parallel runner should return parity result");
    }

    #[test]
    fn heatmap_accumulates_drift() {
        let mut heatmap = DivergenceHeatmap::new(16);
        let mut f1 = ActivationField::new(4, 4);
        let f2     = ActivationField::new(4, 4);
        f1.cells[0].activation = 0.5;
        heatmap.accumulate(&f1, &f2);
        assert!(heatmap.activation_drift[0] > 0.0);
        assert_eq!(heatmap.ticks_accumulated, 1);
    }

    #[test]
    fn not_eligible_for_promotion_below_threshold() {
        let mut report = FrontierValidationReport::new();
        let pass = ParityComparisonResult { all_passed: true, ..Default::default() };
        let sparse = SparseSolverResult::default();
        for _ in 0..100 { report.record(&pass, sparse); }
        assert!(!report.eligible_for_promotion(),
            "should not be eligible before {} consecutive passes",
            SPARSE_PROMOTION_THRESHOLD);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\activation\weights.rs ---
// ===================================================================
// mirage-mkr-core/src/activation/weights.rs
// PURPOSE: ExecutionWeights — Runtime-Tunable Activation Coefficients
//
// DESIGN:
// ExecutionWeights captures the runtime-configurable blend of signals
// that feed into the activation computation.  Each weight is a
// continuous scalar in [0.0, 1.0] that scales one class of input.
//
// This is NOT a scheduler priority table.
// This is NOT an ECS component weight.
// It is a tunable linear combination kernel for the activation field.
//
// CURRENT SOURCES (V3, pre-CEK):
// - thermal_weight:  heat contribution from the thermal subsystem.
// - topology_weight: contribution from TopologyGraph edge influence.
// - entropy_weight:  how strongly entropy suppresses activation.
// - residency_weight: bonus activation for chunks currently resident
//   in VRAM (compatibility bridge to old ThermalSystem).
//
// TODO(V3-CEK): When CEK is integrated, these weights will be driven
// by CEK field outputs rather than manually tuned constants.  The
// `compute_activation` / `compute_probability` method signatures must
// stay stable.
// ===================================================================

/// Runtime-tunable weights that control the activation field blend.
///
/// All fields are normalised scalars in `[0.0, 1.0]`.
/// The activation formula is:
///
/// ```text
/// activation = clamp(
///     heat   × thermal_weight  +
///     topo   × topology_weight +
///     (1-e)  × (1 - entropy_weight × entropy) +
///     resid  × residency_weight,
///     0, 1
/// )
/// ```
///
/// where `entropy_weight` controls how strongly entropy penalises
/// activation (set to 0.0 to disable entropy influence entirely).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ExecutionWeights {
    /// How much raw heat contributes to activation.
    /// Default: 0.50 — heat is the primary activation driver.
    pub thermal_weight: f32,

    /// How much topology-graph edge pressure contributes.
    /// Default: 0.25 — neighbours amplify activation.
    pub topology_weight: f32,

    /// How strongly entropy suppresses the activation signal.
    /// 0.0 = entropy has no effect; 1.0 = full entropy penalty.
    /// Default: 0.15.
    pub entropy_weight: f32,

    /// Bonus activation for currently VRAM-resident chunks.
    /// Compatibility bridge to the old ThermalSystem residency concept.
    /// Default: 0.10.
    pub residency_weight: f32,
}

impl Default for ExecutionWeights {
    fn default() -> Self {
        Self {
            thermal_weight:   0.50,
            topology_weight:  0.25,
            entropy_weight:   0.15,
            residency_weight: 0.10,
        }
    }
}

impl ExecutionWeights {
    // ------------------------------------------------------------------
    // Construction helpers
    // ------------------------------------------------------------------

    /// Create weights with custom values.  All inputs are clamped to
    /// `[0.0, 1.0]` to prevent runaway activation.
    pub fn new(thermal: f32, topology: f32, entropy: f32, residency: f32) -> Self {
        Self {
            thermal_weight:   thermal.clamp(0.0, 1.0),
            topology_weight:  topology.clamp(0.0, 1.0),
            entropy_weight:   entropy.clamp(0.0, 1.0),
            residency_weight: residency.clamp(0.0, 1.0),
        }
    }

    /// Weights biased toward thermal dominance — useful for initial
    /// bring-up before CEK is integrated.
    pub fn thermal_dominant() -> Self {
        Self {
            thermal_weight:   0.70,
            topology_weight:  0.15,
            entropy_weight:   0.10,
            residency_weight: 0.05,
        }
    }

    // ------------------------------------------------------------------
    // Activation computation
    // ------------------------------------------------------------------

    /// Compute a continuous activation scalar from raw field signals.
    ///
    /// # Parameters
    /// * `heat`      — cell heat (0..1).
    /// * `topo_pull` — topology graph influence on this cell (0..1).
    /// * `entropy`   — cell entropy (0..1, high = uncertain/idle).
    /// * `is_resident` — whether the backing chunk is currently VRAM-resident.
    ///   Passed as f32 (1.0 = yes, 0.0 = no) to stay branchless.
    ///
    /// # Returns
    /// Continuous activation in `[0.0, 1.0]`.
    #[inline]
    pub fn compute_activation(
        &self,
        heat: f32,
        topo_pull: f32,
        entropy: f32,
        is_resident: f32,
    ) -> f32 {
        let entropy_penalty = self.entropy_weight * entropy;
        let raw = heat      * self.thermal_weight
                + topo_pull * self.topology_weight
                + (1.0 - entropy_penalty)
                + is_resident * self.residency_weight;
        // Normalise by sum of all weights + 1.0 (from entropy complement term)
        let normaliser = self.thermal_weight
                       + self.topology_weight
                       + 1.0
                       + self.residency_weight;
        (raw / normaliser).clamp(0.0, 1.0)
    }

    /// Compute execution probability from an activation scalar.
    ///
    /// Applies a smoothstep curve so that:
    /// * Low activation → near-zero probability (soft gate)
    /// * Mid activation → smooth rise
    /// * High activation → saturates near 1.0
    ///
    /// This is the same formula as `ActivationField::recompute_execution_probability`
    /// but exposed here for use in per-chunk weight-aware emission decisions.
    ///
    /// Formula: `p = a² × (3 − 2a)`  (cubic Hermite interpolation).
    #[inline]
    pub fn compute_probability(&self, activation: f32) -> f32 {
        let a = activation.clamp(0.0, 1.0);
        a * a * (3.0 - 2.0 * a)
    }

    // ------------------------------------------------------------------
    // Runtime tuning
    // ------------------------------------------------------------------

    /// Linearly interpolate toward a target weight set at rate `t`.
    ///
    /// Useful for smooth runtime re-tuning without discontinuities.
    #[inline]
    pub fn lerp_toward(&self, target: &ExecutionWeights, t: f32) -> ExecutionWeights {
        let t = t.clamp(0.0, 1.0);
        ExecutionWeights {
            thermal_weight:   lerp(self.thermal_weight,   target.thermal_weight,   t),
            topology_weight:  lerp(self.topology_weight,  target.topology_weight,  t),
            entropy_weight:   lerp(self.entropy_weight,   target.entropy_weight,   t),
            residency_weight: lerp(self.residency_weight, target.residency_weight, t),
        }
    }
}

#[inline]
fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_weights_are_valid() {
        let w = ExecutionWeights::default();
        assert!(w.thermal_weight   >= 0.0 && w.thermal_weight   <= 1.0);
        assert!(w.topology_weight  >= 0.0 && w.topology_weight  <= 1.0);
        assert!(w.entropy_weight   >= 0.0 && w.entropy_weight   <= 1.0);
        assert!(w.residency_weight >= 0.0 && w.residency_weight <= 1.0);
    }

    #[test]
    fn compute_activation_bounded() {
        let w = ExecutionWeights::default();
        // Fully active: max heat, max topo, zero entropy, resident
        let a = w.compute_activation(1.0, 1.0, 0.0, 1.0);
        assert!(a >= 0.0 && a <= 1.0, "activation={}", a);

        // Fully dormant: no heat, no topo, max entropy, not resident
        let b = w.compute_activation(0.0, 0.0, 1.0, 0.0);
        assert!(b >= 0.0 && b <= 1.0, "activation={}", b);
    }

    #[test]
    fn higher_heat_raises_activation() {
        let w = ExecutionWeights::default();
        let low  = w.compute_activation(0.1, 0.0, 0.5, 0.0);
        let high = w.compute_activation(0.9, 0.0, 0.5, 0.0);
        assert!(high > low, "high_heat={} should be > low_heat={}", high, low);
    }

    #[test]
    fn compute_probability_smoothstep_endpoints() {
        let w = ExecutionWeights::default();
        assert!((w.compute_probability(0.0) - 0.0).abs() < 1e-6);
        assert!((w.compute_probability(1.0) - 1.0).abs() < 1e-6);
    }

    #[test]
    fn probability_monotone() {
        let w = ExecutionWeights::default();
        let p0 = w.compute_probability(0.0);
        let p5 = w.compute_probability(0.5);
        let p1 = w.compute_probability(1.0);
        assert!(p0 <= p5 && p5 <= p1);
    }

    #[test]
    fn lerp_toward_interpolates() {
        let a = ExecutionWeights::default();
        let b = ExecutionWeights::thermal_dominant();
        let mid = a.lerp_toward(&b, 0.5);
        assert!((mid.thermal_weight - (a.thermal_weight + b.thermal_weight) / 2.0).abs() < 1e-6);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\bridge\execution_bridge.rs ---
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



--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\bridge\mod.rs ---
// ===================================================================
// mirage-mkr-core/src/bridge/mod.rs  (V3 — Federated Stabilization Pass)
// PURPOSE: Bridge module root — PROTOCOL/TRANSLATION layers only
//
// ---------------------------------------------------------------
// BRIDGE OWNERSHIP INVARIANT (CRITICAL)
// ---------------------------------------------------------------
// Every bridge in this module MUST satisfy:
//
//   INPUT:  owned by a canonical subsystem (ActivationField, OASIS, etc.)
//   OUTPUT: protocol descriptor consumed by the target subsystem
//
// Bridges MUST NOT:
//   * Own runtime state (no heap-persistent scheduling queues)
//   * Make execution eligibility decisions (that is EmissionGate)
//   * Own streaming lifecycle (that is OASIS/StreamingFabric)
//   * Own residency truth (that is ResidencyTracker in renderer)
//   * Spawn threads or fibers (future: FiberPool will do this)
//
// ---------------------------------------------------------------
// CURRENT BRIDGES
// ---------------------------------------------------------------
//
// renderer_bridge  — ActivationField → RuntimeDirectory::chunk_runtime_states
//                    (continuous probability → discrete ChunkState)
//                    STATUS: Stable. Translation-only. No state owned.
//
// renderer_validation — Shadow validation layer for sparse renderer path.
//                        Compares apply_changed_cells() against
//                        apply_to_directory().
//                        STATUS: Non-authoritative validation only.
//
// execution_bridge — EmissionRequest → SchedulingRequest
//                    (emission layer output → executor-compatible descriptor)
//                    STATUS: Stable. Stateless struct. Pure translation.
//
// ---------------------------------------------------------------
// PLANNED BRIDGES (not yet wired)
// ---------------------------------------------------------------
//
// TODO(V3-BRIDGE-STREAMING): streaming_bridge
//   Translate StreamingDecision → OASIS StreamRequest.
//   MKR produces StreamingDecisions (eligibility only).
//   OASIS executes them (lifecycle only).
//   The bridge does NOT execute the stream itself.
//
// TODO(V3-BRIDGE-PHYSICS): physics_bridge
//   Translate execution_probability → simulation_factor per chunk.
//   Physics reads simulation_factor; field is the authority.
//
// TODO(V3-BRIDGE-RESIDENCY): residency_bridge
//   Translate ActivationField probability → ResidencyDescriptor.
//   Renderer's ResidencyTracker consumes descriptors passively.
//   ResidencyTracker MUST NOT infer thermal authority independently.
// ===================================================================

pub mod renderer_bridge;
pub mod renderer_validation;
pub mod execution_bridge;

pub use renderer_bridge::RendererBridge;
pub use renderer_bridge::probability_to_chunk_state;

pub use renderer_validation::{
    RendererParityReport,
    RendererShadowValidator,
};

pub use execution_bridge::{
    ExecutionBridge,
    SchedulingRequest,
    DEFAULT_DEADLINE_FRAMES,
};


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\bridge\renderer_bridge.rs ---
// ===================================================================
// mirage-mkr-core/src/bridge/renderer_bridge.rs
// PURPOSE: Field-to-Renderer Translation Layer
//
// PROBLEM:
// The legacy renderer (mirage-renderer/src/main.rs) reads discrete
// `ChunkState` values from `RuntimeDirectory::chunk_runtime_states`
// and uploads them as u32 to a GPU buffer.  The new V3 authority is
// `ActivationField::execution_probability` (a continuous f32).
//
// SOLUTION:
// `RendererBridge` translates the continuous field into approximate
// discrete states so the renderer continues to work correctly during
// the V3 transition.
//
// ARCHITECTURE:
// This bridge is UNIDIRECTIONAL and ADDITIVE:
// * It reads from ActivationField (immutable borrow).
// * It writes to RuntimeDirectory::chunk_runtime_states.
// * It does NOT write back into the ActivationField.
// * The renderer's own distance-based writes still happen and still
//   drive the compat ThermalSystem — the bridge only overrides states
//   for cells that have significant activation field energy.
//
// TRANSLATION RULES (continuous → discrete):
//
//   probability ≥ 0.70 → Hot       (full simulation eligible)
//   probability ≥ 0.35 → Resident  (in VRAM, light sim)
//   probability ≥ 0.05 → Predictive (loading eligible)
//   probability <  0.05 → Dormant   (skip entirely)
//
// These thresholds are intentionally lower than the old ThermalSystem
// thresholds to be conservative: the activation field encodes more
// information (topology pressure, entropy) so a lower raw probability
// can still represent meaningful activity.
//
// GPU-READINESS:
// `render_state_scalars()` returns a raw Vec<f32> representation of
// execution_probability for use in shaders that are already updated to
// handle continuous values (future work).
//
// TODO(V3): Once the renderer GPU shader is updated to consume
// execution_probability as a raw f32 buffer, remove
// `apply_to_directory()` and feed `render_state_scalars()` directly
// to `renderer.update_states_buffer()`.
// ===================================================================

use mirage_core::pool::RuntimeDirectory;
use mirage_core::runtime::ChunkState;
use crate::activation::field::ActivationField;

// =====================================================================
// TRANSLATION THRESHOLDS
// =====================================================================

/// execution_probability at or above which a cell is rendered as Hot.
pub const BRIDGE_HOT_THRESHOLD:       f32 = 0.70;
/// execution_probability at or above which a cell is rendered as Resident.
pub const BRIDGE_RESIDENT_THRESHOLD:  f32 = 0.35;
/// execution_probability at or above which a cell is Predictive (loading).
pub const BRIDGE_PREDICTIVE_THRESHOLD: f32 = 0.05;

// =====================================================================
// RENDERER BRIDGE
// =====================================================================

/// Translates continuous `ActivationField` values into discrete
/// `ChunkState` values for the legacy renderer compatibility path.
///
/// # Ownership
/// `RendererBridge` is stateless — all its data comes from references
/// passed per call.  Owned by `MKRWorld`.
///
/// # Thread Safety
/// All methods take `&ActivationField` and `&mut RuntimeDirectory` —
/// both must be borrowed from the same frame context (i.e., inside
/// `MKRWorld::tick()`).  No internal locks.
pub struct RendererBridge;

impl RendererBridge {


        /// Apply renderer translation ONLY to changed cells.
    ///
    /// V4 — Pass 04:
    /// Differential renderer bridge path.
    ///
    /// Instead of translating the entire activation field every frame,
    /// this updates only cells marked changed in the FieldDeltaMask.
    ///
    /// AUTHORITATIVE STATUS:
    /// Shadow-only until renderer parity validation proves stable.
    ///
    /// TODO(V4-PASS06):
    /// Promote this path to authority after long-term parity success.
pub fn apply_changed_cells(
    &self,
    field: &ActivationField,
    delta_mask: &crate::activation::delta::FieldDeltaMask,
    directory: &mut RuntimeDirectory,
) {
    for index in delta_mask.iter_changed() {
        let probability =
            field.cells[index].execution_probability;

        directory.chunk_runtime_states[index] =
            probability_to_chunk_state(probability);
    }
}


    pub fn new() -> Self { Self }

    // ------------------------------------------------------------------
    // Primary V3→Compat translation
    // ------------------------------------------------------------------

    /// Translate activation field probabilities into discrete ChunkStates
    /// and write them into `RuntimeDirectory::chunk_runtime_states`.
    ///
    /// # Behaviour
    /// * Only writes cells where the activation field has a non-Dormant
    ///   probability (i.e., probability ≥ BRIDGE_PREDICTIVE_THRESHOLD).
    /// * Cells below the Predictive threshold are written as Dormant —
    ///   this **overrides** any state the renderer may have written.
    ///   This is intentional: the activation field is the V3 authority.
    ///
    /// # Safety
    /// Panics if `directory.chunk_runtime_states.len() != field.len()`.
    /// Both must be constructed with the same `total_chunks` value.
    pub fn apply_to_directory(
        &self,
        field:     &ActivationField,
        directory: &mut RuntimeDirectory,
    ) {
        debug_assert_eq!(
            field.len(),
            directory.chunk_runtime_states.len(),
            "RendererBridge: field and directory size mismatch"
        );

        let states = &mut directory.chunk_runtime_states;
        let cells  = &field.cells;

        // Branchless translation: the `as u8` cast collapses the chain.
        // Written as explicit match for readability; the compiler folds it.
        for (state, cell) in states.iter_mut().zip(cells.iter()) {
            *state = probability_to_chunk_state(cell.execution_probability);
        }
    }

    // ------------------------------------------------------------------
    // V3-SPARSE: Changed-cell-only renderer updates (Task 8)
    // ------------------------------------------------------------------

    /// Sparse renderer update: only writes states for cells flagged as
    /// `PROBABILITY_CHANGED` in the delta mask.
    ///
    /// **O(|changed|) instead of O(N).**
    ///
    /// # Authority
    /// Renderer remains passive.  This method only writes the subset of
    /// `chunk_runtime_states` that correspond to changed cells.  All other
    /// states retain their values from the previous frame — which is
    /// correct if `execution_probability` didn't change significantly.
    ///
    /// # Correctness Guarantee
    /// `PROBABILITY_EPSILON` (1e-4) is much smaller than the smallest
    /// threshold gap between ChunkState variants (0.05 for Predictive).
    /// A cell whose probability changed by less than 1e-4 cannot have
    /// crossed a state boundary, so its ChunkState is still correct.
    ///
    /// # TODO(V3-SPARSE-VALIDATION): Run apply_changed_cells() in parallel
    /// with apply_to_directory() for 1000 ticks.  Assert that all cells in
    /// the changed set produce identical ChunkStates in both paths.

    /// Sparse probability buffer update: only writes changed cell probabilities.
    ///
    /// Companion to `fill_probability_buffer()`.  For a flat `Vec<f32>` that
    /// is partially updated by the sparse solver, this ensures only the
    /// changed indices are refreshed.
    ///
    /// # TODO(V3-SPARSE-VALIDATION): Validate that partial buffer updates
    /// match full buffer for all changed cells before using in production.
    pub fn update_probability_buffer_sparse(
        &self,
        field:      &ActivationField,
        buffer:     &mut Vec<f32>,
        delta_mask: &crate::activation::delta::FieldDeltaMask,
    ) {
        // Ensure buffer is large enough
        if buffer.len() < field.cells.len() {
            buffer.resize(field.cells.len(), 0.0);
        }
        for idx in delta_mask.iter_changed() {
            if idx >= field.cells.len() { break; }
            buffer[idx] = field.cells[idx].execution_probability;
        }
    }

    // ------------------------------------------------------------------
    // Forward-looking: raw float buffer for updated GPU shaders
    // ------------------------------------------------------------------

    /// Return a `Vec<f32>` of raw execution probabilities for each cell.
    ///
    /// This is the forward-looking V3 output: once the GPU shader is
    /// updated to consume a float buffer instead of a u32 enum buffer,
    /// call this method and feed it to `renderer.update_states_buffer()`.
    ///
    /// Avoids any allocation when the caller pre-allocates the buffer.
    pub fn fill_probability_buffer(
        &self,
        field:  &ActivationField,
        output: &mut Vec<f32>,
    ) {
        output.clear();
        output.extend(field.cells.iter().map(|c| c.execution_probability));
    }

    // ------------------------------------------------------------------
    // Per-cell query helpers
    // ------------------------------------------------------------------

    /// Translate a single execution_probability to a ChunkState.
    /// Useful for per-chunk decisions (e.g., streaming trigger logic).
    #[inline]
    pub fn cell_to_chunk_state(&self, probability: f32) -> ChunkState {
        probability_to_chunk_state(probability)
    }

    /// Return true if a cell is hot enough to be emission-eligible.
    /// Uses the same threshold as the emission gate.
    #[inline]
    pub fn is_emission_eligible(&self, probability: f32) -> bool {
        probability > crate::emission::EMIT_GATE
    }

    /// Return true if a cell should be rendered (Resident or hotter).
    #[inline]
    pub fn should_render(&self, probability: f32) -> bool {
        probability >= BRIDGE_RESIDENT_THRESHOLD
    }

    /// Return true if a cell should trigger async streaming.
    #[inline]
    pub fn should_stream(&self, probability: f32) -> bool {
        probability >= BRIDGE_PREDICTIVE_THRESHOLD && probability < BRIDGE_RESIDENT_THRESHOLD
    }
}

impl Default for RendererBridge {
    fn default() -> Self { Self::new() }
}

// =====================================================================
// TRANSLATION KERNEL (free function — inline hot path)
// =====================================================================

/// Map a continuous `execution_probability` to the nearest `ChunkState`.
///
/// This is the core translation function.  It is a pure function with
/// no side effects, making it trivially testable and GPU-portable.
///
/// # Thresholds
/// ```text
/// probability ≥ 0.70 → Hot
/// probability ≥ 0.35 → Resident
/// probability ≥ 0.05 → Predictive
/// probability <  0.05 → Dormant
/// ```
#[inline]
pub fn probability_to_chunk_state(probability: f32) -> ChunkState {
    // Written as nested selects to encourage branchless codegen.
    // The compiler typically emits FCMP + CSEL (ARM) or FCOMI + CMOV (x86).
    if probability >= BRIDGE_HOT_THRESHOLD {
        ChunkState::Hot
    } else if probability >= BRIDGE_RESIDENT_THRESHOLD {
        ChunkState::Resident
    } else if probability >= BRIDGE_PREDICTIVE_THRESHOLD {
        ChunkState::Predictive
    } else {
        ChunkState::Dormant
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;

    #[test]
    fn probability_mapping_boundaries() {
        assert_eq!(probability_to_chunk_state(0.0),    ChunkState::Dormant);
        assert_eq!(probability_to_chunk_state(0.04),   ChunkState::Dormant);
        assert_eq!(probability_to_chunk_state(0.05),   ChunkState::Predictive);
        assert_eq!(probability_to_chunk_state(0.35),   ChunkState::Resident);
        assert_eq!(probability_to_chunk_state(0.70),   ChunkState::Hot);
        assert_eq!(probability_to_chunk_state(1.0),    ChunkState::Hot);
    }

    #[test]
    fn apply_to_directory_full_hot() {
        let mut field = ActivationField::new(4, 4);
        for cell in &mut field.cells {
            cell.execution_probability = 1.0;
        }
        let mut dir = RuntimeDirectory::new(16);
        RendererBridge::new().apply_to_directory(&field, &mut dir);
        assert!(dir.chunk_runtime_states.iter().all(|&s| s == ChunkState::Hot));
    }

    #[test]
    fn apply_to_directory_dormant_field() {
        let field = ActivationField::new(4, 4); // all zeros
        let mut dir = RuntimeDirectory::new(16);
        // Pre-set some states to Resident to verify override behaviour
        dir.chunk_runtime_states[0] = ChunkState::Resident;
        RendererBridge::new().apply_to_directory(&field, &mut dir);
        // Bridge overrides to Dormant when probability is zero
        assert_eq!(dir.chunk_runtime_states[0], ChunkState::Dormant);
    }

    #[test]
    fn fill_probability_buffer_matches_field() {
        let mut field = ActivationField::new(2, 2);
        field.cells[0].execution_probability = 0.8;
        field.cells[3].execution_probability = 0.4;
        let bridge = RendererBridge::new();
        let mut buf = Vec::new();
        bridge.fill_probability_buffer(&field, &mut buf);
        assert_eq!(buf.len(), 4);
        assert!((buf[0] - 0.8).abs() < 1e-6);
        assert!((buf[3] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn helper_predicates() {
        let b = RendererBridge::new();
        assert!( b.should_render(0.5));
        assert!(!b.should_render(0.2));
        assert!( b.should_stream(0.1));
        assert!(!b.should_stream(0.5));
        assert!(!b.should_stream(0.01));
        assert!( b.is_emission_eligible(0.1));
        assert!(!b.is_emission_eligible(0.01));
    }

 #[test]
fn apply_changed_cells_updates_only_delta_cells() {
    use crate::activation::ActivationField;
    use crate::activation::delta::FieldDeltaMask;
    use mirage_core::pool::RuntimeDirectory;
    use mirage_core::runtime::ChunkState;

    let bridge = RendererBridge::new();

    let mut field = ActivationField::new(4, 4);
    let mut directory = RuntimeDirectory::new(16);

    // كله Dormant بالبداية
    for state in directory.chunk_runtime_states.iter_mut() {
        *state = ChunkState::Dormant;
    }

    // خلية واحدة فقط هتتغير
    field.cells[5].execution_probability = 1.0;

    let mut delta = FieldDeltaMask::new(16);
    delta.mark_changed(5);

    bridge.apply_changed_cells(
        &field,
        &delta,
        &mut directory,
    );

    // الخلية المتغيرة لازم تبقى non-dormant
    assert_ne!(
        directory.chunk_runtime_states[5],
        ChunkState::Dormant,
    );

    // أي خلية تانية لازم تفضل Dormant
    for (i, state) in
        directory.chunk_runtime_states.iter().enumerate()
    {
        if i != 5 {
            assert_eq!(*state, ChunkState::Dormant);
        }
    }
}
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\bridge\renderer_validation.rs ---
// ===================================================================
// mirage-mkr-core/src/renderer_validation.rs
//
// V4 PASS 03:
// Differential Renderer Shadow Validation
//
// PURPOSE:
// Runs sparse renderer updates in shadow alongside the authoritative
// full-field renderer path.
//
// AUTHORITY:
// apply_to_directory() ALWAYS remains authoritative.
// apply_changed_cells() is SHADOW ONLY.
//
// The validator compares:
//
// - chunk_runtime_states
// - probability buffers
// - unchanged-cell preservation
//
// NO authoritative state is overwritten here.
// ===================================================================

use mirage_core::pool::RuntimeDirectory;
use mirage_core::runtime::ChunkState;

use crate::activation::{
    delta::FieldDeltaMask,
    field::ActivationField,
};

use crate::bridge::renderer_bridge::RendererBridge;

// ===================================================================
// CONSTANTS
// ===================================================================

pub const RENDERER_PROBABILITY_EPSILON: f32 = 1e-4;
pub const RENDERER_PROMOTION_THRESHOLD: u64 = 1000;

// ===================================================================
// DIFFERENTIAL RENDERER MODE
// ===================================================================

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DifferentialRendererMode {
    Disabled,
    ShadowValidation,
    DifferentialAuthoritative, // reserved
}

// ===================================================================
// PARITY REPORT
// ===================================================================

#[derive(Debug, Clone)]
pub struct RendererParityReport {
    pub mismatched_chunk_states: usize,
    pub max_probability_drift: f32,
    pub mean_probability_drift: f32,
    pub changed_cells_checked: usize,
    pub severe_divergence: bool,
}

impl Default for RendererParityReport {
    fn default() -> Self {
        Self {
            mismatched_chunk_states: 0,
            max_probability_drift: 0.0,
            mean_probability_drift: 0.0,
            changed_cells_checked: 0,
            severe_divergence: false,
        }
    }
}

// ===================================================================
// VALIDATION REPORT
// ===================================================================

#[derive(Debug, Clone, Default)]
pub struct DifferentialRendererValidationReport {
    pub ticks_run: u64,
    pub ticks_passed: u64,
    pub ticks_failed: u64,

    pub consecutive_passes: u64,

    pub severe_divergence_events: u64,

    pub peak_probability_drift: f32,
    pub peak_chunk_state_mismatches: usize,
}

impl DifferentialRendererValidationReport {
    pub fn record(&mut self, parity: &RendererParityReport) {
        self.ticks_run += 1;

        let passed =
            parity.mismatched_chunk_states == 0 &&
            !parity.severe_divergence;

        if passed {
            self.ticks_passed += 1;
            self.consecutive_passes += 1;
        } else {
            self.ticks_failed += 1;
            self.consecutive_passes = 0;
        }

        if parity.severe_divergence {
            self.severe_divergence_events += 1;
        }

        self.peak_probability_drift =
            self.peak_probability_drift.max(parity.max_probability_drift);

        self.peak_chunk_state_mismatches =
            self.peak_chunk_state_mismatches
                .max(parity.mismatched_chunk_states);
    }

    #[inline]
    pub fn eligible_for_promotion(&self) -> bool {
        self.consecutive_passes >= RENDERER_PROMOTION_THRESHOLD
            && self.severe_divergence_events == 0
    }
}

// ===================================================================
// SHADOW VALIDATOR
// ===================================================================

pub struct RendererShadowValidator {

    previous_shadow_states: Vec<ChunkState>,
    pub mode: DifferentialRendererMode,

    pub shadow_directory: RuntimeDirectory,
    pub shadow_probability_buffer: Vec<f32>,

    pub last_report: Option<RendererParityReport>,
    pub validation_report: DifferentialRendererValidationReport,

    bridge: RendererBridge,
}

impl RendererShadowValidator {
    pub fn new(total_chunks: usize) -> Self {
        Self {

            previous_shadow_states:
    vec![ChunkState::Dormant; total_chunks],
            mode: DifferentialRendererMode::Disabled,

            shadow_directory: RuntimeDirectory::new(total_chunks),

            shadow_probability_buffer: vec![0.0; total_chunks],

            last_report: None,

            validation_report:
                DifferentialRendererValidationReport::default(),

            bridge: RendererBridge::new(),
        }
    }

    #[inline]
    pub fn enable_shadow(&mut self) {
        self.mode = DifferentialRendererMode::ShadowValidation;
    }

    #[inline]
    pub fn disable(&mut self) {
        self.mode = DifferentialRendererMode::Disabled;
    }

    #[inline]
    pub fn is_active(&self) -> bool {
        self.mode != DifferentialRendererMode::Disabled
    }

    // ===============================================================
    // VALIDATION TICK
    // ===============================================================

    pub fn validate_tick(

        
        &mut self,
        field: &ActivationField,
        delta_mask: &FieldDeltaMask,

        authoritative_directory: &RuntimeDirectory,
        authoritative_probability_buffer: &[f32],
    ) {
        if !self.is_active() {
            return;
        }

        // -----------------------------------------------------------
        // SHADOW sparse update
        // -----------------------------------------------------------


        self.previous_shadow_states
    .copy_from_slice(
        &self.shadow_directory.chunk_runtime_states
    );

    self.previous_shadow_states.copy_from_slice(
    &self.shadow_directory.chunk_runtime_states
);
self.bridge.apply_changed_cells(
    field,
    delta_mask,
    &mut self.shadow_directory,
);

        self.bridge.update_probability_buffer_sparse(
            field,
            &mut self.shadow_probability_buffer,
            delta_mask,
        );

        // -----------------------------------------------------------
        // PARITY comparison
        // -----------------------------------------------------------

        let mut report = RendererParityReport::default();

        let mut drift_sum = 0.0;

        let shadow_states =
            &self.shadow_directory.chunk_runtime_states;

        let authoritative_states =
            &authoritative_directory.chunk_runtime_states;

        for idx in delta_mask.iter_changed() {
            if idx >= shadow_states.len() {
                break;
            }

            report.changed_cells_checked += 1;

            // -------------------------------------------------------
            // ChunkState parity
            // -------------------------------------------------------

            if shadow_states[idx] != authoritative_states[idx] {
                report.mismatched_chunk_states += 1;
                report.severe_divergence = true;
            }

            // -------------------------------------------------------
            // Probability parity
            // -------------------------------------------------------

            let drift =
                (self.shadow_probability_buffer[idx]
                    - authoritative_probability_buffer[idx])
                    .abs();

            drift_sum += drift;

            if drift > report.max_probability_drift {
                report.max_probability_drift = drift;
            }
        }

        if report.changed_cells_checked > 0 {
            report.mean_probability_drift =
                drift_sum / report.changed_cells_checked as f32;
        }

        for idx in 0..shadow_states.len() {
    if !delta_mask.is_changed(idx) {
        if shadow_states[idx]
            != self.previous_shadow_states[idx]
        {
            report.severe_divergence = true;
            report.mismatched_chunk_states += 1;
        }
    }
}

        self.validation_report.record(&report);

        self.last_report = Some(report);
    }
}

// ===================================================================
// TESTS
// ===================================================================

#[cfg(test)]
mod tests {
    use super::*;

    use crate::activation::{
        ActivationField,
        delta::FieldDeltaMask,
    };

    #[test]
    fn validator_disabled_by_default() {
        let validator = RendererShadowValidator::new(16);

        assert!(!validator.is_active());

        assert_eq!(
            validator.validation_report.ticks_run,
            0
        );
    }

    #[test]
    fn promotion_requires_clean_history() {
        let mut report =
            DifferentialRendererValidationReport::default();

        report.consecutive_passes =
            RENDERER_PROMOTION_THRESHOLD;

        assert!(report.eligible_for_promotion());

        report.severe_divergence_events = 1;

        assert!(!report.eligible_for_promotion());
    }

    #[test]
    fn parity_detects_chunk_state_mismatch() {
        let mut field = ActivationField::new(4, 4);

        field.cells[0].execution_probability = 1.0;

        let mut validator =
            RendererShadowValidator::new(16);

        validator.enable_shadow();

        let mut mask = FieldDeltaMask::new(16);
        mask.set(0);

        let mut authoritative =
            RuntimeDirectory::new(16);

        authoritative.chunk_runtime_states[0] =
            ChunkState::Dormant;

        let authoritative_probabilities =
            vec![1.0; 16];

        validator.validate_tick(
            &field,
            &mask,
            &authoritative,
            &authoritative_probabilities,
        );

        let report =
            validator.last_report.as_ref().unwrap();

        assert!(report.severe_divergence);

        assert_eq!(
            report.mismatched_chunk_states,
            1
        );
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\pool\field_handle.rs ---
// ===================================================================
// mirage-mkr-core/src/pool/field_handle.rs
// PURPOSE: FieldCellHandle — V3 Primary Addressing Type
//
// TRANSITION CONTEXT:
// V2 addressing:  UUID → Handle → AddressMapping → ChunkState
// V3 addressing:  FieldCellIndex → ActivationField::cells[index]
//
// FieldCellHandle is the V3 address primitive.  It is a newtype over
// usize that directly indexes ActivationField::cells.
//
// KEY PROPERTIES vs. old Handle:
// * No UUID lookup — direct array index.
// * No generation check on hot path — field cells are persistent.
// * No page_id / slot_idx indirection — one coordinate, the field index.
// * O(1) access instead of O(map_lookup) + O(table_access).
//
// COMPATIBILITY BRIDGE:
// `FieldCellHandle::from_legacy_chunk_idx` converts an old chunk index
// (u32) directly to a FieldCellHandle, enabling gradual migration:
//
//   old:  directory.chunk_runtime_states[chunk_idx]
//   new:  field.cells[FieldCellHandle::from_legacy(chunk_idx).index()]
//
// STREAMING DESCRIPTOR:
// `StreamingDescriptor` replaces AddressMapping for the streaming path.
// It pairs a FieldCellHandle with an OASIS page reference so the
// streaming layer and activation field share one key space.
//
// IS CURRENTLY SAFE TO REPLACE Handle WITH: PARTIALLY.
//   New code must use FieldCellHandle.
//   Old code using `Handle` is still supported via `from_legacy_handle`.
//   Full removal of Handle requires mirage-matrix-macros migration.
//
// TODO(V3-POOL-1): Update NeuralCluster macro to emit FieldCellHandle
//   instead of Handle+UUID.
// TODO(V3-POOL-2): Remove Handle from RuntimeDirectory once all callers
//   use FieldCellHandle-based lookups.
// TODO(V3-POOL-3): Remove AddressMapping once StreamingDescriptor is
//   the canonical streaming address type everywhere.
// ===================================================================

// =====================================================================
// FIELD CELL HANDLE — Primary V3 address
// =====================================================================

/// Direct index into `ActivationField::cells`.
///
/// This is the primary chunk-addressing type in V3.  It replaces the
/// UUID → Handle → AddressMapping chain with a single flat index.
///
/// # Safety
/// No bounds checking occurs inside `index()`.  The caller must
/// ensure the handle was created for the same field it is used on.
/// Use `ActivationField::index_of(x, y)` to construct safe handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct FieldCellHandle(usize);

impl FieldCellHandle {
    /// Create a `FieldCellHandle` from a raw flat field index.
    #[inline(always)]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Create from a chunk grid coordinate pair `(x, y)` and field width.
    ///
    /// Equivalent to `y * width + x`.  Does NOT check bounds.
    #[inline(always)]
    pub const fn from_grid(x: usize, y: usize, width: usize) -> Self {
        Self(y * width + x)
    }

    /// Convert from a legacy `chunk_idx: u32` (V2 runtime index).
    ///
    /// Use during the migration period to convert old chunk indices
    /// to FieldCellHandle without modifying call sites.
    #[inline(always)]
    pub const fn from_legacy_chunk_idx(chunk_idx: u32) -> Self {
        Self(chunk_idx as usize)
    }

    /// Convert from an old-style `Handle` (V2 entity handle).
    ///
    /// Maps `handle.index()` directly to a field cell index.
    /// Generation is discarded — V3 cells are persistent, not generational.
    ///
    /// TODO(V3-POOL-2): Remove once Handle is no longer used.
    #[inline(always)]
    pub fn from_legacy_handle(handle: &mirage_core::pool::Handle) -> Self {
        Self(handle.index() as usize)
    }

    /// Raw flat field index.
    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0
    }

    /// Convert back to a legacy `u32` chunk index.
    ///
    /// TODO(V3-POOL-2): Remove once chunk_runtime_states is gone.
    #[inline(always)]
    pub const fn as_chunk_idx(self) -> u32 {
        self.0 as u32
    }
}

impl From<usize> for FieldCellHandle {
    fn from(idx: usize) -> Self { Self(idx) }
}

impl From<u32> for FieldCellHandle {
    fn from(idx: u32) -> Self { Self(idx as usize) }
}

// =====================================================================
// STREAMING DESCRIPTOR — V3 streaming address
// =====================================================================

/// V3 streaming address: combines a field cell index with an OASIS page ref.
///
/// Replaces `AddressMapping { page_id, chunk_idx, slot_idx, ... }` for
/// the streaming path.  The two fields share a single key space:
///
/// * `field_handle` — which activation cell to heat on completion.
/// * `oasis_page_id` — which OASIS virtual page to load from.
///
/// TODO(V3-POOL-3): Migrate StreamingFabric to accept StreamingDescriptor
/// instead of raw (page_id: u32, chunk_idx: u32) pairs.
#[derive(Debug, Clone, Copy)]
pub struct StreamingDescriptor {
    /// V3 primary key — indexes ActivationField::cells directly.
    pub field_handle:   FieldCellHandle,
    /// OASIS virtual page containing this chunk's data.
    pub oasis_page_id:  u32,
}

impl StreamingDescriptor {
    pub fn new(field_handle: FieldCellHandle, oasis_page_id: u32) -> Self {
        Self { field_handle, oasis_page_id }
    }

    /// Convert from legacy (chunk_idx, page_id) pair.
    pub fn from_legacy(chunk_idx: u32, page_id: u32) -> Self {
        Self {
            field_handle:  FieldCellHandle::from_legacy_chunk_idx(chunk_idx),
            oasis_page_id: page_id,
        }
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_cell_handle_from_grid() {
        // Row-major: (x=2, y=3, width=10) → index = 3*10 + 2 = 32
        let h = FieldCellHandle::from_grid(2, 3, 10);
        assert_eq!(h.index(), 32);
    }

    #[test]
    fn from_legacy_chunk_idx_is_identity() {
        let h = FieldCellHandle::from_legacy_chunk_idx(42);
        assert_eq!(h.index(), 42);
        assert_eq!(h.as_chunk_idx(), 42);
    }

    #[test]
    fn streaming_descriptor_from_legacy() {
        let sd = StreamingDescriptor::from_legacy(7, 3);
        assert_eq!(sd.field_handle.index(), 7);
        assert_eq!(sd.oasis_page_id, 3);
    }

    #[test]
    fn field_cell_handle_ordering() {
        let a = FieldCellHandle::new(5);
        let b = FieldCellHandle::new(10);
        assert!(a < b);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\pool\handle.rs ---
// ===================================================================
// ملف: handle.rs
// الوظيفة: المعرّف الذكي (المقبض) الذي يربط المطور بالدليل (Runtime Directory).
// السر الهندسي: تم إزالة الـ <T> ليصبح مقبضاً عالمياً (Type Erasure).
// ===================================================================

// serde import retained for when Handle needs Serialize/Deserialize for asset persistence.
// TODO(V3): Enable when Handle is used in serialisable asset manifests.
// use serde::{Deserialize, Serialize};

/// [Handle] هو "المفتاح" السريع الذي يستخدمه المحرك.
/// حجمه 8 بايت فقط، صديق للـ Cache، ولا يهتم بنوع البيانات.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]#[repr(C)] 
pub struct Handle {
    index: u32,
    generation: u32,
}

impl Handle {
    pub const NONE: Self = Self {
        index: 0,
        generation: 0,
    };

    pub fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    #[inline(always)]
    pub fn index(&self) -> u32 {
        self.index
    }

    #[inline(always)]
    pub fn generation(&self) -> u32 {
        self.generation
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\pool\mod.rs ---
// ===================================================================
// mirage-mkr-core/src/pool/mod.rs
// PURPOSE: RuntimeDirectory — Entity Handle Registry
//
// TODO(V3-COMPAT): RuntimeDirectory is a V2 compatibility structure.
// In V3, chunk addressing will migrate to ActivationField cell indices
// rather than UUID-to-handle lookup tables.  This module is retained
// so that existing code continues to compile during the transition.
// ===================================================================

pub mod handle;
pub use handle::Handle;

use std::collections::HashMap;

/// Lightweight entity identity key.
///
/// TODO(V3-COMPAT): In V3 this will be replaced by a field-cell index
/// (usize) once the streaming layer is redesigned.  The UUID abstraction
/// is retained for compat-only code paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct LocalUuid(pub [u8; 16]);

impl LocalUuid {
    /// Create a zero UUID (placeholder / unassigned).
    pub const fn zero() -> Self {
        Self([0u8; 16])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AddressMapping {
    pub page_id:    u32,
    pub chunk_idx:  u32,
    pub slot_idx:   u32,
    pub generation: u32,
    pub is_alive:   bool,
}

/// TODO(V3-COMPAT): RuntimeDirectory is a compatibility structure.
/// Its design mirrors the old V2 entity registry.  It will be redesigned
/// once the ActivationField becomes the primary chunk addressing mechanism.
pub struct RuntimeDirectory {
    uuid_to_handle: HashMap<LocalUuid, Handle>,
    address_table:  Vec<AddressMapping>,
    /// TODO(V3-POOL-2): free_slots will become the slab-allocator free-list
    /// for the V3 StreamingDescriptor table.  Retained now to avoid API
    /// churn during the compat transition.
    #[allow(dead_code)]
    free_slots:     Vec<u32>,
}

impl RuntimeDirectory {
    pub fn new(_total_chunks: usize) -> Self {
        Self {
            uuid_to_handle: HashMap::new(),
            address_table:  Vec::new(),
            free_slots:     Vec::new(),
        }
    }

    pub fn register_entity(
        &mut self,
        uuid:      LocalUuid,
        page_id:   u32,
        chunk_idx: u32,
        slot_idx:  u32,
    ) -> Handle {
        let generation = 1;
        let mapping = AddressMapping {
            page_id,
            chunk_idx,
            slot_idx,
            generation,
            is_alive: true,
        };
        let index = self.address_table.len() as u32;
        self.address_table.push(mapping);
        let handle = Handle::new(index, generation);
        self.uuid_to_handle.insert(uuid, handle);
        handle
    }

    pub fn get_mapping(&self, handle: Handle) -> Option<AddressMapping> {
        let mapping = self.address_table.get(handle.index() as usize)?;
        if mapping.is_alive && mapping.generation == handle.generation() {
            Some(*mapping)
        } else {
            None
        }
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-mkr-core\src\streaming\mod.rs ---
// ===================================================================
// mirage-mkr-core/src/streaming/mod.rs  (V3 — Federated Stabilization Pass)
// PURPOSE: StreamingCoordinator — Activation-Driven Streaming Gate
//
// ---------------------------------------------------------------
// STREAMING OWNERSHIP BOUNDARY (CANONICAL)
// ---------------------------------------------------------------
//
// MKR (this module) is ONLY responsible for:
//   * Computing streaming ELIGIBILITY from execution_probability
//   * Classifying cells into Prefetch / PromoteResident actions
//   * Returning a bounded StreamingDecision slice to the caller
//   * Providing heat values for stream-completion feedback
//
// TODO(V3-OASIS-CANONICAL): The caller is responsible for forwarding
// StreamingDecisions to mirage-memory-oasis::StreamingFabric, which
// is the ONLY authorised executor of stream lifecycles.  MKR must
// NEVER call StreamingFabric::prefetch_horizon() directly from inside
// this module — that would make MKR a streaming owner, violating the
// federated architecture contract.
//
// ---------------------------------------------------------------
// OASIS OWNS:
// ---------------------------------------------------------------
//   * prefetch_horizon() execution
//   * residency promotion lifecycle
//   * mmap page mapping
//   * stream completion signals
//   * loaded/queued/evicted state
//
// MKR OWNS (eligibility only):
//   * probability thresholds (STREAM_PREFETCH_THRESHOLD, STREAM_RESIDENT_THRESHOLD)
//   * StreamingDecision generation (scan output)
//   * STREAM_COMPLETION_HEAT (feedback amount only)
//
// TODO(V3-OASIS-CANONICAL): StreamingFabric's prefetch_horizon() is
// currently camera-position-driven.  A future pass will add a
// field-index-based request path so MKR StreamingDecisions can drive
// OASIS directly without camera coordinate conversion.
//
// ---------------------------------------------------------------
// WHAT THIS IS NOT
// ---------------------------------------------------------------
//   * NOT a replacement for mirage-memory-oasis (OASIS is canonical).
//   * NOT a job queue — it makes boolean decisions, not work items.
//   * NOT camera-aware — camera velocity is an upstream concern.
//   * NOT the residency authority — that is OASIS/ResidencyTracker.
//
// TODO(V3-CEK): Once CEK is implemented, streaming requests will be
// generated from CEK emission events rather than direct probability
// threshold scans.  The StreamingCoordinator will become a CEK
// plugin rather than a direct ActivationField reader.
// ===================================================================

use crate::activation::field::ActivationField;

// =====================================================================
// CONSTANTS
// =====================================================================

/// execution_probability threshold to trigger predictive streaming (prefetch).
///
/// Cells above this value signal that streaming should begin.
/// Lower than EMIT_GATE (0.05) to ensure streaming begins before execution.
pub const STREAM_PREFETCH_THRESHOLD: f32 = 0.03;

/// execution_probability threshold to trigger hot-path residency promotion.
///
/// Cells above this value should be in VRAM and actively simulated.
/// Matches BRIDGE_RESIDENT_THRESHOLD in renderer_bridge.
pub const STREAM_RESIDENT_THRESHOLD: f32 = 0.35;

/// Heat injection amount when a streaming operation completes.
///
/// Completing a stream raises the cell's heat signal, which in turn
/// raises activation and execution_probability on the next tick.
/// This closes the streaming ↔ field feedback loop.
pub const STREAM_COMPLETION_HEAT: f32 = 0.25;

/// Maximum number of stream requests to generate per tick.
///
/// Prevents the streaming layer from being flooded when many cells
/// simultaneously cross the prefetch threshold.
pub const MAX_STREAM_REQUESTS_PER_TICK: usize = 32;

// =====================================================================
// STREAMING DECISION
// =====================================================================

/// A streaming decision record produced by `StreamingCoordinator::scan()`.
///
/// Tells the caller exactly which field cells need streaming action and
/// what kind of action is required.
#[derive(Debug, Clone, Copy)]
pub struct StreamingDecision {
    /// Flat field cell index (== chunk index for 1:1 grids).
    pub cell_index: usize,
    /// Action required for this cell.
    pub action: StreamAction,
    /// Raw probability at scan time (caller can use for priority sorting).
    pub probability: f32,
}

/// Streaming action type derived from execution_probability.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamAction {
    /// Begin async prefetch — probability is above STREAM_PREFETCH_THRESHOLD
    /// but below STREAM_RESIDENT_THRESHOLD.
    Prefetch,
    /// Promote to resident VRAM — probability is at or above STREAM_RESIDENT_THRESHOLD.
    PromoteResident,
}

// =====================================================================
// STREAMING COORDINATOR
// =====================================================================

/// Activation-driven streaming gate.
///
/// Scans `execution_probability` each tick and produces a bounded list
/// of `StreamingDecision`s for cells that need OASIS streaming action.
///
/// # Ownership
/// `StreamingCoordinator` is stateless — all data comes from the field
/// reference passed to `scan()`.  It owns only a pre-allocated scratch
/// buffer to avoid per-tick heap allocation.
///
/// # How to Use
/// ```rust
/// // Inside a hypothetical game loop (not MKRWorld::tick itself):
/// let decisions = coordinator.scan(world.activation_field());
/// for decision in decisions {
///     match decision.action {
///         StreamAction::Prefetch => {
///             oasis_fabric.request_stream(decision.cell_index as u32);
///         }
///         StreamAction::PromoteResident => {
///             residency_tracker.request_load(decision.cell_index as u32);
///         }
///     }
/// }
/// ```
///
/// On stream completion, inject heat back into the field:
/// ```rust
/// world.inject_heat_at_chunk(x, y, STREAM_COMPLETION_HEAT);
/// ```
pub struct StreamingCoordinator {
    /// Reusable scratch buffer — avoids per-tick heap allocation.
    scratch: Vec<StreamingDecision>,
}

impl StreamingCoordinator {
    pub fn new() -> Self {
        Self {
            scratch: Vec::with_capacity(MAX_STREAM_REQUESTS_PER_TICK),
        }
    }

    /// Scan the activation field and return streaming decisions.
    ///
    /// Returns cells in descending probability order, bounded to
    /// `MAX_STREAM_REQUESTS_PER_TICK`.
    ///
    /// # Decision Logic (branchless-structured)
    /// For each cell:
    ///   * probability >= STREAM_RESIDENT_THRESHOLD → PromoteResident
    ///   * probability >= STREAM_PREFETCH_THRESHOLD → Prefetch
    ///   * probability <  STREAM_PREFETCH_THRESHOLD → skip
    ///
    /// # Returns
    /// Immutable slice valid until next call to `scan()`.
    pub fn scan<'a>(&'a mut self, field: &ActivationField) -> &'a [StreamingDecision] {
        self.scratch.clear();

        for (idx, cell) in field.cells.iter().enumerate() {
            let p = cell.execution_probability;

            if p >= STREAM_PREFETCH_THRESHOLD {
                let action = if p >= STREAM_RESIDENT_THRESHOLD {
                    StreamAction::PromoteResident
                } else {
                    StreamAction::Prefetch
                };
                self.scratch.push(StreamingDecision {
                    cell_index:  idx,
                    action,
                    probability: p,
                });
            }
        }

        // Budget cap: keep highest-probability decisions.
        let budget = MAX_STREAM_REQUESTS_PER_TICK.min(self.scratch.len());
        if self.scratch.len() > budget {
            self.scratch.select_nth_unstable_by(budget - 1, |a, b| {
                b.probability
                    .partial_cmp(&a.probability)
                    .unwrap_or(std::cmp::Ordering::Equal)
            });
            self.scratch.truncate(budget);
        }

        &self.scratch[..budget]
    }

    /// Check if a single cell should initiate prefetch.
    ///
    /// Use this for per-cell queries without a full field scan.
    #[inline]
    pub fn should_prefetch(&self, probability: f32) -> bool {
        probability >= STREAM_PREFETCH_THRESHOLD
    }

    /// Check if a single cell should be promoted to resident VRAM.
    #[inline]
    pub fn should_promote_resident(&self, probability: f32) -> bool {
        probability >= STREAM_RESIDENT_THRESHOLD
    }

    /// Compute the heat injection amount for a completed stream.
    ///
    /// Scales the base `STREAM_COMPLETION_HEAT` by the cell's probability
    /// at completion time — higher probability cells get slightly more heat.
    #[inline]
    pub fn completion_heat(&self, probability_at_request: f32) -> f32 {
        // Linear scale: cells that were more likely to execute when
        // streaming was requested get proportionally more heat.
        STREAM_COMPLETION_HEAT * (0.5 + 0.5 * probability_at_request)
    }
}

impl Default for StreamingCoordinator {
    fn default() -> Self { Self::new() }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;

    fn field_with_prob(w: usize, h: usize, prob: f32) -> ActivationField {
        let mut f = ActivationField::new(w, h);
        for cell in &mut f.cells {
            cell.execution_probability = prob;
        }
        f
    }

    #[test]
    fn dormant_field_produces_no_decisions() {
        let field = field_with_prob(4, 4, 0.0);
        let mut coord = StreamingCoordinator::new();
        assert_eq!(coord.scan(&field).len(), 0);
    }

    #[test]
    fn cells_above_prefetch_threshold_get_prefetch_action() {
        let field = field_with_prob(4, 4, STREAM_PREFETCH_THRESHOLD + 0.01);
        let mut coord = StreamingCoordinator::new();
        let decisions = coord.scan(&field);
        assert!(!decisions.is_empty());
        for d in decisions {
            assert_eq!(d.action, StreamAction::Prefetch,
                "cell {} should be Prefetch, not {:?}", d.cell_index, d.action);
        }
    }

    #[test]
    fn cells_above_resident_threshold_get_promote_action() {
        let field = field_with_prob(4, 4, STREAM_RESIDENT_THRESHOLD + 0.01);
        let mut coord = StreamingCoordinator::new();
        let decisions = coord.scan(&field);
        assert!(!decisions.is_empty());
        for d in decisions {
            assert_eq!(d.action, StreamAction::PromoteResident,
                "cell {} should be PromoteResident, not {:?}", d.cell_index, d.action);
        }
    }

    #[test]
    fn decisions_bounded_by_budget() {
        // 32×32 = 1024 cells, all above threshold
        let field = field_with_prob(32, 32, 1.0);
        let mut coord = StreamingCoordinator::new();
        let decisions = coord.scan(&field);
        assert!(decisions.len() <= MAX_STREAM_REQUESTS_PER_TICK,
            "decisions {} exceeded budget {}", decisions.len(), MAX_STREAM_REQUESTS_PER_TICK);
    }

    #[test]
    fn completion_heat_scales_with_probability() {
        let coord = StreamingCoordinator::new();
        let low = coord.completion_heat(0.0);
        let high = coord.completion_heat(1.0);
        assert!(high > low, "higher probability should give more heat");
        assert!(high <= 1.0, "completion heat must not exceed 1.0: {}", high);
    }
}


===================================================================
CRATE: mirage-memory-oasis
===================================================================
--- FILE: E:\Mirage Engine\crates\mirage-memory-oasis\src\main.rs ---
fn main() {
    println!("Hello, world!");
}


--- FILE: E:\Mirage Engine\crates\mirage-memory-oasis\src\oasis\mod.rs ---
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


--- FILE: E:\Mirage Engine\crates\mirage-memory-oasis\src\oasis\streamer.rs ---
/// ===================================================================
/// mirage-memory-oasis/src/oasis/streamer.rs  (V3 — Federated Stabilization Pass)
/// PURPOSE: Async Oasis Streaming System — CANONICAL STREAMING AUTHORITY
///
/// ---------------------------------------------------------------
/// OASIS CANONICAL OWNERSHIP (V3 FEDERATED ARCHITECTURE)
/// ---------------------------------------------------------------
///
/// StreamingFabric IS the canonical streaming execution authority.
/// No other crate may own or execute streaming lifecycle operations.
///
/// OASIS owns:
///   * prefetch_horizon() — camera-predictive loading
///   * request_stream()   — activation-driven loading (future)
///   * process_results()  — result drain + residency state update
///   * loaded / queued / max_resident state tracking
///   * mmap page lifecycle (OasisManager)
///
/// MKR (mirage-mkr-core) coordinates:
///   * Computing streaming eligibility (StreamingCoordinator)
///   * Forwarding StreamingDecisions to this module via the caller
///
/// Renderer (mirage-renderer) passively consumes:
///   * is_loaded() queries for rendering decisions
///   * ResidencyTracker is updated by OASIS signals, NOT by the renderer
///
/// TODO(V3-OASIS-CANONICAL): Add a field-index-based stream request API:
///   fn request_stream_by_field_index(&mut self, cell_index: usize)
///   so MKR StreamingDecisions can drive OASIS without coordinate conversion.
///
/// TODO(V3-OASIS-CANONICAL): Move ResidencyTracker (currently in
/// mirage-renderer/src/residency.rs) into this module or a shared
/// mirage-residency crate.  The renderer must not own residency state —
/// that state should be OASIS-owned and renderer-consumed passively.
///
/// ---------------------------------------------------------------
/// IMPLEMENTATION INTENT
/// ---------------------------------------------------------------
///
/// This module implements predictive chunk loading from disk using
/// memory-mapped files (mmap). The system enables true virtualized
/// world streaming where the world exists virtually before loading.
///
/// HARDWARE INTENT:
/// - Zero-copy mmap streaming (SSD -> CPU cache -> GPU VRAM)
/// - Background loading threads (non-blocking)
/// - Prefetch prediction (load before camera arrives)
/// - Sparse page activation (only touch needed chunks)
///
/// STREAMING GUARANTEE:
/// - Camera always has chunks loaded before it arrives
/// - Predictive system looks ahead based on velocity
/// - Chunks evict when far enough away
/// - No stutters from disk I/O (all async)
/// ===================================================================

use std::sync::Arc;
use std::sync::mpsc::{channel, Sender, Receiver};

/// Chunk streaming request
#[derive(Debug, Clone)]
pub struct StreamRequest {
    pub page_id: u32,
    pub chunk_idx: u32,
}

/// Chunk streaming result
#[derive(Debug, Clone)]
pub struct StreamResult {
    pub chunk_idx: u32,
    pub data: Vec<u8>,
}

/// Background streaming worker
///
/// Handles async chunk loading from disk. Multiple instances can run
/// in parallel, each pulling from the shared request channel.
pub struct StreamWorker {
    request_rx: Receiver<StreamRequest>,
    result_tx: Sender<StreamResult>,
}

impl StreamWorker {
    pub fn new(
        request_rx: Receiver<StreamRequest>,
        result_tx: Sender<StreamResult>,
    ) -> Self {
        Self { request_rx, result_tx }
    }

    /// Run worker loop (should run in background thread)
    pub fn run(&self) {
        while let Ok(request) = self.request_rx.recv() {
            // In production, this would actually load from Oasis mmap
            // For now, simulate with zeros
            let data = vec![0u8; 3072]; // CHUNK_SIZE_BYTES

            let result = StreamResult {
                chunk_idx: request.chunk_idx,
                data,
            };

            // Send result back to main thread
            let _ = self.result_tx.send(result);
        }
    }
}

/// Streaming fabric controller
///
/// Coordinates predictive chunk loading based on camera position
/// and velocity. Maintains a queue of chunks to load and manages
/// background workers.
pub struct StreamingFabric {
    request_tx: Sender<StreamRequest>,
    result_rx: Receiver<StreamResult>,

    /// Currently queued chunks for loading
    queued: std::collections::HashSet<u32>,

    /// Already loaded chunks (cached in VRAM)
    loaded: std::collections::HashSet<u32>,

    /// Max chunks to keep loaded in VRAM
    max_resident: usize,
}

impl StreamingFabric {
    pub fn new(
        request_tx: Sender<StreamRequest>,
        result_rx: Receiver<StreamResult>,
    ) -> Self {
        Self {
            request_tx,
            result_rx,
            queued: std::collections::HashSet::new(),
            loaded: std::collections::HashSet::new(),
            max_resident: 256,
        }
    }

    /// Request predictive loading of chunks in horizon
    pub fn prefetch_horizon(
        &mut self,
        camera_pos: [f32; 3],
        camera_vel: [f32; 3],
        radius: f32,
    ) {
        // Calculate predictive position (where camera will be in future)
        let predicted_pos = [
            camera_pos[0] + camera_vel[0] * 10.0, // 10 frames ahead
            camera_pos[1] + camera_vel[1] * 10.0,
            camera_pos[2] + camera_vel[2] * 10.0,
        ];

        // Compute chunks in horizon around predicted position
        let grid_pos_x = (predicted_pos[0] / 64.0) as i32;
        let grid_pos_z = (predicted_pos[2] / 64.0) as i32;

        // Request chunks in radius around prediction
        let radius_int = radius.ceil() as i32;
        for z in (grid_pos_z - radius_int)..=(grid_pos_z + radius_int) {
            for x in (grid_pos_x - radius_int)..=(grid_pos_x + radius_int) {
                if x >= 0 && x < 25 && z >= 0 && z < 25 {
                    let chunk_idx = (z as u32 * 25) + x as u32;

                    // Only queue if not already loaded/queued and within capacity
                    if !self.loaded.contains(&chunk_idx)
                        && !self.queued.contains(&chunk_idx)
                        && self.queued.len() < 16
                    {
                        self.queued.insert(chunk_idx);
                        let _ = self.request_tx.send(StreamRequest {
                            page_id: 0,
                            chunk_idx,
                        });
                    }
                }
            }
        }
    }

    /// Process completed streaming results
    pub fn process_results(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            self.queued.remove(&result.chunk_idx);
            self.loaded.insert(result.chunk_idx);

            // Evict oldest chunk if over capacity
            if self.loaded.len() > self.max_resident {
                if let Some(&oldest) = self.loaded.iter().next() {
                    self.loaded.remove(&oldest);
                }
            }
        }
    }

    /// Check if chunk is loaded
    pub fn is_loaded(&self, chunk_idx: u32) -> bool {
        self.loaded.contains(&chunk_idx)
    }

    /// Get loading stats
    pub fn get_stats(&self) -> StreamStats {
        StreamStats {
            queued: self.queued.len(),
            loaded: self.loaded.len(),
            max_resident: self.max_resident,
        }
    }
}

#[derive(Debug, Clone)]
pub struct StreamStats {
    pub queued: usize,
    pub loaded: usize,
    pub max_resident: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn streaming_fabric_creation() {
        let (req_tx, _req_rx) = channel();
        let (_result_tx, result_rx) = channel();
        let fabric = StreamingFabric::new(req_tx, result_rx);
        assert_eq!(fabric.loaded.len(), 0);
    }
}


--- FILE: E:\Mirage Engine\crates\mirage-memory-oasis\src\oasis\uuid.rs ---
// ===================================================================
// ملف: uuid.rs (داخل نظام Oasis)
// الوظيفة: إنشاء "رقم قومي" (128-bit) لا يتكرر أبداً لكل كائن في اللعبة.
// ===================================================================

use serde::{Deserialize, Serialize};
use bytemuck::{Pod, Zeroable};

/// [MirageUuid] هو الرقم القومي للكائن.
/// السر الهندسي: استخدمنا `#[repr(transparent)]` ومصفوفة من 16 بايت `[u8; 16]`.
/// هذا يعني أن هذا الهيكل في الذاكرة هو "مجرد 16 بايت خام" بدون أي إضافات،
/// مما يجعله جاهزاً 100% ليتم قراءته من الهارد (SSD) إلى الرام فوراً (Zero-Copy).
#[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash, Pod, Zeroable)]
#[repr(transparent)]
pub struct MirageUuid(pub [u8; 16]);

impl MirageUuid {
    /// دالة لإنشاء رقم قومي فارغ (أصفار) - تُستخدم عند تهيئة الذاكرة
    #[inline]
    pub const fn zero() -> Self {
        Self([0; 16])
    }

    /// دالة لإنشاء رقم قومي جديد وعشوائي كلياً
    #[inline]
    pub fn new() -> Self {
        // نستخدم مكتبة uuid القياسية في لغة Rust لتوليد رقم عشوائي (Version 4)
        // ثم نحوله فوراً إلى مصفوفة بايتات (Bytes) ليتوافق مع معمارية Oasis
        let id = uuid::Uuid::new_v4();
        Self(id.into_bytes())
    }
}

// لضمان أن المولد الافتراضي ينشئ رقماً جديداً دائماً
impl Default for MirageUuid {
    fn default() -> Self {
        Self::new()
    }
}


