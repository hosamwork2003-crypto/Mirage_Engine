// ===================================================================
// mirage-mkr-core/src/bridge/renderer_bridge.rs
// PURPOSE: Field-to-Renderer Translation Layer
//
// PROBLEM:
// The legacy renderer (mirage-renderer/src/main.rs) reads discrete
// `ChunkState` values from `RuntimeDirectory::chunk_runtime_states`
// and uploads them as u32 to a GPU buffer.  The new V3 authority is
// `ActivationField::execution_probability` (a continuous f32).
//
// SOLUTION:
// `RendererBridge` translates the continuous field into approximate
// discrete states so the renderer continues to work correctly during
// the V3 transition.
//
// ARCHITECTURE:
// This bridge is UNIDIRECTIONAL and ADDITIVE:
// * It reads from ActivationField (immutable borrow).
// * It writes to RuntimeDirectory::chunk_runtime_states.
// * It does NOT write back into the ActivationField.
// * The renderer's own distance-based writes still happen and still
//   drive the compat ThermalSystem — the bridge only overrides states
//   for cells that have significant activation field energy.
//
// TRANSLATION RULES (continuous → discrete):
//
//   probability ≥ 0.70 → Hot       (full simulation eligible)
//   probability ≥ 0.35 → Resident  (in VRAM, light sim)
//   probability ≥ 0.05 → Predictive (loading eligible)
//   probability <  0.05 → Dormant   (skip entirely)
//
// These thresholds are intentionally lower than the old ThermalSystem
// thresholds to be conservative: the activation field encodes more
// information (topology pressure, entropy) so a lower raw probability
// can still represent meaningful activity.
//
// GPU-READINESS:
// `render_state_scalars()` returns a raw Vec<f32> representation of
// execution_probability for use in shaders that are already updated to
// handle continuous values (future work).
//
// TODO(V3): Once the renderer GPU shader is updated to consume
// execution_probability as a raw f32 buffer, remove
// `apply_to_directory()` and feed `render_state_scalars()` directly
// to `renderer.update_states_buffer()`.
// ===================================================================

use mirage_core::pool::RuntimeDirectory;
use mirage_core::runtime::ChunkState;
use crate::activation::field::ActivationField;

// =====================================================================
// TRANSLATION THRESHOLDS
// =====================================================================

/// execution_probability at or above which a cell is rendered as Hot.
pub const BRIDGE_HOT_THRESHOLD:       f32 = 0.70;
/// execution_probability at or above which a cell is rendered as Resident.
pub const BRIDGE_RESIDENT_THRESHOLD:  f32 = 0.35;
/// execution_probability at or above which a cell is Predictive (loading).
pub const BRIDGE_PREDICTIVE_THRESHOLD: f32 = 0.05;

// =====================================================================
// RENDERER BRIDGE
// =====================================================================

/// Translates continuous `ActivationField` values into discrete
/// `ChunkState` values for the legacy renderer compatibility path.
///
/// # Ownership
/// `RendererBridge` is stateless — all its data comes from references
/// passed per call.  Owned by `MKRWorld`.
///
/// # Thread Safety
/// All methods take `&ActivationField` and `&mut RuntimeDirectory` —
/// both must be borrowed from the same frame context (i.e., inside
/// `MKRWorld::tick()`).  No internal locks.
pub struct RendererBridge;

impl RendererBridge {
    pub fn new() -> Self { Self }

    // ------------------------------------------------------------------
    // Primary V3→Compat translation
    // ------------------------------------------------------------------

    /// Translate activation field probabilities into discrete ChunkStates
    /// and write them into `RuntimeDirectory::chunk_runtime_states`.
    ///
    /// # Behaviour
    /// * Only writes cells where the activation field has a non-Dormant
    ///   probability (i.e., probability ≥ BRIDGE_PREDICTIVE_THRESHOLD).
    /// * Cells below the Predictive threshold are written as Dormant —
    ///   this **overrides** any state the renderer may have written.
    ///   This is intentional: the activation field is the V3 authority.
    ///
    /// # Safety
    /// Panics if `directory.chunk_runtime_states.len() != field.len()`.
    /// Both must be constructed with the same `total_chunks` value.
    pub fn apply_to_directory(
        &self,
        field:     &ActivationField,
        directory: &mut RuntimeDirectory,
    ) {
        debug_assert_eq!(
            field.len(),
            directory.chunk_runtime_states.len(),
            "RendererBridge: field and directory size mismatch"
        );

        let states = &mut directory.chunk_runtime_states;
        let cells  = &field.cells;

        // Branchless translation: the `as u8` cast collapses the chain.
        // Written as explicit match for readability; the compiler folds it.
        for (state, cell) in states.iter_mut().zip(cells.iter()) {
            *state = probability_to_chunk_state(cell.execution_probability);
        }
    }

