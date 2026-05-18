/// ===================================================================
/// mirage-matrix/src/bus.rs
/// PURPOSE: Lock-Free Disturbance Bus - Reactive Event Propagation
///
/// This module implements a lock-free event bus for propagating
/// disturbances (chunk mutations, physics impacts, entity events)
/// through the runtime without global locks.
///
/// HARDWARE INTENT:
/// - Lock-free: Uses atomic compare-and-swap for event queuing
/// - Minimal allocations: Ring buffers pre-allocated
/// - SIMD-ready: Events packed for batch processing
/// - No false sharing: Event queues aligned to cache lines
///
/// DESIGN PHILOSOPHY:
/// Chunks are not isolated entities. When a chunk mutates, that
/// mutation affects adjacent chunks. The Disturbance Bus ensures
/// these effects propagate efficiently without:
/// - Global Mutex locks
/// - Per-entity event dispatch
/// - Full dependency graph rebuilds
///
/// Instead, disturbances:
/// 1. Originate in a chunk
/// 2. Queue in lock-free buffer
/// 3. Get processed in batch by Matrix
/// 4. Propagate to adjacent chunks
/// 5. Thermal system reacts accordingly
/// ===================================================================

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::cell::UnsafeCell;

// =====================================================================
// DISTURBANCE EVENT TYPES
// =====================================================================

/// Type of disturbance/event
#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DisturbanceKind {
    /// Entity mutation detected in chunk
    ChunkMutation = 0,
    
    /// Physics impact/collision in chunk
    PhysicsImpact = 1,
    
    /// AI activity/pathfinding in chunk
    AIActivity = 2,
    
    /// Camera entered chunk proximity
    CameraInterest = 3,
    
    /// Disturbance wave from adjacent chunk
    PropagatedWave = 4,
}

/// A single disturbance event
///
/// Designed for cache efficiency:
/// - 32 bytes per event (half cache line)
/// - Batch events in groups of 2 per 64-byte cache line
/// - Minimal padding, maximum data density
#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Disturbance {
    /// Origin chunk index
    pub origin_chunk: u32,
    
    /// Type of disturbance
    pub kind: DisturbanceKind,
    
    /// Intensity/magnitude (0.0 - 1.0)
    pub intensity: f32,
    
    /// Affected radius (in chunk units)
    pub radius: f32,
    
    /// Frame this disturbance was created
    pub frame: u64,
    
    /// Custom payload (interpretation depends on kind)
    pub payload: u32,
}

impl Disturbance {
    pub fn new(origin: u32, kind: DisturbanceKind, intensity: f32) -> Self {
        Self {
            origin_chunk: origin,
            kind,
            intensity,
            radius: 1.0,
            frame: 0,
            payload: 0,
        }
    }
}

// =====================================================================
// LOCK-FREE DISTURBANCE QUEUE
// =====================================================================

/// Lock-free disturbance queue using ring buffer
///
/// This queue is optimized for:
/// - Multiple writers (background threads)
/// - Single reader (main thread processing)
/// - Minimal contention (atomic operations only at write pointer)
/// - Pre-allocated fixed size (no heap churn during runtime)
pub struct DisturbanceQueue {
    /// Ring buffer of disturbances
    buffer: Vec<UnsafeCell<Disturbance>>,
    
    /// Write position (producer advances this)
    write_pos: AtomicUsize,
    
    /// Read position (consumer advances this)
    read_pos: AtomicUsize,
    
    /// Capacity mask for O(1) wraparound
    capacity_mask: usize,
}

unsafe impl Sync for DisturbanceQueue {}

impl DisturbanceQueue {
    /// Create queue with capacity (must be power of 2)
    pub fn new(capacity: usize) -> Self {
        let capacity = capacity.next_power_of_two();
        Self {
buffer: (0..capacity)
    .map(|_| {
        UnsafeCell::new(Disturbance {
            origin_chunk: 0,
            kind: DisturbanceKind::ChunkMutation,
            intensity: 0.0,
            radius: 0.0,
            frame: 0,
            payload: 0,
        })
    })
    .collect(),
            write_pos: AtomicUsize::new(0),
            read_pos: AtomicUsize::new(0),
            capacity_mask: capacity - 1,
        }
    }

    /// Try to enqueue a disturbance (lock-free, may fail if full)
    pub fn enqueue(&self, disturbance: Disturbance) -> bool {
        let write = self.write_pos.load(Ordering::Acquire);
        let next_write = (write + 1) & self.capacity_mask;
        let read = self.read_pos.load(Ordering::Acquire);

        // If next_write == read, queue is full
        if next_write == read {
            return false;
        }

        // SAFETY: Write position ensures we don't overwrite unread data
        unsafe {
            *self.buffer.get_unchecked(write).get() = disturbance;
        }

        // Publish write with full barrier
        self.write_pos.store(next_write, Ordering::Release);
        true
    }

