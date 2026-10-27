//! Optional Vello pipeline cache in the native host's private directory, never app documents.

use std::path::{Path, PathBuf};

pub(super) struct Cache {
    pub gpu: wgpu::PipelineCache,
    pub loaded: bool,
    path: PathBuf,
}

impl Cache {
    pub fn open(adapter: &wgpu::Adapter, device: &wgpu::Device, directory: &Path) -> Option<Self> {
        if !device.features().contains(wgpu::Features::PIPELINE_CACHE) { return None; }
        let key = wgpu::util::pipeline_cache_key(&adapter.get_info())?;
        std::fs::create_dir_all(directory).ok()?;
        let path = directory.join(format!("vello09-wgpu29-{key}.bin"));
        let data = std::fs::metadata(&path).ok().filter(|m| m.len() <= 32 * 1024 * 1024)
            .and_then(|_| std::fs::read(&path).ok());
        // This host-private file is written only by save(), using PipelineCache::get_data.
        // wgpu validates device/driver compatibility and falls back when the cache is stale.
        let gpu = unsafe { device.create_pipeline_cache(&wgpu::PipelineCacheDescriptor {
            label: Some("Vello Android startup cache"), data: data.as_deref(), fallback: true,
        }) };
        Some(Self { gpu, loaded: data.is_some(), path })
    }

    pub fn save(&self) -> std::io::Result<()> {
        if let Some(data) = self.gpu.get_data() {
            let temporary = self.path.with_extension("tmp");
            std::fs::write(&temporary, data)?;
            std::fs::rename(temporary, &self.path)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
