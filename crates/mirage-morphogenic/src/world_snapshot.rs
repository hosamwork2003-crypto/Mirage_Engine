// ===================================================================
// mirage-morphogenic/src/world_snapshot.rs
// PURPOSE: World snapshot definitions, sealing, and validation.
// ===================================================================

use std::collections::BTreeMap;
use crate::spatial_continuity::SpatialContinuityState;
use mirage_core::invariants::DeterministicInvariantViolation;
use mirage_core::numerics::hash_u64;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct WorldSnapshotIdentity {
    pub world_tick: u64,
    pub continuity_epoch: u64,
    pub replay_sequence: u64,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructuralWorldSnapshot {
    pub identity: WorldSnapshotIdentity,
    pub region_snapshots: BTreeMap<u64, u8>,
    pub residency_snapshots: BTreeMap<u64, u8>,
    pub continuity_snapshots: BTreeMap<u64, SpatialContinuityState>,
}

impl Ord for StructuralWorldSnapshot {
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.identity.cmp(&other.identity)
            .then_with(|| self.region_snapshots.cmp(&other.region_snapshots))
            .then_with(|| self.residency_snapshots.cmp(&other.residency_snapshots))
            .then_with(|| self.continuity_snapshots.cmp(&other.continuity_snapshots))
    }
}

impl PartialOrd for StructuralWorldSnapshot {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl StructuralWorldSnapshot {
    /// Compute a deterministic FNV-1a hash of the entire snapshot state.
    pub fn seal(&self) -> u64 {
        let mut hash = 2166136261u64;

        hash = hash_u64(hash, self.identity.world_tick);
        hash = hash_u64(hash, self.identity.continuity_epoch);
        hash = hash_u64(hash, self.identity.replay_sequence);

        for (&region_id, &activation_state) in &self.region_snapshots {
            hash = hash_u64(hash, region_id);
            hash = hash_u64(hash, activation_state as u64);
        }

        for (&region_id, &residency_state) in &self.residency_snapshots {
            hash = hash_u64(hash, region_id);
            hash = hash_u64(hash, residency_state as u64);
        }

        for (&region_id, continuity_state) in &self.continuity_snapshots {
            hash = hash_u64(hash, region_id);
            hash = hash_u64(hash, continuity_state.intensity.to_bits());
            hash = hash_u64(hash, continuity_state.persistence.to_bits());
            hash = hash_u64(hash, continuity_state.resonance.to_bits());
        }

        hash
    }

    /// Validate exact replay equivalence against another snapshot.
    pub fn validate_equivalence(&self, other: &Self) -> Result<(), DeterministicInvariantViolation> {
        if self.identity != other.identity {
            return Err(DeterministicInvariantViolation {
                invariant_name: "snapshot_identity_mismatch",
                subsystem: "world_snapshot",
            });
        }
        if self.region_snapshots != other.region_snapshots {
            return Err(DeterministicInvariantViolation {
                invariant_name: "snapshot_region_mismatch",
                subsystem: "world_snapshot",
            });
        }
        if self.residency_snapshots != other.residency_snapshots {
            return Err(DeterministicInvariantViolation {
                invariant_name: "snapshot_residency_mismatch",
                subsystem: "world_snapshot",
            });
        }
        if self.continuity_snapshots != other.continuity_snapshots {
            return Err(DeterministicInvariantViolation {
                invariant_name: "snapshot_continuity_mismatch",
                subsystem: "world_snapshot",
            });
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StructuralWorldFrame {
    pub snapshot: StructuralWorldSnapshot,
    pub frame_hash: u64,
    pub system_seal: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::spatial_continuity::{SpatialContinuityField, SpatialContinuityState, SpatialContinuitySequence, SpatialContinuityPropagationDescriptor};
    use crate::region_replay::{RegionReplayBuffer, RegionReplayFrame, RegionReplaySnapshot};

    #[test]
    fn spatial_continuity_equivalence() {
        let mut field_a = SpatialContinuityField::new();
        let mut field_b = SpatialContinuityField::new();

        field_a.values.insert(1, SpatialContinuityState { intensity: 1.5, persistence: 2.0, resonance: 0.5 });
        field_b.values.insert(1, SpatialContinuityState { intensity: 1.5, persistence: 2.0, resonance: 0.5 });

        let seq = SpatialContinuitySequence::new(vec![
            SpatialContinuityPropagationDescriptor {
                source_region: 1,
                target_region: 2,
                factor: 0.5,
                sequence_index: 0,
            }
        ]);

        seq.apply_propagation(&mut field_a);
        seq.apply_propagation(&mut field_b);

        assert_eq!(field_a, field_b);
    }

    #[test]
    fn world_snapshot_equality() {
        let mut regions = BTreeMap::new();
        regions.insert(1, 2);
        let mut residency = BTreeMap::new();
        residency.insert(1, 3);
        let mut continuity = BTreeMap::new();
        continuity.insert(1, SpatialContinuityState { intensity: 1.0, persistence: 1.0, resonance: 1.0 });

        let snapshot_a = StructuralWorldSnapshot {
            identity: WorldSnapshotIdentity {
                world_tick: 10,
                continuity_epoch: 2,
                replay_sequence: 100,
            },
            region_snapshots: regions.clone(),
            residency_snapshots: residency.clone(),
            continuity_snapshots: continuity.clone(),
        };

        let snapshot_b = StructuralWorldSnapshot {
            identity: WorldSnapshotIdentity {
                world_tick: 10,
                continuity_epoch: 2,
                replay_sequence: 100,
            },
            region_snapshots: regions,
            residency_snapshots: residency,
            continuity_snapshots: continuity,
        };

        assert_eq!(snapshot_a, snapshot_b);
        assert_eq!(snapshot_a.seal(), snapshot_b.seal());
    }

    #[test]
    fn deterministic_region_replay() {
        let mut buffer_a = RegionReplayBuffer::new();
        let mut buffer_b = RegionReplayBuffer::new();

        let frame = RegionReplayFrame {
            tick: 1,
            region_id: 100,
            state_hash: 42,
            payload_bytes: vec![1, 2, 3],
        };

        buffer_a.push(frame.clone());
        buffer_b.push(frame);

        assert_eq!(buffer_a, buffer_b);
        assert_eq!(buffer_a.seal(), buffer_b.seal());
        assert!(buffer_a.validate_equivalence(&buffer_b).is_ok());
    }

    #[test]
    fn replay_snapshot_roundtrip() {
        let mut buffer = RegionReplayBuffer::new();
        buffer.push(RegionReplayFrame {
            tick: 1,
            region_id: 100,
            state_hash: 42,
            payload_bytes: vec![1, 2, 3],
        });

        let snapshot = RegionReplaySnapshot {
            buffer_hash: buffer.seal(),
            frame_count: buffer.frames.len(),
        };

        assert_eq!(snapshot.frame_count, 1);
        assert_ne!(snapshot.buffer_hash, 0);
    }

    #[test]
    fn continuity_decay_stability() {
        let mut field = SpatialContinuityField::new();
        field.values.insert(1, SpatialContinuityState { intensity: 10.0, persistence: 1.0, resonance: 1.0 });

        field.decay(0.9);
        let val1 = field.values.get(&1).unwrap().intensity;

        let mut field2 = SpatialContinuityField::new();
        field2.values.insert(1, SpatialContinuityState { intensity: 10.0, persistence: 1.0, resonance: 1.0 });
        field2.decay(0.9);
        let val2 = field2.values.get(&1).unwrap().intensity;

        assert_eq!(val1, val2);
    }
}

