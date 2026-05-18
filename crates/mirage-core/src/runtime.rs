// ===================================================================
// V3 AUTHORITY CONSOLIDATION — THERMAL SYSTEM DEMOTION
// ===================================================================
//
// LEGACY AUTHORITY (V2): ThermalSystem + ChunkState
//   ThermalSystem was the sole runtime authority:
//   * It owned the per-chunk heat values and decay logic.
//   * ChunkState transitions were the only source of scheduling truth.
//   * All downstream systems (executor, renderer, physics, topology)
//     branched on ChunkState enum arms to make decisions.
//   * This is a DISCRETE, threshold-driven state machine.
//
// NEW AUTHORITY (V3): ActivationField (mirage-mkr-core)
//   * ActivationField::execution_probability is the ONLY scheduling
//     authority.  It is a continuous f32 in [0.0, 1.0].
//   * All execution eligibility decisions derive from this field.
//   * No enum arms.  No hysteresis thresholds driving new decisions.
//   * The RendererBridge translates probability → ChunkState for
//     the legacy renderer ONLY — not as a decision input.
//
// CURRENT STATUS: LEGACY MIRROR ONLY
//   ThermalSystem::update_frame() is called once per tick AFTER the
//   ActivationField step, exclusively to keep downstream compat code
//   from seeing stale states.  It does NOT influence:
//   * MKR emission gate decisions
//   * Topology pressure propagation
//   * Fiber scheduling priority
//   * OASIS streaming eligibility
//
// REMOVAL PLAN:
//   TODO(V3-REMOVE-THERMAL-1): Replace ThermalScheduler in
//     mirage-executor with a field-driven fiber emitter that reads
//     EmissionRequest records from MKRWorld::emission_requests().
//   TODO(V3-REMOVE-THERMAL-2): Replace residency decisions in
//     mirage-renderer with RendererBridge::should_render() /
//     should_stream() queries driven by execution_probability.
//   TODO(V3-REMOVE-THERMAL-3): Replace ChunkPhysics::get_simulation_factor()
//     in mirage-physics with direct execution_probability reads.
//   TODO(V3-REMOVE-THERMAL-4): Remove TopologyGraph::propagate_thermal()
//     after executor migration is complete.
//   TODO(V3-REMOVE-THERMAL-FINAL): Once all four removals above are
//     complete, delete ThermalSystem entirely.  ChunkState becomes a
//     presentation-only enum used only in RendererBridge output.
//
// DO NOT:
//   * Add new scheduling logic that reads ChunkState enum arms.
//   * Add new threshold-only activation assumptions here.
//   * Increase the scope of ThermalSystem's responsibilities.
//   * Call update_frame() from any V3 decision-making path.
//
// IS CURRENTLY SAFE TO REMOVE: NO.
//   Removing ThermalSystem now would break:
//   * mirage-executor (ThermalScheduler owns ThermalSystem)
//   * mirage-executor tests (ChunkTask::state: ChunkState)
//   * mirage-renderer (reads chunk_runtime_states Vec<ChunkState>)
//   * mirage-physics (ChunkPhysics::state: ChunkState)
//   * mirage-matrix (TopologyGraph::propagate_thermal)
// ===================================================================


/// ===================================================================
/// mirage-core/src/runtime.rs
/// PURPOSE: Runtime Thermal System - Adaptive Chunk State Management
///
/// The Runtime Thermal System is the heartbeat of Mirage Engine.
/// It tracks chunk "thermal state" - how hot/active each chunk is.
/// 
/// HARDWARE INTENT:
/// - Chunk state transitions are branchless and atomic-free
/// - Thermal scoring is vectorizable (fits in cache)
/// - State transitions use hysteresis to avoid thrashing
/// - No per-entity state tracking (chunk-level only)
///
/// CACHE BEHAVIOR:
/// - All chunk states fit in L3 cache for 15,625 chunks (62.5 KB)
/// - Linear access patterns for thermal updates (SIMD-friendly)
/// - Minimal memory fragmentation (dense Vec<ChunkState>)
/// - Cache line alignment: 64 bytes = ~8 chunk states
///
/// RUNTIME STRATEGY:
/// - Heat accumulates when chunk is visible/mutating/simulating
/// - Heat decays exponentially when dormant
/// - State transitions happen on thermal thresholds with hysteresis
/// - GPU uses state enum directly for SIMD branching
/// ===================================================================


