// ===================================================================
// mirage-core/src/world_contracts.rs
// PURPOSE: World execution contracts and governance restrictions.
// ===================================================================

pub struct StructuralWorldContract;
pub struct RuntimeSpatialContract;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ForbiddenSpatialAuthority {
    AsyncScheduling,
    TaskGraphs,
    UnorderedTraversal,
    RuntimeReordering,
    TopologyMutationFromMorphogenic,
    ContinuityMutationFromRenderer,
    OrchestrationOutsideMkrCore,
}

impl ForbiddenSpatialAuthority {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::AsyncScheduling => "async scheduling",
            Self::TaskGraphs => "task graphs",
            Self::UnorderedTraversal => "unordered traversal",
            Self::RuntimeReordering => "runtime reordering",
            Self::TopologyMutationFromMorphogenic => "topology mutation from morphogenic",
            Self::ContinuityMutationFromRenderer => "continuity mutation from renderer",
            Self::OrchestrationOutsideMkrCore => "orchestration outside mkr-core",
        }
    }
}
