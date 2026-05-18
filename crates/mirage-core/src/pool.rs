// ===================================================================
// mirage-core/src/pool.rs
// PURPOSE: RuntimeDirectory — Entity Handle Registry (V2 Compat Stub)
//
// TODO(V3): [RuntimeDirectory] This is a V2 compatibility stub.
// In V3, chunk addressing will use ActivationField cell indices (usize)
// as the primary key.  The UUID→Handle→AddressMapping lookup chain
// should be replaced by a flat `field_index → StreamingHandle` table
// aligned to the activation field grid.
//
// TODO(V3): [Handle] Replace with a `FieldCellHandle(usize)` newtype
// that is a direct index into `ActivationField::cells`.  Generation
// tracking should move to a separate sparse generational array that is
// only consulted during streaming I/O, not on every hot-path access.
//
// TODO(V3): [AddressMapping] Replace page_id/chunk_idx/slot_idx with
// a V3 StreamingDescriptor { field_index: usize, oasis_page_id: u32 }
// so that the streaming layer and the activation field share the same
// primary key space.
// ===================================================================

/// Generation-tracked index handle for entity addressing.
///
/// TODO(V3): Replace with FieldCellHandle(usize).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(C)]
pub struct Handle {
    pub index:      u32,
    pub generation: u32,
}

impl Handle {
    pub const NONE: Self = Self { index: 0, generation: 0 };

    pub fn new(index: u32, generation: u32) -> Self {
        Self { index, generation }
    }

    #[inline(always)]
    pub fn index(&self) -> u32 { self.index }

    #[inline(always)]
    pub fn generation(&self) -> u32 { self.generation }
}

/// Address of an entity within the chunk/page system.
///
/// TODO(V3): Replace with StreamingDescriptor { field_index, oasis_page_id }.
#[derive(Debug, Clone, Copy)]
pub struct AddressMapping {
    pub page_id:    u32,
    pub chunk_idx:  u32,
    pub slot_idx:   u32,
    pub generation: u32,
    pub is_alive:   bool,
}

/// V2-compat entity registry + chunk state mirror.
///
/// # V3 Role
/// `chunk_runtime_states` is the bridge that allows `mirage-renderer`
/// to continue reading discrete `ChunkState` values while the
/// `ActivationField` is the real execution authority.
///
/// The renderer writes distances to this vec; the V3 bridge (see
/// `mirage-mkr-core/src/bridge/renderer_bridge.rs`) will eventually
/// translate `execution_probability` back into approximate render states
/// so the renderer gets V3-correct output without code changes.
///
/// TODO(V3): Once the renderer reads `execution_probability` directly,
/// remove `chunk_runtime_states` and collapse this struct into a thin
/// FieldIndex→OasisPageId lookup.
pub struct RuntimeDirectory {
    /// Discrete chunk states for renderer / legacy compat.
    /// Written by the renderer event loop; read by GPU upload.
    pub chunk_runtime_states: Vec<crate::runtime::ChunkState>,
    /// Entity address table for Handle lookups.
    address_table: Vec<AddressMapping>,
}

impl RuntimeDirectory {
    pub fn new(total_chunks: usize) -> Self {
        Self {
            chunk_runtime_states: vec![crate::runtime::ChunkState::Dormant; total_chunks],
            address_table: Vec::new(),
        }
    }

    /// Register an entity and return its Handle.
    ///
    /// `_uuid` is accepted for backward compatibility with the `NeuralCluster`
    /// proc-macro which generates calls of the form
    /// `register_entity(uuid, page_id, chunk_idx, slot_idx)`.
    ///
    /// TODO(V3): Remove `_uuid` parameter once the macro is updated to use
    /// field-cell indices as the primary addressing scheme.
    pub fn register_entity(
        &mut self,
        _uuid:     crate::oasis::uuid::MirageUuid,
        page_id:   u32,
        chunk_idx: u32,
        slot_idx:  u32,
    ) -> Handle {
        let generation = 1;
        let index = self.address_table.len() as u32;
        self.address_table.push(AddressMapping {
            page_id,
            chunk_idx,
            slot_idx,
            generation,
            is_alive: true,
        });
        Handle::new(index, generation)
    }

    /// Resolve a Handle to its AddressMapping, or None if stale/invalid.
    pub fn get_mapping(&self, handle: Handle) -> Option<AddressMapping> {
        let m = self.address_table.get(handle.index() as usize)?;
        if m.is_alive && m.generation == handle.generation() {
            Some(*m)
        } else {
            None
        }
    }

    /// Return chunk states as u32 values for GPU buffer upload.
    ///
    /// Each u32 encodes: 0=Dormant, 1=Predictive, 2=Resident, 3=Hot.
    /// TODO(V3): Replace with `execution_probability` float buffer once
    /// the GPU shaders are updated to consume continuous values.
    pub fn get_raw_states(&self) -> Vec<u32> {
        self.chunk_runtime_states
            .iter()
            .map(|&s| u32::from(s))
            .collect()
    }
}
