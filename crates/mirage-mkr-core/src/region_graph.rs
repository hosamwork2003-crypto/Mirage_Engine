// ===================================================================
// mirage-mkr-core/src/region_graph.rs
// PURPOSE: Deterministic region transition graph.
// ===================================================================

use std::collections::BTreeMap;
use crate::region_identity::StructuralRegionId;
use serde::{Serialize, Deserialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct StructuralRegionNode {
    pub region_id: StructuralRegionId,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub struct StructuralRegionEdge {
    pub source: StructuralRegionId,
    pub target: StructuralRegionId,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct RegionTransitionDescriptor {
    pub source_region: StructuralRegionId,
    pub target_region: StructuralRegionId,
    pub sequence_index: u64,
    pub transition_weight: f64,
    pub provenance: u64,
}

impl PartialEq for RegionTransitionDescriptor {
    fn eq(&self, other: &Self) -> bool {
        self.source_region == other.source_region
            && self.target_region == other.target_region
            && self.sequence_index == other.sequence_index
            && self.provenance == other.provenance
            && self.transition_weight.to_bits() == other.transition_weight.to_bits()
    }
}

impl Eq for RegionTransitionDescriptor {}

impl Ord for RegionTransitionDescriptor {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.source_region.cmp(&other.source_region)
            .then_with(|| self.target_region.cmp(&other.target_region))
            .then_with(|| self.sequence_index.cmp(&other.sequence_index))
            .then_with(|| self.provenance.cmp(&other.provenance))
            .then_with(|| self.transition_weight.to_bits().cmp(&other.transition_weight.to_bits()))
    }
}

impl PartialOrd for RegionTransitionDescriptor {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StructuralRegionGraph {
    pub nodes: BTreeMap<StructuralRegionId, StructuralRegionNode>,
    pub transitions: Vec<RegionTransitionDescriptor>,
}

impl Default for StructuralRegionGraph {
    fn default() -> Self {
        Self::new()
    }
}

impl StructuralRegionGraph {
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
            transitions: Vec::new(),
        }
    }

    /// Add a region node to the graph.
    pub fn add_region(&mut self, node: StructuralRegionNode) {
        self.nodes.insert(node.region_id, node);
    }

    /// Add a transition edge to the graph.
    pub fn add_transition(&mut self, transition: RegionTransitionDescriptor) {
        self.transitions.push(transition);
    }

    /// Sort transitions deterministically according to governance traversal rules.
    pub fn stable_sort(&mut self) {
        self.transitions.sort();
    }

    /// Canonicalize the graph.
    pub fn canonicalize(&mut self) {
        self.stable_sort();
    }

    /// Return an iterator over transitions in deterministic order.
    pub fn deterministic_traversal_iterators(&self) -> std::slice::Iter<'_, RegionTransitionDescriptor> {
        self.transitions.iter()
    }
}
