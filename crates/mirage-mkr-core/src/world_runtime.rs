// ===================================================================
// mirage-mkr-core/src/world_runtime.rs
// PURPOSE: Metamorphic Kernel Core world runtime realization.
// ===================================================================

use std::collections::BTreeMap;
use crate::region_identity::StructuralRegionId;
use crate::region_graph::StructuralRegionGraph;
use crate::region_runtime::StructuralRegionRuntime;
use crate::streaming_pipeline::StructuralStreamingSequence;
use crate::residency_runtime::StructuralResidencySequence;
use mirage_morphogenic::spatial_continuity::{SpatialContinuitySequence, SpatialContinuityField};
use mirage_morphogenic::world_snapshot::{StructuralWorldSnapshot, WorldSnapshotIdentity};
use mirage_core::invariants::DeterministicInvariantViolation;
use serde::{Serialize, Deserialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash, Serialize, Deserialize)]
pub enum WorldRuntimePhase {
    TopologyExtraction = 1,
    RegionGraphConstruction = 2,
    RegionActivation = 3,
    ResidencyPreparation = 4,
    StructuralStreaming = 5,
    ContinuityPropagation = 6,
    EmergencePropagation = 7,
    PersistenceStabilization = 8,
    ReplaySnapshotSealing = 9,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WorldRuntimeSequence {
    pub phases: Vec<WorldRuntimePhase>,
}

impl Default for WorldRuntimeSequence {
    fn default() -> Self {
        Self {
            phases: vec![
                WorldRuntimePhase::TopologyExtraction,
                WorldRuntimePhase::RegionGraphConstruction,
                WorldRuntimePhase::RegionActivation,
                WorldRuntimePhase::ResidencyPreparation,
                WorldRuntimePhase::StructuralStreaming,
                WorldRuntimePhase::ContinuityPropagation,
                WorldRuntimePhase::EmergencePropagation,
                WorldRuntimePhase::PersistenceStabilization,
                WorldRuntimePhase::ReplaySnapshotSealing,
            ],
        }
    }
}

pub struct StructuralWorldRuntime {
    pub world_tick: u64,
    pub continuity_epoch: u64,
    pub replay_sequence: u64,
    pub region_graph: StructuralRegionGraph,
    pub region_runtimes: BTreeMap<StructuralRegionId, StructuralRegionRuntime>,
}

impl Default for StructuralWorldRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl StructuralWorldRuntime {
    pub fn new() -> Self {
        Self {
            world_tick: 0,
            continuity_epoch: 0,
            replay_sequence: 0,
            region_graph: StructuralRegionGraph::new(),
            region_runtimes: BTreeMap::new(),
        }
    }

    /// Sequential, deterministic, synchronoustick execution realizers.
    pub fn realize_world_tick(
        &mut self,
        streaming_seq: &StructuralStreamingSequence,
        residency_seq: &StructuralResidencySequence,
        continuity_seq: &SpatialContinuitySequence,
        continuity_field: &mut SpatialContinuityField,
    ) -> Result<StructuralWorldSnapshot, DeterministicInvariantViolation> {
        // Phase 1: TopologyExtraction (verify graph structure holds nodes)
        if self.region_graph.nodes.is_empty() && !self.region_runtimes.is_empty() {
            return Err(DeterministicInvariantViolation {
                invariant_name: "empty_topology_during_execution",
                subsystem: "world_runtime",
            });
        }

        // Phase 2: RegionGraphConstruction
        self.region_graph.canonicalize();

        // Phase 3 & 4 & 5: Activation, Residency Preparation, Streaming realization
        streaming_seq.apply_streaming_sequence(&mut self.region_runtimes)?;

        // Phase 6 & 7: Continuity & Emergence Propagation
        continuity_seq.apply_propagation(continuity_field);
        continuity_field.stabilize();

        // Phase 8: Persistence Stabilization
        residency_seq.apply_residency_sequence(&mut self.region_runtimes)?;

        // Phase 9: ReplaySnapshotSealing
        self.world_tick += 1;
        self.replay_sequence += 1;

        self.seal_world_snapshot(continuity_field)
    }

