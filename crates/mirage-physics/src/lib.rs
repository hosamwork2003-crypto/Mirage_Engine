/// ===================================================================
/// mirage-physics/src/lib.rs
/// PURPOSE: Physics Chunk Pipeline - Thermal-Aware Simulation
///
/// V3 COMPATIBILITY NOTICE:
/// ChunkPhysics::get_simulation_factor() uses discrete ChunkState arms
/// as the authority for physics simulation density.  This is a
/// threshold-only scheduling assumption that is incompatible with the
/// V3 continuous activation field model.
///
/// TODO(V3-COMPAT): Replace `state: ChunkState` in ChunkPhysics with
/// `execution_probability: f32` read from the ActivationField.  The
/// simulation factor becomes: probability × base_cost_estimate.
///
/// This module is chunk-centric (not entity-centric), which is correct
/// for V3.  Only the state authority mechanism needs to change.
///
/// This module implements chunk-based physics simulation that scales
/// with chunk thermal state. Physics work is distributed across chunks
/// with Hot chunks receiving full precision and Dormant chunks
/// receiving no CPU work (GPU decides on rendering).
///
/// HARDWARE INTENT:
/// - SIMD-friendly: Processes chunks in batches
/// - Thermal scheduling: Only simulate hot chunks heavily
/// - Disturbance propagation: Physics impacts heat adjacent chunks
/// - Branchless: Minimal conditional logic in hot paths
///
/// PHILOSOPHY:
/// Physics is not entity-centric. It's chunk-centric.
/// All entities in a chunk share physics simulation density.
/// ===================================================================


use glam::Vec3;

// Re-export core types for convenience
pub use mirage_core::runtime::{ChunkState, ChunkThermals};

/// Broadphase collision query
///
/// Determines which chunks might collide with each other
/// based on AABB bounds.
#[derive(Debug, Clone)]
pub struct BroadphaseQuery {
    pub chunk_a: u32,
    pub chunk_b: u32,
    pub distance: f32,
}

/// Disturbance field for a chunk
///
/// Represents propagating forces/impacts in a chunk
/// that may affect adjacent chunks.
#[derive(Debug, Clone)]
pub struct DisturbanceField {
    pub origin_chunk: u32,
    pub force: Vec3,
    pub radius: f32,
    pub decay: f32,
}

impl DisturbanceField {
    pub fn new(origin: u32, force: Vec3, radius: f32) -> Self {
        Self {
            origin_chunk: origin,
            force,
            radius,
            decay: 1.0,
        }
    }

    /// Apply decay over time
    pub fn decay_step(&mut self, factor: f32) {
        self.decay *= factor;
        self.radius *= factor.sqrt();
    }

    /// Check if chunk is affected by this disturbance
    pub fn affects_chunk(&self, chunk_idx: u32, origin_chunk: u32) -> bool {
        // Simple grid distance check
        let grid_size = 25;
        let ox = (origin_chunk % grid_size) as i32;
        let oz = (origin_chunk / grid_size) as i32;
        let cx = (chunk_idx % grid_size) as i32;
        let cz = (chunk_idx / grid_size) as i32;

        let dist_sq = ((ox - cx).pow(2) + (oz - cz).pow(2)) as f32;
        dist_sq <= (self.radius * self.radius)
    }
}

/// Chunk physics simulator
///
/// Manages physics simulation for a single chunk.
/// Simulation intensity depends on chunk thermal state.
pub struct ChunkPhysics {
    chunk_idx: u32,
    state: ChunkState,
    thermals: ChunkThermals,
    disturbances: Vec<DisturbanceField>,
}

impl ChunkPhysics {
    pub fn new(chunk_idx: u32, state: ChunkState, thermals: ChunkThermals) -> Self {
        Self {
            chunk_idx,
            state,
            thermals,
            disturbances: Vec::new(),
        }
    }

    /// Add a disturbance to this chunk
    pub fn add_disturbance(&mut self, field: DisturbanceField) {
        self.disturbances.push(field);
    }

