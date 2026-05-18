// ===================================================================
// mirage-mkr-core/src/pool/mod.rs
// PURPOSE: RuntimeDirectory — Entity Handle Registry
//
// TODO(V3-COMPAT): RuntimeDirectory is a V2 compatibility structure.
// In V3, chunk addressing will migrate to ActivationField cell indices
// rather than UUID-to-handle lookup tables.  This module is retained
// so that existing code continues to compile during the transition.
// ===================================================================

pub mod handle;
pub use handle::Handle;

use std::collections::HashMap;

/// Lightweight entity identity key.
///
/// TODO(V3-COMPAT): In V3 this will be replaced by a field-cell index
/// (usize) once the streaming layer is redesigned.  The UUID abstraction
/// is retained for compat-only code paths.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct LocalUuid(pub [u8; 16]);

impl LocalUuid {
    /// Create a zero UUID (placeholder / unassigned).
    pub const fn zero() -> Self {
        Self([0u8; 16])
    }
}

#[derive(Debug, Clone, Copy)]
pub struct AddressMapping {
    pub page_id:    u32,
    pub chunk_idx:  u32,
    pub slot_idx:   u32,
    pub generation: u32,
    pub is_alive:   bool,
}

/// TODO(V3-COMPAT): RuntimeDirectory is a compatibility structure.
/// Its design mirrors the old V2 entity registry.  It will be redesigned
/// once the ActivationField becomes the primary chunk addressing mechanism.
pub struct RuntimeDirectory {
    uuid_to_handle: HashMap<LocalUuid, Handle>,
    address_table:  Vec<AddressMapping>,
    /// TODO(V3-POOL-2): free_slots will become the slab-allocator free-list
    /// for the V3 StreamingDescriptor table.  Retained now to avoid API
    /// churn during the compat transition.
    #[allow(dead_code)]
    free_slots:     Vec<u32>,
}

impl RuntimeDirectory {
    pub fn new(_total_chunks: usize) -> Self {
        Self {
            uuid_to_handle: HashMap::new(),
            address_table:  Vec::new(),
            free_slots:     Vec::new(),
        }
    }

    pub fn register_entity(
        &mut self,
        uuid:      LocalUuid,
        page_id:   u32,
        chunk_idx: u32,
        slot_idx:  u32,
    ) -> Handle {
        let generation = 1;
        let mapping = AddressMapping {
            page_id,
            chunk_idx,
            slot_idx,
            generation,
            is_alive: true,
        };
        let index = self.address_table.len() as u32;
        self.address_table.push(mapping);
        let handle = Handle::new(index, generation);
        self.uuid_to_handle.insert(uuid, handle);
        handle
    }

    pub fn get_mapping(&self, handle: Handle) -> Option<AddressMapping> {
        let mapping = self.address_table.get(handle.index() as usize)?;
        if mapping.is_alive && mapping.generation == handle.generation() {
            Some(*mapping)
        } else {
            None
        }
    }
}