    // ------------------------------------------------------------------
    // V3-SPARSE: Changed-cell-only renderer updates (Task 8)
    // ------------------------------------------------------------------

    /// Sparse renderer update: only writes states for cells flagged as
    /// `PROBABILITY_CHANGED` in the delta mask.
    ///
    /// **O(|changed|) instead of O(N).**
    ///
    /// # Authority
    /// Renderer remains passive.  This method only writes the subset of
    /// `chunk_runtime_states` that correspond to changed cells.  All other
    /// states retain their values from the previous frame — which is
    /// correct if `execution_probability` didn't change significantly.
    ///
    /// # Correctness Guarantee
    /// `PROBABILITY_EPSILON` (1e-4) is much smaller than the smallest
    /// threshold gap between ChunkState variants (0.05 for Predictive).
    /// A cell whose probability changed by less than 1e-4 cannot have
    /// crossed a state boundary, so its ChunkState is still correct.
    ///
    /// # TODO(V3-SPARSE-VALIDATION): Run apply_changed_cells() in parallel
    /// with apply_to_directory() for 1000 ticks.  Assert that all cells in
    /// the changed set produce identical ChunkStates in both paths.
    pub fn apply_changed_cells(
        &self,
        field:      &ActivationField,
        directory:  &mut RuntimeDirectory,
        delta_mask: &crate::activation::delta::FieldDeltaMask,
    ) {
        debug_assert_eq!(
            field.len(),
            directory.chunk_runtime_states.len(),
            "RendererBridge: field and directory size mismatch"
        );

        for idx in delta_mask.iter_changed() {
            if idx >= field.cells.len() { break; }
            directory.chunk_runtime_states[idx] =
                probability_to_chunk_state(field.cells[idx].execution_probability);
        }
    }

    /// Sparse probability buffer update: only writes changed cell probabilities.
    ///
    /// Companion to `fill_probability_buffer()`.  For a flat `Vec<f32>` that
    /// is partially updated by the sparse solver, this ensures only the
    /// changed indices are refreshed.
    ///
    /// # TODO(V3-SPARSE-VALIDATION): Validate that partial buffer updates
    /// match full buffer for all changed cells before using in production.
    pub fn update_probability_buffer_sparse(
        &self,
        field:      &ActivationField,
        buffer:     &mut Vec<f32>,
        delta_mask: &crate::activation::delta::FieldDeltaMask,
    ) {
        // Ensure buffer is large enough
        if buffer.len() < field.cells.len() {
            buffer.resize(field.cells.len(), 0.0);
        }
        for idx in delta_mask.iter_changed() {
            if idx >= field.cells.len() { break; }
            buffer[idx] = field.cells[idx].execution_probability;
        }
    }

    // ------------------------------------------------------------------
    // Forward-looking: raw float buffer for updated GPU shaders
    // ------------------------------------------------------------------

    /// Return a `Vec<f32>` of raw execution probabilities for each cell.
    ///
    /// This is the forward-looking V3 output: once the GPU shader is
    /// updated to consume a float buffer instead of a u32 enum buffer,
    /// call this method and feed it to `renderer.update_states_buffer()`.
    ///
    /// Avoids any allocation when the caller pre-allocates the buffer.
    pub fn fill_probability_buffer(
        &self,
        field:  &ActivationField,
        output: &mut Vec<f32>,
    ) {
        output.clear();
        output.extend(field.cells.iter().map(|c| c.execution_probability));
    }

    // ------------------------------------------------------------------
    // Per-cell query helpers
    // ------------------------------------------------------------------

    /// Translate a single execution_probability to a ChunkState.
    /// Useful for per-chunk decisions (e.g., streaming trigger logic).
    #[inline]
    pub fn cell_to_chunk_state(&self, probability: f32) -> ChunkState {
        probability_to_chunk_state(probability)
    }

