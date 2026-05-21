// ===================================================================
// mirage-matrix/src/lib.rs
// PURPOSE: Neural Matrix — Signal/Dependency Graph Math Layer
//
// V6 OWNERSHIP DECLARATION:
// ---------------------------------------------------------------
// OWNS:
//   * NeuralMatrix                — handle dependency/signal graph
//   * DisturbanceBus              — lock-free event propagation bus
//   * PlayerTransform (example)   — NeuralCluster macro demonstration
//
// MUST NOT OWN:
//   * TopologyGraph               — owned exclusively by mirage-mts
//   * continuity / emergence     — owned exclusively by mirage-morphogenic
//   * orchestration / runtime    — owned exclusively by mirage-mkr-core
//
// DETERMINISTIC & REPLAY GUARANTEES:
//   * Mathematical primitives only. No side effects.
// ===================================================================

pub mod bus;

// Topology is owned by mirage-mts. mirage-matrix contains math/signal primitives only.

use mirage_core::pool::{RuntimeDirectory, Handle, AddressMapping};
use std::collections::HashMap;

/// Neural Matrix: manages handle dependency relationships for Zero-Cost Dormancy.
pub struct NeuralMatrix {
    /// Dependency Graph: which handle affects which other handle?
    dependencies: HashMap<Handle, Vec<Handle>>,
}

impl NeuralMatrix {
    pub fn new() -> Self {
        Self {
            dependencies: HashMap::new(),
        }
    }

    /// Connect a propagation edge between two handles.
    pub fn connect(&mut self, source: Handle, target: Handle) {
        self.dependencies.entry(source).or_insert_with(Vec::new).push(target);
    }

    /// Trace the predictive impact of a change at `source`.
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
    const NUM_CHUNKS: u32 = 1024;

    #[test]
    fn test_mava_to_matrix_synapse() {
        let mut directory = RuntimeDirectory::new(NUM_CHUNKS as usize);
        let mut matrix = NeuralMatrix::new();

        let player = PlayerTransform { x: 0.0, y: 0.0, z: 0.0 };

        let handles = player.wire_to_matrix(&mut matrix, &mut directory);

        if handles.len() >= 2 {
            matrix.connect(handles[0], handles[1]);

            let impacts = matrix.trace_impact(handles[0], &directory);
            assert_eq!(impacts.len(), 1, "only one entity should be affected");
        }
    }
}