    /// Get simulation step complexity factor
    ///
    /// Returns multiplier for simulation intensity:
    /// - Dormant: 0.0 (no simulation)
    /// - Predictive: 0.1 (minimal, mostly streaming)
    /// - Resident: 0.5 (collision detection, no solver)
    /// - Hot: 1.0 (full simulation)
    pub fn get_simulation_factor(&self) -> f32 {
        match self.state {
            ChunkState::Dormant => 0.0,
            ChunkState::Predictive => 0.1,
            ChunkState::Resident => 0.5,
            ChunkState::Hot => 1.0,
        }
    }

    /// Update physics for this chunk
    ///
    /// Called once per frame. Actual simulation work is scaled
    /// based on thermal state.
    pub fn update(&mut self, delta_time: f32) {
        let factor = self.get_simulation_factor();
        if factor < 0.001 {
            return; // Skip entirely for dormant chunks
        }

        // Apply disturbances
        for dist in &mut self.disturbances {
            dist.decay_step(0.9);
        }

        // Remove decayed disturbances
        self.disturbances.retain(|d| d.decay > 0.01);

        // Scale solver iterations by thermal state
        let iterations = ((factor * 4.0) as usize).max(1);
        for _ in 0..iterations {
            // Physics update: in real implementation, would update entities
            // For now, just simulate heat generation
            self.thermals.add_heat(factor * 0.01);
        }
    }

    /// Broadphase collision queries for adjacent chunks
    pub fn broadphase_queries(&self) -> Vec<BroadphaseQuery> {
        let mut queries = Vec::new();

        if self.get_simulation_factor() < 0.01 {
            return queries; // Dormant chunks don't collide
        }

        // Check 8 adjacent chunks
        let grid_size = 25;
        let ox = (self.chunk_idx % grid_size) as i32;
        let oz = (self.chunk_idx / grid_size) as i32;

        for dz in -1..=1 {
            for dx in -1..=1 {
                if dx == 0 && dz == 0 {
                    continue;
                }
                let nx = ox + dx;
                let nz = oz + dz;

                if nx >= 0 && nx < 25 && nz >= 0 && nz < 25 {
                    let neighbor_idx = (nz as u32 * grid_size as u32) + nx as u32;
                    queries.push(BroadphaseQuery {
                        chunk_a: self.chunk_idx,
                        chunk_b: neighbor_idx,
                        distance: 1.0, // Adjacent chunks are distance 1
                    });
                }
            }
        }

        queries
    }
}

/// Global physics system
pub struct PhysicsSystem {
    chunks: Vec<ChunkPhysics>,
}

impl PhysicsSystem {
    pub fn new(num_chunks: usize) -> Self {
        let chunks = (0..num_chunks)
            .map(|i| ChunkPhysics::new(i as u32, ChunkState::Dormant, ChunkThermals::default()))
            .collect();

        Self { chunks }
    }

    /// Update chunk thermal state and physics
    pub fn update_chunk(&mut self, chunk_idx: usize, state: ChunkState, thermals: ChunkThermals) {
        if let Some(chunk) = self.chunks.get_mut(chunk_idx) {
            chunk.state = state;
            chunk.thermals = thermals;
        }
    }

    /// Simulate all physics
    pub fn step(&mut self, delta_time: f32) {
        for chunk in &mut self.chunks {
            chunk.update(delta_time);
        }
    }

    /// Get all broadphase queries (for narrow-phase solver)
    pub fn get_broadphase_queries(&self) -> Vec<BroadphaseQuery> {
        let mut all_queries = Vec::new();
        for chunk in &self.chunks {
            all_queries.extend(chunk.broadphase_queries());
        }
        all_queries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_physics_simulation_factor() {
        let chunk = ChunkPhysics::new(0, ChunkState::Dormant, ChunkThermals::default());
        assert_eq!(chunk.get_simulation_factor(), 0.0);

        let chunk = ChunkPhysics::new(0, ChunkState::Hot, ChunkThermals::default());
        assert_eq!(chunk.get_simulation_factor(), 1.0);
    }

    #[test]
    fn disturbance_field_affects_nearby_chunks() {
        let field = DisturbanceField::new(0, Vec3::new(1.0, 0.0, 0.0), 2.0);
        
        // Origin should be affected
        assert!(field.affects_chunk(0, 0));
        
        // Adjacent should be affected
        assert!(field.affects_chunk(1, 0));
    }

    #[test]
    fn physics_system_creation() {
        let system = PhysicsSystem::new(100);
        assert_eq!(system.chunks.len(), 100);
    }
}

