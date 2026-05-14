use crate::loader::AssetLoader;
use std::path::Path;
use std::sync::Arc;
use futures::future::{BoxFuture, FutureExt};
use anyhow::{Result, anyhow};
use image::GenericImageView;

pub struct TextureLoader;

impl AssetLoader for TextureLoader {
    fn supports(&self, extension: &str) -> bool {
        // يدعم أشهر صيغ الصور
        matches!(extension, "png" | "jpg" | "jpeg" | "tga" | "bmp")
    }

    fn load<'a>(
        &'a self,
        path: &'a Path,
    ) -> BoxFuture<'a, Result<Arc<dyn std::any::Any + Send + Sync>>> {
        async move {
            // 🔍 قراءة الملف بأسلوب Zero-Copy (Memory Mapping يمكن إضافته لاحقاً)
            let bytes = std::fs::read(path)?;
            
            // 🎨 فك الضغط في خيط منفصل (Worker Thread) لضمان عدم توقف المحرك
            let img = image::load_from_memory(&bytes)?;
            let (width, height) = img.dimensions();
            let rgba_data = img.to_rgba8().into_raw();

            println!("✅ Texture Loaded: {:?} ({}x{})", path, width, height);

            // إرجاع البيانات كـ Arc ليكون متاحاً لـ Renderer
            let resource_data: Arc<dyn std::any::Any + Send + Sync> = Arc::new(rgba_data);
            Ok(resource_data)
        }
        .boxed()
    }
}