// ===================================================================
// mirage-morphogenic/src/region_replay.rs
// PURPOSE: Replay logging and exact verification for spatial regions.
// ===================================================================

use mirage_core::invariants::DeterministicInvariantViolation;
use mirage_core::numerics::hash_u64;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionReplayFrame {
    pub tick: u64,
    pub region_id: u64,
    pub state_hash: u64,
    pub payload_bytes: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegionReplayBuffer {
    pub frames: Vec<RegionReplayFrame>,
}

impl Default for RegionReplayBuffer {
    fn default() -> Self {
        Self::new()
    }
}

impl RegionReplayBuffer {
    pub fn new() -> Self {
        Self {
            frames: Vec::new(),
        }
    }

    /// Push a frame to the buffer (insertion-ordered).
    pub fn push(&mut self, frame: RegionReplayFrame) {
        self.frames.push(frame);
    }

    /// Seal the replay buffer deterministically using FNV-1a hashing.
    pub fn seal(&self) -> u64 {
        let mut hash = 2166136261u64;

        for frame in &self.frames {
            hash = hash_u64(hash, frame.tick);
            hash = hash_u64(hash, frame.region_id);
            hash = hash_u64(hash, frame.state_hash);
            for &byte in &frame.payload_bytes {
                hash = hash_u64(hash, byte as u64);
            }
        }

        hash
    }

    /// Validate exact replay equivalence against another buffer.
    pub fn validate_equivalence(&self, other: &Self) -> Result<(), DeterministicInvariantViolation> {
        if self.frames.len() != other.frames.len() {
            return Err(DeterministicInvariantViolation {
                invariant_name: "replay_buffer_length_mismatch",
                subsystem: "region_replay",
            });
        }

        for (f_a, f_b) in self.frames.iter().zip(other.frames.iter()) {
            if f_a.tick != f_b.tick {
                return Err(DeterministicInvariantViolation {
                    invariant_name: "replay_tick_mismatch",
                    subsystem: "region_replay",
                });
            }
            if f_a.region_id != f_b.region_id {
                return Err(DeterministicInvariantViolation {
                    invariant_name: "replay_region_id_mismatch",
                    subsystem: "region_replay",
                });
            }
            if f_a.state_hash != f_b.state_hash {
                return Err(DeterministicInvariantViolation {
                    invariant_name: "replay_state_hash_mismatch",
                    subsystem: "region_replay",
                });
            }
            if f_a.payload_bytes != f_b.payload_bytes {
                return Err(DeterministicInvariantViolation {
                    invariant_name: "replay_payload_mismatch",
                    subsystem: "region_replay",
                });
            }
        }

        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct RegionReplaySnapshot {
    pub buffer_hash: u64,
    pub frame_count: usize,
}
