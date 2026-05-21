/// ===================================================================
/// mirage-renderer/src/residency.rs  (V3 — Federated Stabilization Pass)
/// PURPOSE: GPU Residency Tracker — COMPAT AUTHORITY (scheduled migration)
///
/// ---------------------------------------------------------------
/// V3-RENDERER-PASSIVE: CURRENT OWNERSHIP RISK
/// ---------------------------------------------------------------
///
/// ResidencyTracker currently lives in mirage-renderer and makes
/// autonomous residency decisions (request_load, evict, mark_dirty).
/// This is an AUTHORITY VIOLATION in the V3 federated architecture.
///
/// TODO(V3-RENDERER-PASSIVE): Residency authority must move to OASIS.
///   * ResidencyTracker should be consumed by the renderer passively.
///   * The renderer should NOT call request_load() or evict() —
///     those decisions belong to OASIS::StreamingFabric.
///   * Renderer should only call is_loaded() / get_stats() for display.
///   * Upload batching (get_upload_batch) can remain renderer-side as
///     a GPU-facing adapter, but the state it reads must come from OASIS.
///
/// TODO(V3-RENDERER-PASSIVE): ResidencyDescriptor protocol type
///   (see mirage-mkr-core/src/pool/field_handle.rs::StreamingDescriptor)
///   should replace raw (chunk_idx: u32) in all residency APIs.
///   This unifies the field-index key space with the OASIS page address.
///
/// TODO(V3-RENDERER-PASSIVE): Renderer must NOT infer thermal state from
///   residency state.  Thermal state is derived ONLY from ActivationField
///   via RendererBridge.  ResidencyState is a GPU memory concern, not a
///   scheduling concern.
///
/// ---------------------------------------------------------------
/// HARDWARE INTENT (retained from V2 — still valid)
/// ---------------------------------------------------------------
///
/// - Sparse residency: Only hot/resident chunks in VRAM
/// - Partial uploads: Dirty chunks only, not full buffers
/// - Predictive eviction: Remove chunks before camera leaves
/// - Upload budgeting: Limit bandwidth per frame
/// ===================================================================

/// GPU residency state for a chunk
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResidencyState {
    /// Not in VRAM
    Evicted,
    /// Loading to VRAM (in flight)
    Loading,
    /// Loaded in VRAM but not dirty
    Resident,
    /// Loaded in VRAM and needs update
    Dirty,
}

/// Tracks GPU residency for all chunks
pub struct ResidencyTracker {
    /// Residency state of each chunk
    states: Vec<ResidencyState>,
    
    /// Dirty chunks ready for upload
    dirty_chunks: Vec<u32>,
    
    /// Max upload bandwidth (bytes per frame)
    upload_budget: usize,
    
    /// Current frame upload used
    frame_upload_used: usize,
}

impl ResidencyTracker {
    pub fn new(num_chunks: usize) -> Self {
        Self {
            states: vec![ResidencyState::Evicted; num_chunks],
            dirty_chunks: Vec::new(),
            upload_budget: 16 * 1024 * 1024, // 16 MB per frame
            frame_upload_used: 0,
        }
    }

    /// Mark chunk as dirty (needs GPU update)
    pub fn mark_dirty(&mut self, chunk_idx: u32) {
        if let Some(state) = self.states.get_mut(chunk_idx as usize) {
            if *state == ResidencyState::Resident {
                *state = ResidencyState::Dirty;
                self.dirty_chunks.push(chunk_idx);
            }
        }
    }

    /// Get chunks ready for upload (respecting budget)
    pub fn get_upload_batch(&mut self) -> Vec<u32> {
        let mut batch = Vec::new();
        let chunk_size = 3072; // CHUNK_SIZE_BYTES
        
        self.frame_upload_used = 0;
        
        for &chunk_idx in &self.dirty_chunks {
            if self.frame_upload_used + chunk_size <= self.upload_budget {
                batch.push(chunk_idx);
                self.frame_upload_used += chunk_size;
                
                // Mark as resident (upload complete in simulation)
                if let Some(state) = self.states.get_mut(chunk_idx as usize) {
                    *state = ResidencyState::Resident;
                }
            }
        }
        
        self.dirty_chunks.clear();
        batch
    }

    /// Request chunk loading to VRAM
    pub fn request_load(&mut self, chunk_idx: u32) {
        if let Some(state) = self.states.get_mut(chunk_idx as usize) {
            *state = ResidencyState::Loading;
        }
    }

    /// Complete loading (chunk now in VRAM)
    pub fn complete_load(&mut self, chunk_idx: u32) {
        if let Some(state) = self.states.get_mut(chunk_idx as usize) {
            *state = ResidencyState::Resident;
        }
    }

    /// Evict chunk from VRAM
    pub fn evict(&mut self, chunk_idx: u32) {
        if let Some(state) = self.states.get_mut(chunk_idx as usize) {
            *state = ResidencyState::Evicted;
        }
    }

    /// Get residency statistics
    pub fn get_stats(&self) -> ResidencyStats {
        let mut evicted = 0;
        let mut loading = 0;
        let mut resident = 0;
        let mut dirty = 0;

        for &state in &self.states {
            match state {
                ResidencyState::Evicted => evicted += 1,
                ResidencyState::Loading => loading += 1,
                ResidencyState::Resident => resident += 1,
                ResidencyState::Dirty => dirty += 1,
            }
        }

        ResidencyStats {
            evicted,
            loading,
            resident,
            dirty,
            upload_budget: self.upload_budget,
            frame_used: self.frame_upload_used,
        }
    }
}

/// Residency statistics for debugging
#[derive(Debug, Clone)]
pub struct ResidencyStats {
    pub evicted: usize,
    pub loading: usize,
    pub resident: usize,
    pub dirty: usize,
    pub upload_budget: usize,
    pub frame_used: usize,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn residency_tracker_creation() {
        let tracker = ResidencyTracker::new(100);
        assert!(tracker.states.iter().all(|&s| s == ResidencyState::Evicted));
    }

    #[test]
    fn residency_transitions() {
        let mut tracker = ResidencyTracker::new(100);
        
        tracker.request_load(0);
        assert_eq!(tracker.states[0], ResidencyState::Loading);
        
        tracker.complete_load(0);
        assert_eq!(tracker.states[0], ResidencyState::Resident);
        
        tracker.mark_dirty(0);
        assert_eq!(tracker.states[0], ResidencyState::Dirty);
        
        tracker.evict(0);
        assert_eq!(tracker.states[0], ResidencyState::Evicted);
    }
}
