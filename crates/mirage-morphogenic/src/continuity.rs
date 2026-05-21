/// Structural continuity storage and snapshotting.
/// continuity storage is private; only deterministic APIs are exposed.

pub type ContinuityEpoch = u64;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SnapshotIdentity {
    pub continuity_epoch: u64,
    pub originating_tick: u64,
    pub realization_sequence_index: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ContinuitySnapshot {
    pub epoch: ContinuityEpoch,
    /// Deterministic realization sequence index (provenance)
    pub realization_sequence_index: u64,
    /// Originating tick of the snapshot (provenance)
    pub originating_tick: u64,
    /// Snapshot identity (immutable provenance bundle)
    pub snapshot_identity: SnapshotIdentity,
    /// Immutable snapshot payload (public for replay/audit; produced/consumed by deterministic appliers)
    pub continuity: Vec<f32>,
}

impl ContinuitySnapshot {
    /// Create a snapshot with default provenance (zeros).
    pub fn new(epoch: ContinuityEpoch, continuity: Vec<f32>) -> Self {
        let mut c = continuity;
        for v in &mut c {
            *v = v.clamp(0.0, 1.0);
        }
        let realization_sequence_index = 0;
        let originating_tick = 0;
        Self {
            epoch,
            realization_sequence_index,
            originating_tick,
            snapshot_identity: SnapshotIdentity {
                continuity_epoch: epoch,
                originating_tick,
                realization_sequence_index,
            },
            continuity: c,
        }
    }

    /// Create a snapshot with explicit provenance.
    pub fn with_provenance(epoch: ContinuityEpoch, realization_sequence_index: u64, originating_tick: u64, continuity: Vec<f32>) -> Self {
        let mut c = continuity;
        for v in &mut c {
            *v = v.clamp(0.0, 1.0);
        }
        Self {
            epoch,
            realization_sequence_index,
            originating_tick,
            snapshot_identity: SnapshotIdentity {
                continuity_epoch: epoch,
                originating_tick,
                realization_sequence_index,
            },
            continuity: c,
        }
    }

    #[inline]
    pub fn len(&self) -> usize { self.continuity.len() }

    #[inline]
    pub fn get(&self, idx: usize) -> Option<f32> { self.continuity.get(idx).copied() }

    /// Obtain a copy of the snapshot identity.
    pub fn identity(&self) -> SnapshotIdentity { self.snapshot_identity.clone() }
}

/// Deterministic lifecycle metadata for managed continuity snapshots.
#[derive(Debug, Clone, PartialEq)]
pub struct ContinuityLifecycleState {
    pub created_epoch: ContinuityEpoch,
    pub last_realized_epoch: ContinuityEpoch,
    pub realization_count: u64,
    pub continuity_version: u64,
}

/// Managed continuity snapshot pairs a snapshot with deterministic lifecycle metadata.
#[derive(Debug, Clone, PartialEq)]
pub struct ManagedContinuitySnapshot {
    pub lifecycle: ContinuityLifecycleState,
    pub snapshot: ContinuitySnapshot,
}

impl ManagedContinuitySnapshot {
    pub fn new(created_epoch: ContinuityEpoch, snapshot: ContinuitySnapshot) -> Self {
        Self {
            lifecycle: ContinuityLifecycleState {
                created_epoch,
                last_realized_epoch: snapshot.epoch,
                realization_count: 1,
                continuity_version: 0,
            },
            snapshot,
        }
    }

    /// Advance to the next snapshot, returning a new ManagedContinuitySnapshot.
    /// Deterministic: increments counters, updates epoch, never mutates self.
    pub fn advance(&self, next_snapshot: ContinuitySnapshot) -> Self {
        let mut lifecycle = self.lifecycle.clone();
        lifecycle.realization_count = lifecycle.realization_count.saturating_add(1);
        lifecycle.continuity_version = lifecycle.continuity_version.saturating_add(1);
        lifecycle.last_realized_epoch = next_snapshot.epoch;
        Self { lifecycle, snapshot: next_snapshot }
    }
}

/// Private runtime continuity field. Direct access to the internal Vec is not allowed.
#[derive(Clone, Debug, PartialEq)]
pub struct StructuralContinuityField {
    continuity: Vec<f32>,
}

impl StructuralContinuityField {
    /// Create a new field of length `len` initialised to zeros.
    pub fn new(len: usize) -> Self {
        Self { continuity: vec![0.0f32; len] }
    }

    #[inline]
    pub fn len(&self) -> usize { self.continuity.len() }

    #[inline]
    pub fn is_empty(&self) -> bool { self.continuity.is_empty() }

    /// Deterministic read-only access.
    #[inline]
    pub fn get(&self, idx: usize) -> Option<f32> { self.continuity.get(idx).copied() }

    /// Deterministic write (clamped). Out-of-range index is a no-op.
    pub fn set(&mut self, idx: usize, value: f32) {
        if idx < self.continuity.len() {
            self.continuity[idx] = value.clamp(0.0, 1.0);
        }
    }

    /// Produce an immutable snapshot (deterministic copy) with default provenance.
    pub fn snapshot(&self, epoch: ContinuityEpoch) -> ContinuitySnapshot {
        ContinuitySnapshot::new(epoch, self.continuity.clone())
    }

    /// Produce an immutable snapshot including provenance fields.
    pub fn snapshot_with_provenance(&self, epoch: ContinuityEpoch, realization_sequence_index: u64, originating_tick: u64) -> ContinuitySnapshot {
        ContinuitySnapshot::with_provenance(epoch, realization_sequence_index, originating_tick, self.continuity.clone())
    }

    /// Replace current continuity with a snapshot (deterministic).
    /// Snapshot continuity length must equal field length; otherwise a panic is raised.
    pub fn apply_snapshot(&mut self, snapshot: &ContinuitySnapshot) {
        if snapshot.continuity.len() != self.continuity.len() {
            panic!("snapshot length mismatch in apply_snapshot: {} vs {}", snapshot.continuity.len(), self.continuity.len());
        }
        self.continuity = snapshot.continuity.clone();
    }

    /// Deterministic neighbor smoothing pass (index-ordered).
    pub fn smoothing_pass(&mut self) {
        let n = self.continuity.len();
        if n == 0 { return; }
        let mut smoothed = self.continuity.clone();
        for i in 0..n {
            let mut sum = self.continuity[i];
            let mut count = 1.0f32;
            if i > 0 {
                sum += self.continuity[i - 1];
                count += 1.0;
            }
            if i + 1 < n {
                sum += self.continuity[i + 1];
                count += 1.0;
            }
            smoothed[i] = (sum / count).clamp(0.0, 1.0);
        }
        self.continuity = smoothed;
    }

    /// Deterministic decay application (index-ordered).
    pub fn apply_decay(&mut self, factor: f32) {
        let f = factor.clamp(0.0, 1.0);
        for v in &mut self.continuity {
            *v = (*v * f).clamp(0.0, 1.0);
        }
    }

    /// Deterministic enumerated snapshot of (index, value) as a Vec (stable ordering).
    pub fn stable_index_iter_clone(&self) -> Vec<(usize, f32)> {
        self.continuity.iter().enumerate().map(|(i, &v)| (i, v)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn snapshot_and_apply_roundtrip() {
        let mut field = StructuralContinuityField::new(4);
        field.set(1, 0.7);
        let snap = field.snapshot(42);
        assert_eq!(snap.epoch, 42);
        let mut field2 = StructuralContinuityField::new(4);
        field2.apply_snapshot(&snap);
        assert_eq!(field2.get(1).unwrap(), 0.7);
    }

    #[test]
    fn smoothing_deterministic() {
        let mut f = StructuralContinuityField::new(3);
        f.set(0, 1.0);
        f.set(1, 0.0);
        f.set(2, 0.0);
        f.smoothing_pass();
        assert!((f.get(0).unwrap() - 0.5).abs() < 1e-6);
    }

    #[test]
    fn continuity_lifecycle_advancement_replay_safe() {
        let mut field = StructuralContinuityField::new(3);
        field.set(0, 0.5);
        let snap0 = field.snapshot(10);
        let managed = ManagedContinuitySnapshot::new(10, snap0.clone());
        let mut field2 = StructuralContinuityField::new(3);
        field2.set(0, 0.8);
        let snap1 = field2.snapshot(11);
        let managed2 = managed.advance(snap1.clone());
        assert_eq!(managed2.lifecycle.realization_count, managed.lifecycle.realization_count + 1);
        assert_eq!(managed2.lifecycle.last_realized_epoch, snap1.epoch);
        assert_eq!(managed2.lifecycle.continuity_version, managed.lifecycle.continuity_version + 1);
    }
}