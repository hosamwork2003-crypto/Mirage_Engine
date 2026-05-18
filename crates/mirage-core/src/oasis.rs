// ===================================================================
// mirage-core/src/oasis.rs
// PURPOSE: Oasis layer — entity identity + virtual page management
//
// TODO(V3): [OasisManager] The OasisManager currently lives here as a
// thin compatibility shim.  In V3, the streaming layer should be split:
//   1. MirageUuid → retained for serialised asset manifests only.
//   2. OasisManager → should become `StreamingCoordinator` that owns a
//      channel to the async streaming thread and exposes:
//        - queue_stream(field_index, oasis_page_id) -> StreamHandle
//        - poll_ready() -> impl Iterator<Item = (field_index, Vec<u8>)>
//      StreamingCoordinator feeds directly into
//      MKRWorld::inject_heat_at_chunk() on completion, eliminating the
//      manual streaming loop in mirage-renderer's main.rs.
//
// TODO(V3): [MirageUuid] In V3, chunk addressing uses ActivationField
// cell indices (usize).  MirageUuid is retained for V2 compat only.
// ===================================================================

pub mod uuid {
    use serde::{Deserialize, Serialize};

    /// 128-bit entity identity key.
    ///
    /// TODO(V3): Replace chunk-level addressing with field cell index.
    #[derive(Serialize, Deserialize, Debug, Clone, Copy, PartialEq, Eq, Hash)]
    #[repr(transparent)]
    pub struct MirageUuid(pub [u8; 16]);

    impl MirageUuid {
        pub const fn zero() -> Self { Self([0u8; 16]) }
        pub fn new() -> Self {
            let id = uuid::Uuid::new_v4();
            Self(id.into_bytes())
        }
    }

    impl Default for MirageUuid {
        fn default() -> Self { Self::new() }
    }
}

// Re-export MirageUuid at the oasis level for compat imports.
pub use uuid::MirageUuid;

// ===================================================================
// OasisManager — Virtual Page Loader (V2 compat stub)
//
// TODO(V3): Replace with StreamingCoordinator that feeds activation
// heat injection on streaming completion.  See module-level doc above.
// ===================================================================

use memmap2::Mmap;
use std::sync::Arc;

/// A memory-mapped virtual page backing a set of chunks.
pub struct OasisVirtualPage {
    pub page_id: u32,
    pub data:    Mmap,
}

/// Global virtual-page manager.
///
/// TODO(V3): [StreamingCoordinator] Replace with async streaming that
/// injects heat into the ActivationField on completion instead of
/// setting discrete `ChunkState::Resident` in the renderer.
pub struct OasisManager {
    pub pages: Vec<Arc<OasisVirtualPage>>,
}

impl OasisManager {
    pub fn new() -> Self {
        Self { pages: Vec::new() }
    }

    /// Load raw chunk data from the virtual page at `page_id`.
    /// Returns a zeroed buffer if the page is not loaded.
    ///
    /// TODO(V3): This should become an async channel message so the
    /// streaming thread delivers data without blocking the render loop.
    pub fn load_chunk_data(&self, page_id: u32, chunk_idx: u32) -> Vec<u8> {
        let chunk_size_bytes = 3072;
        for page in &self.pages {
            if page.page_id == page_id {
                let offset = (chunk_idx as usize) * chunk_size_bytes;
                if offset + chunk_size_bytes <= page.data.len() {
                    return page.data[offset..offset + chunk_size_bytes].to_vec();
                }
            }
        }
        vec![0u8; chunk_size_bytes]
    }
}

impl Default for OasisManager {
    fn default() -> Self { Self::new() }
}
