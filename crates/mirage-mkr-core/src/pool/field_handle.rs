// ===================================================================
// mirage-mkr-core/src/pool/field_handle.rs
// PURPOSE: FieldCellHandle — V3 Primary Addressing Type
//
// TRANSITION CONTEXT:
// V2 addressing:  UUID → Handle → AddressMapping → ChunkState
// V3 addressing:  FieldCellIndex → ActivationField::cells[index]
//
// FieldCellHandle is the V3 address primitive.  It is a newtype over
// usize that directly indexes ActivationField::cells.
//
// KEY PROPERTIES vs. old Handle:
// * No UUID lookup — direct array index.
// * No generation check on hot path — field cells are persistent.
// * No page_id / slot_idx indirection — one coordinate, the field index.
// * O(1) access instead of O(map_lookup) + O(table_access).
//
// COMPATIBILITY BRIDGE:
// `FieldCellHandle::from_legacy_chunk_idx` converts an old chunk index
// (u32) directly to a FieldCellHandle, enabling gradual migration:
//
//   old:  directory.chunk_runtime_states[chunk_idx]
//   new:  field.cells[FieldCellHandle::from_legacy(chunk_idx).index()]
//
// STREAMING DESCRIPTOR:
// `StreamingDescriptor` replaces AddressMapping for the streaming path.
// It pairs a FieldCellHandle with an OASIS page reference so the
// streaming layer and activation field share one key space.
//
// IS CURRENTLY SAFE TO REPLACE Handle WITH: PARTIALLY.
//   New code must use FieldCellHandle.
//   Old code using `Handle` is still supported via `from_legacy_handle`.
//   Full removal of Handle requires mirage-matrix-macros migration.
//
// TODO(V3-POOL-1): Update NeuralCluster macro to emit FieldCellHandle
//   instead of Handle+UUID.
// TODO(V3-POOL-2): Remove Handle from RuntimeDirectory once all callers
//   use FieldCellHandle-based lookups.
// TODO(V3-POOL-3): Remove AddressMapping once StreamingDescriptor is
//   the canonical streaming address type everywhere.
// ===================================================================

// =====================================================================
// FIELD CELL HANDLE — Primary V3 address
// =====================================================================

/// Direct index into `ActivationField::cells`.
///
/// This is the primary chunk-addressing type in V3.  It replaces the
/// UUID → Handle → AddressMapping chain with a single flat index.
///
/// # Safety
/// No bounds checking occurs inside `index()`.  The caller must
/// ensure the handle was created for the same field it is used on.
/// Use `ActivationField::index_of(x, y)` to construct safe handles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct FieldCellHandle(usize);

impl FieldCellHandle {
    /// Create a `FieldCellHandle` from a raw flat field index.
    #[inline(always)]
    pub const fn new(index: usize) -> Self {
        Self(index)
    }

    /// Create from a chunk grid coordinate pair `(x, y)` and field width.
    ///
    /// Equivalent to `y * width + x`.  Does NOT check bounds.
    #[inline(always)]
    pub const fn from_grid(x: usize, y: usize, width: usize) -> Self {
        Self(y * width + x)
    }

    /// Convert from a legacy `chunk_idx: u32` (V2 runtime index).
    ///
    /// Use during the migration period to convert old chunk indices
    /// to FieldCellHandle without modifying call sites.
    #[inline(always)]
    pub const fn from_legacy_chunk_idx(chunk_idx: u32) -> Self {
        Self(chunk_idx as usize)
    }

    /// Convert from an old-style `Handle` (V2 entity handle).
    ///
    /// Maps `handle.index()` directly to a field cell index.
    /// Generation is discarded — V3 cells are persistent, not generational.
    ///
    /// TODO(V3-POOL-2): Remove once Handle is no longer used.
    #[inline(always)]
    pub fn from_legacy_handle(handle: &mirage_core::pool::Handle) -> Self {
        Self(handle.index() as usize)
    }

    /// Raw flat field index.
    #[inline(always)]
    pub const fn index(self) -> usize {
        self.0
    }

    /// Convert back to a legacy `u32` chunk index.
    ///
    /// TODO(V3-POOL-2): Remove once chunk_runtime_states is gone.
    #[inline(always)]
    pub const fn as_chunk_idx(self) -> u32 {
        self.0 as u32
    }
}

impl From<usize> for FieldCellHandle {
    fn from(idx: usize) -> Self { Self(idx) }
}

impl From<u32> for FieldCellHandle {
    fn from(idx: u32) -> Self { Self(idx as usize) }
}

// =====================================================================
// STREAMING DESCRIPTOR — V3 streaming address
// =====================================================================

/// V3 streaming address: combines a field cell index with an OASIS page ref.
///
/// Replaces `AddressMapping { page_id, chunk_idx, slot_idx, ... }` for
/// the streaming path.  The two fields share a single key space:
///
/// * `field_handle` — which activation cell to heat on completion.
/// * `oasis_page_id` — which OASIS virtual page to load from.
///
/// TODO(V3-POOL-3): Migrate StreamingFabric to accept StreamingDescriptor
/// instead of raw (page_id: u32, chunk_idx: u32) pairs.
#[derive(Debug, Clone, Copy)]
pub struct StreamingDescriptor {
    /// V3 primary key — indexes ActivationField::cells directly.
    pub field_handle:   FieldCellHandle,
    /// OASIS virtual page containing this chunk's data.
    pub oasis_page_id:  u32,
}

impl StreamingDescriptor {
    pub fn new(field_handle: FieldCellHandle, oasis_page_id: u32) -> Self {
        Self { field_handle, oasis_page_id }
    }

    /// Convert from legacy (chunk_idx, page_id) pair.
    pub fn from_legacy(chunk_idx: u32, page_id: u32) -> Self {
        Self {
            field_handle:  FieldCellHandle::from_legacy_chunk_idx(chunk_idx),
            oasis_page_id: page_id,
        }
    }
}

// =====================================================================
// TESTS
// =====================================================================

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn field_cell_handle_from_grid() {
        // Row-major: (x=2, y=3, width=10) → index = 3*10 + 2 = 32
        let h = FieldCellHandle::from_grid(2, 3, 10);
        assert_eq!(h.index(), 32);
    }

    #[test]
    fn from_legacy_chunk_idx_is_identity() {
        let h = FieldCellHandle::from_legacy_chunk_idx(42);
        assert_eq!(h.index(), 42);
        assert_eq!(h.as_chunk_idx(), 42);
    }

    #[test]
    fn streaming_descriptor_from_legacy() {
        let sd = StreamingDescriptor::from_legacy(7, 3);
        assert_eq!(sd.field_handle.index(), 7);
        assert_eq!(sd.oasis_page_id, 3);
    }

    #[test]
    fn field_cell_handle_ordering() {
        let a = FieldCellHandle::new(5);
        let b = FieldCellHandle::new(10);
        assert!(a < b);
    }
}
