use std::path::Path;
use std::sync::Arc;
use futures::future::BoxFuture;
use anyhow::Result;

/// 🧠 واجهة محمل الموارد (The Asset Nerve)
/// مقتبسة من Loader الخاص بـ Fyrox ولكن مبسطة لـ Mirage
pub trait AssetLoader: Send + Sync {
    /// هل يدعم هذا المحمل هذا الامتداد؟ (مثلاً .png أو .fbx)
    fn supports(&self, extension: &str) -> bool;

    /// عملية التحميل الفعلية (Async Zero-Copy Task)
    fn load<'a>(
        &'a self,
        path: &'a Path,
    ) -> BoxFuture<'a, Result<Arc<dyn std::any::Any + Send + Sync>>>;
}

/// 📦 حاوية المحملات
pub struct LoaderRegistry {
    loaders: Vec<Box<dyn AssetLoader>>,
}

impl LoaderRegistry {
    pub fn new() -> Self {
        Self { loaders: Vec::new() }
    }

    pub fn register<L: AssetLoader + 'static>(&mut self, loader: L) {
        self.loaders.push(Box::new(loader));
    }

    pub fn find_for_path(&self, path: &Path) -> Option<&dyn AssetLoader> {
        let ext = path.extension()?.to_str()?.to_lowercase();
        self.loaders.iter().find(|l| l.supports(&ext)).map(|l| l.as_ref())
    }
}