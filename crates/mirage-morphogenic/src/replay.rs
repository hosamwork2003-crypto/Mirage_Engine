use crate::continuity::ContinuitySnapshot;

#[derive(Debug, Clone, PartialEq)]
pub struct StructuralReplaySnapshot {
    pub originating_tick: u64,
    pub realization_sequence_index: u64,
    pub continuity_epoch: u64,
    pub continuity_snapshot: ContinuitySnapshot,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StructuralReplayFrame {
    pub replay_index: u64,
    pub snapshot: StructuralReplaySnapshot,
}

#[derive(Debug, Clone)]
pub struct StructuralReplayBuffer {
    pub frames: Vec<StructuralReplayFrame>,
}

impl StructuralReplayBuffer {
    pub fn new() -> Self { Self { frames: Vec::new() } }

    pub fn append_frame(&mut self, frame: StructuralReplayFrame) {
        self.frames.push(frame);
    }

    pub fn latest_frame(&self) -> Option<&StructuralReplayFrame> { self.frames.last() }

    /// Return a copy of frames in [start, end) (end is exclusive). Bounds are clamped deterministically.
    pub fn replay_range(&self, start: usize, end: usize) -> Vec<StructuralReplayFrame> {
        let len = self.frames.len();
        if start >= len || start >= end { return Vec::new(); }
        let e = if end > len { len } else { end };
        self.frames[start..e].to_vec()
    }

    /// Determine buffer equivalence (frame-wise equality).
    pub fn replay_equivalence(&self, other: &Self) -> bool {
        self.frames == other.frames
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::continuity::ContinuitySnapshot;

    #[test]
    fn append_and_latest_and_range() {
        let mut buf = StructuralReplayBuffer::new();
        let cs = ContinuitySnapshot::new(1, vec![0.0]);
        let snap = StructuralReplaySnapshot { originating_tick: 1, realization_sequence_index: 0, continuity_epoch: 1, continuity_snapshot: cs.clone() };
        let frame = StructuralReplayFrame { replay_index: 0, snapshot: snap.clone() };
        buf.append_frame(frame.clone());
        assert!(buf.latest_frame().is_some());
        let r = buf.replay_range(0, 1);
        assert_eq!(r.len(), 1);
    }

    #[test]
    fn replay_equivalence_true_false() {
        let mut a = StructuralReplayBuffer::new();
        let mut b = StructuralReplayBuffer::new();
        let cs = ContinuitySnapshot::new(1, vec![0.0]);
        let snap = StructuralReplaySnapshot { originating_tick: 1, realization_sequence_index: 0, continuity_epoch: 1, continuity_snapshot: cs.clone() };
        let frame = StructuralReplayFrame { replay_index: 0, snapshot: snap.clone() };
        a.append_frame(frame.clone());
        b.append_frame(frame.clone());
        assert!(a.replay_equivalence(&b));
        b.append_frame(StructuralReplayFrame { replay_index: 1, snapshot: snap.clone() });
        assert!(!a.replay_equivalence(&b));
    }
}