use std::sync::atomic::{AtomicU8, Ordering};

// =====================================================================
// THERMAL CONSTANTS - Hardware-tuned thresholds
// =====================================================================

/// How much heat decays per frame when chunk is dormant
/// Lower = slower decay, chunks stay hot longer
pub const THERMAL_DECAY_RATE: f32 = 0.95;

/// Heat threshold to transition Dormant -> Predictive
pub const PREDICTIVE_THRESHOLD: f32 = 0.1;

/// Heat threshold to transition Predictive -> Resident
pub const RESIDENT_THRESHOLD: f32 = 0.4;

/// Heat threshold to transition Resident -> Hot
pub const HOT_THRESHOLD: f32 = 0.7;

/// Hysteresis: Must decay below this to go back down (prevents thrashing)
pub const HOT_HYSTERESIS: f32 = 0.5;
pub const RESIDENT_HYSTERESIS: f32 = 0.2;
pub const PREDICTIVE_HYSTERESIS: f32 = 0.05;

// =====================================================================
// CHUNK STATE ENUM
// =====================================================================

/// Thermal state of a chunk in the runtime
///
/// This enum drives GPU execution decisions and CPU scheduling priorities.
/// GPU compute shader reads this as u32 array and branches accordingly.
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ChunkState {
    /// Dormant: Not in VRAM, minimal CPU work, no GPU simulation
    /// Heat source: None (decays naturally)
    /// GPU behavior: Skipped entirely
    /// Typical duration: Majority of chunks
    Dormant = 0,

    /// Predictive: Loading from disk, async streaming active, CPU updates sparse
    /// Heat source: Camera approaching, velocity prediction, adjacent chunk activity
    /// GPU behavior: Skipped (not in VRAM yet)
    /// Typical duration: Seconds before camera arrives
    Predictive = 1,

    /// Resident: In VRAM, regular CPU updates, GPU can render
    /// Heat source: Recent camera visibility, completed prefetch
    /// GPU behavior: Renders (increments instance_count) but skips physics
    /// Typical duration: Chunks near camera boundary
    Resident = 2,

    /// Hot: Fully active, dense CPU updates, full GPU physics simulation
    /// Heat source: Camera inside/adjacent, high mutation, entity interactions
    /// GPU behavior: Full simulation (physics + render + disturbance propagation)
    /// Typical duration: Chunks camera is in or immediately around
    Hot = 3,
}

impl Default for ChunkState {
    fn default() -> Self {
        ChunkState::Dormant
    }
}

impl From<u8> for ChunkState {
    fn from(val: u8) -> Self {
        match val {
            0 => ChunkState::Dormant,
            1 => ChunkState::Predictive,
            2 => ChunkState::Resident,
            3 => ChunkState::Hot,
            _ => ChunkState::Dormant,
        }
    }
}

impl From<ChunkState> for u32 {
    fn from(state: ChunkState) -> u32 {
        state as u8 as u32
    }
}

// =====================================================================
// CHUNK THERMALS - Per-chunk heat tracking
// =====================================================================

/// Per-chunk thermal metadata
///
/// This struct tracks why a chunk is hot and how it's being used.
/// Used for scheduling decisions and debug visualization.
///
/// MEMORY LAYOUT (cache-friendly):
/// - heat: f32 (heat accumulation, 0.0-1.0)
/// - last_access_frame: u64 (frame counter for decay)
/// - mutation_frequency: f32 (entity mutations per frame in chunk)
/// - camera_interest: f32 (distance-based visibility score, 0.0-1.0)
/// - ai_activity: f32 (AI simulation intensity)
///
/// Total: 24 bytes per chunk. 15,625 chunks = 375 KB.
/// Fits comfortably in L3 cache with room for other state.
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct ChunkThermals {
    /// Current heat level (0.0 = cold, 1.0 = maximum)
    pub heat: f32,

    /// Last frame this chunk was accessed (for decay calculation)
    pub last_access_frame: u64,

    /// Fraction of entities in chunk that mutated last frame
    /// Helps distinguish between static and dynamic chunks
    pub mutation_frequency: f32,

    /// How much camera cares about this chunk (visibility + interest)
    /// Combines distance, visibility cone, prediction
    pub camera_interest: f32,

    /// AI simulation intensity (if AI system is active)
    /// Prevents AI zones from being evicted prematurely
    pub ai_activity: f32,
}

