use crate::emergence::StructuralEmergenceState;
use crate::resonance::EmergenceProvenance;

#[derive(Debug, Clone, PartialEq)]
pub struct EmergenceRealizationFrame {
    pub tick: u64,
    pub epoch: u64,
    pub snapshot: StructuralEmergenceState,
    pub provenance: EmergenceProvenance,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmergenceRealizationSequence {
    pub frames: Vec<EmergenceRealizationFrame>,
}

impl EmergenceRealizationSequence {
    pub fn new() -> Self { Self { frames: Vec::new() } }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EmergenceHistoryBuffer {
    frames: Vec<EmergenceRealizationFrame>,
    capacity: usize,
    next_index: usize,
    filled: bool,
}

impl EmergenceHistoryBuffer {
    pub fn new(capacity: usize) -> Self { Self { frames: Vec::with_capacity(capacity), capacity, next_index: 0, filled: false } }

    pub fn push_frame(&mut self, frame: EmergenceRealizationFrame) {
        if self.frames.len() < self.capacity {
            self.frames.push(frame);
            if self.frames.len() == self.capacity { self.filled = true; }
        } else {
            self.frames[self.next_index] = frame;
            self.next_index = (self.next_index + 1) % self.capacity;
        }
    }

    pub fn latest(&self) -> Option<&EmergenceRealizationFrame> {
        if self.frames.is_empty() { return None; }
        if !self.filled { return self.frames.last(); }
        let idx = if self.next_index == 0 { self.capacity - 1 } else { self.next_index - 1 };
        self.frames.get(idx)
    }

    pub fn frame_at(&self, index: usize) -> Option<&EmergenceRealizationFrame> { self.frames.get(index) }
    pub fn len(&self) -> usize { self.frames.len() }
    pub fn capacity(&self) -> usize { self.capacity }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::emergence::StructuralEmergenceState;
    use crate::resonance::EmergenceProvenance;

    #[test]
    fn history_buffer_overwrite_deterministic() {
        let mut buf = EmergenceHistoryBuffer::new(2);
        let s = StructuralEmergenceState::new(0.1, 0.2, 0.3, 0.4);
        let p = EmergenceProvenance { originating_tick:0, continuity_epoch:0, resonance_sequence_index:0, topology_generation:0 };
        buf.push_frame(EmergenceRealizationFrame { tick: 1, epoch: 1, snapshot: s.clone(), provenance: p });
        buf.push_frame(EmergenceRealizationFrame { tick: 2, epoch: 2, snapshot: s.clone(), provenance: p });
        buf.push_frame(EmergenceRealizationFrame { tick: 3, epoch: 3, snapshot: s.clone(), provenance: p });
        assert_eq!(buf.len(), 2);
        assert_eq!(buf.latest().unwrap().epoch, 3);
    }
}