    /// Realize runtime changes for a single region.
    pub fn realize_region_runtime(
        &mut self,
        region_id: StructuralRegionId,
    ) -> Result<(), DeterministicInvariantViolation> {
        let runtime = self.region_runtimes.get_mut(&region_id).ok_or(
            DeterministicInvariantViolation {
                invariant_name: "missing_region_runtime",
                subsystem: "world_runtime",
            }
        )?;
        runtime.stabilize_region()?;
        Ok(())
    }

    /// Seal the current state of the world into a world snapshot.
    pub fn seal_world_snapshot(
        &self,
        continuity_field: &SpatialContinuityField,
    ) -> Result<StructuralWorldSnapshot, DeterministicInvariantViolation> {
        let identity = WorldSnapshotIdentity {
            world_tick: self.world_tick,
            continuity_epoch: self.continuity_epoch,
            replay_sequence: self.replay_sequence,
        };

        let mut region_snapshots = BTreeMap::new();
        let mut residency_snapshots = BTreeMap::new();
        let mut continuity_snapshots = BTreeMap::new();

        for (&id, runtime) in &self.region_runtimes {
            region_snapshots.insert(id.0, runtime.activation as u8);
            residency_snapshots.insert(id.0, runtime.residency as u8);

            // Inherit continuity if mapped in the field
            if let Some(&state) = continuity_field.values.get(&id.0) {
                continuity_snapshots.insert(id.0, state);
            }
        }

        Ok(StructuralWorldSnapshot {
            identity,
            region_snapshots,
            residency_snapshots,
            continuity_snapshots,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::region_identity::{StructuralRegionId, StructuralRegionGeneration, RegionRuntimeIdentity};
    use crate::region_graph::{StructuralRegionNode, RegionTransitionDescriptor};
    use crate::region_runtime::{RegionActivationState, RegionResidencyState, RegionStreamingState};
    use crate::streaming_pipeline::{StructuralStreamingDescriptor, StructuralStreamingSequence, StreamingPhase};
    use crate::residency_runtime::{StructuralResidencyDescriptor, StructuralResidencySequence, ResidencyStabilizationState};
    use mirage_morphogenic::spatial_continuity::{SpatialContinuityState, SpatialContinuityPropagationDescriptor};

    #[test]
    fn deterministic_region_ordering() {
        let r1 = StructuralRegionId(10);
        let r2 = StructuralRegionId(5);
        let mut list = vec![r1, r2];
        list.sort();
        assert_eq!(list[0], r2);
        assert_eq!(list[1], r1);
    }

    #[test]
    fn stable_region_graph_sorting() {
        let mut graph = StructuralRegionGraph::new();
        graph.add_transition(RegionTransitionDescriptor {
            source_region: StructuralRegionId(2),
            target_region: StructuralRegionId(10),
            sequence_index: 5,
            transition_weight: 1.0,
            provenance: 0,
        });
        graph.add_transition(RegionTransitionDescriptor {
            source_region: StructuralRegionId(1),
            target_region: StructuralRegionId(5),
            sequence_index: 2,
            transition_weight: 0.5,
            provenance: 0,
        });

        graph.canonicalize();
        let transitions = graph.deterministic_traversal_iterators().collect::<Vec<_>>();
        assert_eq!(transitions[0].source_region, StructuralRegionId(1));
        assert_eq!(transitions[1].source_region, StructuralRegionId(2));
    }

    #[test]
    fn replay_safe_region_activation() {
        let id = RegionRuntimeIdentity::new(StructuralRegionId(1), StructuralRegionGeneration(1), 0, 0, 0);
        let mut runtime = StructuralRegionRuntime::new(id);

        assert_eq!(runtime.activation, RegionActivationState::Inactive);
        assert!(runtime.activate_region().is_ok());
        assert_eq!(runtime.activation, RegionActivationState::Activating);
        assert!(runtime.activate_region().is_ok());
        assert_eq!(runtime.activation, RegionActivationState::Active);

        assert!(runtime.activate_region().is_err());
    }

    #[test]
    fn deterministic_streaming_sequence() {
        let id = RegionRuntimeIdentity::new(StructuralRegionId(1), StructuralRegionGeneration(1), 0, 0, 0);
        let runtime = StructuralRegionRuntime::new(id);
        let mut runtimes = BTreeMap::new();
        runtimes.insert(StructuralRegionId(1), runtime);

        let seq = StructuralStreamingSequence::new(vec![
            StructuralStreamingDescriptor {
                region_id: StructuralRegionId(1),
                phase: StreamingPhase::RegionActivation,
                sequence_index: 0,
                target_residency: RegionResidencyState::Evicted,
            },
            StructuralStreamingDescriptor {
                region_id: StructuralRegionId(1),
                phase: StreamingPhase::ResidencyPreparation,
                sequence_index: 1,
                target_residency: RegionResidencyState::Evicted,
            },
        ]);

        assert!(seq.apply_streaming_sequence(&mut runtimes).is_ok());
        let rt = runtimes.get(&StructuralRegionId(1)).unwrap();
        assert_eq!(rt.activation, RegionActivationState::Activating);
        assert_eq!(rt.residency, RegionResidencyState::Loading);
    }

    #[test]
    fn residency_transition_stability() {
        let id = RegionRuntimeIdentity::new(StructuralRegionId(1), StructuralRegionGeneration(1), 0, 0, 0);
        let mut runtime = StructuralRegionRuntime::new(id);
        runtime.residency = RegionResidencyState::Resident;

        let mut runtimes = BTreeMap::new();
        runtimes.insert(StructuralRegionId(1), runtime);

        let seq = StructuralResidencySequence::new(vec![
            StructuralResidencyDescriptor {
                region_id: StructuralRegionId(1),
                target_state: RegionResidencyState::Evicted,
                stabilization: ResidencyStabilizationState::Unstable,
                sequence_index: 0,
            }
        ]);

        assert!(seq.apply_residency_sequence(&mut runtimes).is_err());
    }

    #[test]
    fn canonical_world_tick_order() {
        let seq = WorldRuntimeSequence::default();
        assert_eq!(seq.phases.len(), 9);
        assert_eq!(seq.phases[0], WorldRuntimePhase::TopologyExtraction);
        assert_eq!(seq.phases[8], WorldRuntimePhase::ReplaySnapshotSealing);
    }

    #[test]
    fn replay_equivalent_world_ticks() {
        let mut world_a = StructuralWorldRuntime::new();
        let mut world_b = StructuralWorldRuntime::new();

        let rid = StructuralRegionId(1);
        world_a.region_graph.add_region(StructuralRegionNode { region_id: rid });
        world_b.region_graph.add_region(StructuralRegionNode { region_id: rid });

        let identity = RegionRuntimeIdentity::new(rid, StructuralRegionGeneration(1), 0, 0, 0);
        world_a.region_runtimes.insert(rid, StructuralRegionRuntime::new(identity));
        world_b.region_runtimes.insert(rid, StructuralRegionRuntime::new(identity));

        let streaming_seq = StructuralStreamingSequence::new(vec![
            StructuralStreamingDescriptor {
                region_id: rid,
                phase: StreamingPhase::RegionActivation,
                sequence_index: 0,
                target_residency: RegionResidencyState::Loading,
            }
        ]);
        let residency_seq = StructuralResidencySequence::new(vec![]);
        let continuity_seq = SpatialContinuitySequence::new(vec![]);
        let mut continuity_field_a = SpatialContinuityField::new();
        let mut continuity_field_b = SpatialContinuityField::new();

        let snap_a = world_a.realize_world_tick(&streaming_seq, &residency_seq, &continuity_seq, &mut continuity_field_a).unwrap();
        let snap_b = world_b.realize_world_tick(&streaming_seq, &residency_seq, &continuity_seq, &mut continuity_field_b).unwrap();

        assert_eq!(snap_a, snap_b);
        assert_eq!(snap_a.seal(), snap_b.seal());
    }
}