impl Default for ChunkThermals {
    fn default() -> Self {
        Self {
            heat: 0.0,
            last_access_frame: 0,
            mutation_frequency: 0.0,
            camera_interest: 0.0,
            ai_activity: 0.0,
        }
    }
}

impl ChunkThermals {
    /// Add heat to this chunk (called when chunk is accessed)
    #[inline]
    pub fn add_heat(&mut self, amount: f32) {
        self.heat = (self.heat + amount).min(1.0);
    }

    /// Decay heat over time (called once per frame for inactive chunks)
    #[inline]
    pub fn decay(&mut self, frame: u64) {
        let frames_since_access = (frame - self.last_access_frame) as f32;
        // Exponential decay: heat *= 0.95^frames_since_access
        self.heat *= THERMAL_DECAY_RATE.powf(frames_since_access);
    }

    /// Compute the next state based on thermal score
    ///
    /// This implements the state machine with hysteresis to prevent flickering
    /// between states when heat is near transition boundaries.
    pub fn compute_next_state(&self, current_state: ChunkState) -> ChunkState {
        match current_state {
            ChunkState::Dormant => {
                if self.heat > PREDICTIVE_THRESHOLD {
                    ChunkState::Predictive
                } else {
                    ChunkState::Dormant
                }
            }
            ChunkState::Predictive => {
                if self.heat > RESIDENT_THRESHOLD {
                    ChunkState::Resident
                } else if self.heat < PREDICTIVE_HYSTERESIS {
                    ChunkState::Dormant
                } else {
                    ChunkState::Predictive
                }
            }
            ChunkState::Resident => {
                if self.heat > HOT_THRESHOLD {
                    ChunkState::Hot
                } else if self.heat < RESIDENT_HYSTERESIS {
                    ChunkState::Predictive
                } else {
                    ChunkState::Resident
                }
            }
            ChunkState::Hot => {
                if self.heat < HOT_HYSTERESIS {
                    ChunkState::Resident
                } else {
                    ChunkState::Hot
                }
            }
        }
    }
}

// =====================================================================
// THERMAL SYSTEM - Global chunk thermal management
// =====================================================================

/// Global thermal management system for all chunks
///
/// Manages chunk thermal state and coordinates transitions.
/// This is a read-heavy, write-sparse system that runs once per frame.
///
/// DESIGN NOTES:
/// - No locks on individual chunk states (lock-free via Vec)
/// - All state updates happen in single frame pass
/// - GPU gets atomic view via get_raw_states()
pub struct ThermalSystem {
    /// Per-chunk thermal metadata
    thermals: Vec<ChunkThermals>,
    /// Per-chunk current state
    states: Vec<ChunkState>,
    /// Per-chunk next state (updated during frame, applied at end)
    next_states: Vec<ChunkState>,
    /// Current frame number (for decay calculation)
    frame: u64,
}

impl ThermalSystem {
    /// Create thermal system for N chunks
    pub fn new(num_chunks: usize) -> Self {
        Self {
            thermals: vec![ChunkThermals::default(); num_chunks],
            states: vec![ChunkState::Dormant; num_chunks],
            next_states: vec![ChunkState::Dormant; num_chunks],
            frame: 0,
        }
    }

    /// Add heat to a chunk (call when chunk is accessed/visible)
    #[inline]
    pub fn heat_chunk(&mut self, chunk_idx: usize, amount: f32) {
        if let Some(thermal) = self.thermals.get_mut(chunk_idx) {
            thermal.add_heat(amount);
            thermal.last_access_frame = self.frame;
        }
    }