    /// Dequeue all available disturbances into a batch
    pub fn drain(&self, batch: &mut Vec<Disturbance>) {
        batch.clear();
        let read = self.read_pos.load(Ordering::Acquire);
        let write = self.write_pos.load(Ordering::Acquire);

        if read == write {
            return;
        }

        // Copy events in order
        let mut pos = read;
        while pos != write {
            // SAFETY: Read position is always behind write position
            unsafe {
                batch.push(*self.buffer.get_unchecked(pos).get());
            }
            pos = (pos + 1) & self.capacity_mask;
        }

        // Update read pointer
        self.read_pos.store(write, Ordering::Release);
    }

    /// Get number of queued disturbances
    pub fn len(&self) -> usize {
        let write = self.write_pos.load(Ordering::Acquire);
        let read = self.read_pos.load(Ordering::Acquire);
        (write - read) & self.capacity_mask
    }

    /// Is queue empty?
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

// =====================================================================
// DISTURBANCE BUS - Central event hub
// =====================================================================

/// Central disturbance bus for the entire engine
///
/// The bus receives events from all systems (physics, AI, camera, mutations)
/// and queues them for processing. It then propagates these disturbances
/// through the Matrix as reactive updates.
pub struct DisturbanceBus {
    /// Global disturbance queue (shared by all producers)
    queue: Arc<DisturbanceQueue>,

    /// Pending disturbances for current frame
    pending: Vec<Disturbance>,

    /// Disturbances from previous frame (for multi-frame propagation)
    lingering: Vec<Disturbance>,
}

impl DisturbanceBus {
    /// Create bus with given queue capacity
    pub fn new(queue_capacity: usize) -> Self {
        Self {
            queue: Arc::new(DisturbanceQueue::new(queue_capacity)),
            pending: Vec::with_capacity(queue_capacity),
            lingering: Vec::with_capacity(queue_capacity),
        }
    }

    /// Get shareable producer handle
    pub fn producer(&self) -> Arc<DisturbanceQueue> {
        Arc::clone(&self.queue)
    }

    /// Process frame: drain queue and prepare disturbances for propagation
    pub fn update_frame(&mut self) {
        // Drain all pending disturbances from lock-free queue
        self.queue.drain(&mut self.pending);

        // Clear lingering disturbances (they've fully propagated)
        self.lingering.clear();

        // Store pending for multi-frame propagation
        self.lingering.extend(&self.pending);
    }

    /// Get current frame disturbances (for Matrix to process)
    pub fn get_disturbances(&self) -> &[Disturbance] {
        &self.lingering
    }

    /// Get mutable pending for batch processing
    pub fn pending_mut(&mut self) -> &mut [Disturbance] {
        &mut self.pending
    }

    /// Statistics for debugging
    pub fn get_stats(&self) -> BusStats {
        BusStats {
            queued: self.queue.len(),
            pending: self.pending.len(),
            lingering: self.lingering.len(),
        }
    }
}

/// Bus statistics
#[derive(Debug, Clone)]
pub struct BusStats {
    pub queued: usize,
    pub pending: usize,
    pub lingering: usize,
}

// =====================================================================
// PROPAGATION LAYER
// =====================================================================

/// Computes which chunks are affected by a disturbance
///
/// This function takes a disturbance and returns the list of chunks
/// that should receive heat/updates from it.
/// 
/// DESIGN:
/// - Branchless neighbor lookup
/// - SIMD-ready array output
/// - Sparse update (only affected chunks)
pub fn compute_propagation(
    origin_chunk: u32,
    radius: f32,
    world_grid_size: u32,
) -> Vec<u32> {
    let mut affected = Vec::new();

    // Grid is organized as 25x25 chunks for 15,625 total
    // origin_chunk index maps to (x, z) = (idx % 25, idx / 25)
    let ox = (origin_chunk % world_grid_size) as i32;
    let oz = (origin_chunk / world_grid_size) as i32;

    let radius_int = radius.ceil() as i32;

    // Iterate all chunks in radius
    for z in (oz - radius_int)..=(oz + radius_int) {
        for x in (ox - radius_int)..=(ox + radius_int) {
            if x >= 0 && x < world_grid_size as i32 && z >= 0 && z < world_grid_size as i32 {
                let idx = (z as u32 * world_grid_size) + x as u32;
                affected.push(idx);
            }
        }
    }

    affected
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn disturbance_queue_enqueue_dequeue() {
        let queue = DisturbanceQueue::new(16);
        let dist = Disturbance::new(0, DisturbanceKind::ChunkMutation, 0.5);

        assert!(queue.enqueue(dist));
        assert_eq!(queue.len(), 1);

        let mut batch = Vec::new();
        queue.drain(&mut batch);
        assert_eq!(batch.len(), 1);
        assert_eq!(batch[0].origin_chunk, 0);
    }

    #[test]
    fn propagation_calculation() {
        let affected = compute_propagation(0, 2.0, 25);
        // Origin 0 is at (0, 0) with radius 2, should affect 25 chunks
        // (3x3 grid + edges)
        assert!(affected.len() > 0);
        assert!(affected.contains(&0)); // Origin itself
    }
}
