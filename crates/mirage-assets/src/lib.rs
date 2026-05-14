use std::path::{Path, PathBuf};
use std::sync::Arc;
use fxhash::FxHashMap;
use uuid::Uuid;
use std::any::Any;

pub mod loader;
pub mod texture_loader;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResourceState {
    Pending,
    Ok,
    GpuReady, 
    LoadError(String),
}

pub struct Resource {
    pub id: Uuid,
    pub path: PathBuf,
    pub state: ResourceState,
    pub data: Option<Arc<dyn Any + Send + Sync>>,
}

pub struct ResourceManager {
    resources: FxHashMap<PathBuf, Arc<Resource>>,
}

impl ResourceManager {
    pub fn new() -> Self {
        Self {
            resources: FxHashMap::default(),
        }
    }

    pub fn request<T: Any + Send + Sync>(&mut self, path: impl AsRef<Path>) -> Arc<Resource> {
        let path_buf = path.as_ref().to_path_buf();
        
        self.resources.entry(path_buf.clone()).or_insert_with(|| {
            Arc::new(Resource {
                id: Uuid::new_v4(),
                path: path_buf,
                state: ResourceState::Pending,
                data: None,
            })
        }).clone()
    }

    // ✅ تم نقل الدالة هنا لتعرف 'self' وتم استخدام '_res' لتجنب التحذير
    pub fn update_status(&mut self, path: &Path, new_state: ResourceState) {
        if let Some(_res) = self.resources.get_mut(path) {
            println!("📢 Resource {:?} changed state to {:?}", path, new_state);
        }
    }
}