    /// Set chunk visibility score (0.0 = not visible, 1.0 = directly visible)
    #[inline]
    pub fn set_camera_interest(&mut self, chunk_idx: usize, interest: f32) {
        if let Some(thermal) = self.thermals.get_mut(chunk_idx) {
            thermal.camera_interest = interest;
            if interest > 0.0 {
                thermal.add_heat(interest * 0.5); // Visibility adds heat
            }
        }
    }

    /// Set mutation frequency (0.0 = static, 1.0 = all entities mutated)
    #[inline]
    pub fn set_mutation_frequency(&mut self, chunk_idx: usize, frequency: f32) {
        if let Some(thermal) = self.thermals.get_mut(chunk_idx) {
            thermal.mutation_frequency = frequency;
            if frequency > 0.0 {
                thermal.add_heat(frequency * 0.3); // Mutations add heat
            }
        }
    }

    /// Get current state of chunk
    #[inline]
    pub fn get_state(&self, chunk_idx: usize) -> ChunkState {
        self.states.get(chunk_idx).copied().unwrap_or(ChunkState::Dormant)
    }

    /// Get all current states as Vec<u32> for GPU
    pub fn get_raw_states(&self) -> Vec<u32> {
        self.states.iter().map(|&s| u32::from(s)).collect()
    }

    /// Perform thermal update (call once per frame)
    ///
    /// This function:
    /// 1. Decays heat for all chunks
    /// 2. Computes next states based on thermal scores
    /// 3. Applies state transitions
    /// 4. Increments frame counter
    pub fn update_frame(&mut self) {
        // Decay heat for all chunks
        for thermal in &mut self.thermals {
            thermal.decay(self.frame);
        }

        // Compute next states
        for (idx, (thermal, current_state)) in self
            .thermals
            .iter()
            .zip(self.states.iter())
            .enumerate()
        {
            self.next_states[idx] = thermal.compute_next_state(*current_state);
        }

        // Apply state transitions
        self.states.copy_from_slice(&self.next_states);

        // Increment frame
        self.frame = self.frame.wrapping_add(1);
    }

    /// Get statistics for debugging
    pub fn get_stats(&self) -> ThermalStats {
        let mut dormant = 0;
        let mut predictive = 0;
        let mut resident = 0;
        let mut hot = 0;

        for &state in &self.states {
            match state {
                ChunkState::Dormant => dormant += 1,
                ChunkState::Predictive => predictive += 1,
                ChunkState::Resident => resident += 1,
                ChunkState::Hot => hot += 1,
            }
        }

        ThermalStats {
            dormant,
            predictive,
            resident,
            hot,
            avg_heat: self.thermals.iter().map(|t| t.heat).sum::<f32>()
                / (self.thermals.len() as f32).max(1.0),
        }
    }
}

/// Debug statistics for thermal system
#[derive(Debug, Clone)]
pub struct ThermalStats {
    pub dormant: usize,
    pub predictive: usize,
    pub resident: usize,
    pub hot: usize,
    pub avg_heat: f32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_ordering() {
        assert!(ChunkState::Dormant < ChunkState::Predictive);
        assert!(ChunkState::Predictive < ChunkState::Resident);
        assert!(ChunkState::Resident < ChunkState::Hot);
    }

    #[test]
    fn thermal_state_transitions() {
        let mut thermal = ChunkThermals::default();

        // Heat should cause state progression
        thermal.heat = 0.15; // Above PREDICTIVE_THRESHOLD (0.1)
        assert_eq!(
            thermal.compute_next_state(ChunkState::Dormant),
            ChunkState::Predictive
        );

        thermal.heat = 0.5; // Above RESIDENT_THRESHOLD (0.4)
        assert_eq!(
            thermal.compute_next_state(ChunkState::Predictive),
            ChunkState::Resident
        );

        // Hysteresis prevents oscillation
        thermal.heat = 0.06; // Below PREDICTIVE_HYSTERESIS (0.05) but above threshold
        assert_eq!(
            thermal.compute_next_state(ChunkState::Predictive),
            ChunkState::Predictive
        );
    }

    #[test]
    fn thermal_system_creation() {
        let system = ThermalSystem::new(1000);
        assert_eq!(system.states.len(), 1000);
        assert!(system.states.iter().all(|&s| s == ChunkState::Dormant));
    }
}
