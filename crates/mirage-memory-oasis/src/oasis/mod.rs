pub mod uuid;
pub mod streamer;

pub use uuid::MirageUuid;
pub use streamer::{StreamingFabric, StreamWorker, StreamRequest, StreamResult};
use memmap2::Mmap;
use std::sync::Arc;

pub struct OasisVirtualPage { pub page_id: u32, pub data: Mmap }
pub struct OasisManager { pub pages: Vec<Arc<OasisVirtualPage>> }

impl OasisManager {
    pub fn new() -> Self { Self { pages: Vec::new() } }
    
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