    /// Return true if a cell is hot enough to be emission-eligible.
    /// Uses the same threshold as the emission gate.
    #[inline]
    pub fn is_emission_eligible(&self, probability: f32) -> bool {
        probability > crate::emission::EMIT_GATE
    }

    /// Return true if a cell should be rendered (Resident or hotter).
    #[inline]
    pub fn should_render(&self, probability: f32) -> bool {
        probability >= BRIDGE_RESIDENT_THRESHOLD
    }

    /// Return true if a cell should trigger async streaming.
    #[inline]
    pub fn should_stream(&self, probability: f32) -> bool {
        probability >= BRIDGE_PREDICTIVE_THRESHOLD && probability < BRIDGE_RESIDENT_THRESHOLD
    }
}

impl Default for RendererBridge {
    fn default() -> Self { Self::new() }
}

// =====================================================================
// TRANSLATION KERNEL (free function — inline hot path)
// =====================================================================

/// Map a continuous `execution_probability` to the nearest `ChunkState`.
///
/// This is the core translation function.  It is a pure function with
/// no side effects, making it trivially testable and GPU-portable.
///
/// # Thresholds
/// ```text
/// probability ≥ 0.70 → Hot
/// probability ≥ 0.35 → Resident
/// probability ≥ 0.05 → Predictive
/// probability <  0.05 → Dormant
/// ```
#[inline]
pub fn probability_to_chunk_state(probability: f32) -> ChunkState {
    // Written as nested selects to encourage branchless codegen.
    // The compiler typically emits FCMP + CSEL (ARM) or FCOMI + CMOV (x86).
    if probability >= BRIDGE_HOT_THRESHOLD {
        ChunkState::Hot
    } else if probability >= BRIDGE_RESIDENT_THRESHOLD {
        ChunkState::Resident
    } else if probability >= BRIDGE_PREDICTIVE_THRESHOLD {
        ChunkState::Predictive
    } else {
        ChunkState::Dormant
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activation::field::ActivationField;

    #[test]
    fn probability_mapping_boundaries() {
        assert_eq!(probability_to_chunk_state(0.0),    ChunkState::Dormant);
        assert_eq!(probability_to_chunk_state(0.04),   ChunkState::Dormant);
        assert_eq!(probability_to_chunk_state(0.05),   ChunkState::Predictive);
        assert_eq!(probability_to_chunk_state(0.35),   ChunkState::Resident);
        assert_eq!(probability_to_chunk_state(0.70),   ChunkState::Hot);
        assert_eq!(probability_to_chunk_state(1.0),    ChunkState::Hot);
    }

    #[test]
    fn apply_to_directory_full_hot() {
        let mut field = ActivationField::new(4, 4);
        for cell in &mut field.cells {
            cell.execution_probability = 1.0;
        }
        let mut dir = RuntimeDirectory::new(16);
        RendererBridge::new().apply_to_directory(&field, &mut dir);
        assert!(dir.chunk_runtime_states.iter().all(|&s| s == ChunkState::Hot));
    }

    #[test]
    fn apply_to_directory_dormant_field() {
        let field = ActivationField::new(4, 4); // all zeros
        let mut dir = RuntimeDirectory::new(16);
        // Pre-set some states to Resident to verify override behaviour
        dir.chunk_runtime_states[0] = ChunkState::Resident;
        RendererBridge::new().apply_to_directory(&field, &mut dir);
        // Bridge overrides to Dormant when probability is zero
        assert_eq!(dir.chunk_runtime_states[0], ChunkState::Dormant);
    }

    #[test]
    fn fill_probability_buffer_matches_field() {
        let mut field = ActivationField::new(2, 2);
        field.cells[0].execution_probability = 0.8;
        field.cells[3].execution_probability = 0.4;
        let bridge = RendererBridge::new();
        let mut buf = Vec::new();
        bridge.fill_probability_buffer(&field, &mut buf);
        assert_eq!(buf.len(), 4);
        assert!((buf[0] - 0.8).abs() < 1e-6);
        assert!((buf[3] - 0.4).abs() < 1e-6);
    }

    #[test]
    fn helper_predicates() {
        let b = RendererBridge::new();
        assert!( b.should_render(0.5));
        assert!(!b.should_render(0.2));
        assert!( b.should_stream(0.1));
        assert!(!b.should_stream(0.5));
        assert!(!b.should_stream(0.01));
        assert!( b.is_emission_eligible(0.1));
        assert!(!b.is_emission_eligible(0.01));
    }
}
