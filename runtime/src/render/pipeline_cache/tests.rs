use super::*;

#[test]
#[ignore = "needs a GPU adapter"]
fn vello_pipeline_cache_survives_a_new_device() {
    let directory = std::env::temp_dir().join(format!("osvauld-gpu-cache-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&directory);
    let instance = wgpu::Instance::default();
    let (adapter, device, _) = pollster::block_on(super::super::open_device(&instance, None));
    if !device.features().contains(wgpu::Features::PIPELINE_CACHE) { return; }
    let first = Cache::open(&adapter, &device, &directory).expect("supported Vulkan cache");
    assert!(!first.loaded);
    let renderer = super::super::new_renderer(&device, Some(first.gpu.clone()));
    first.save().expect("persist actual wgpu cache bytes");
    drop(renderer);
    drop(first);
    drop(device);
    let (adapter, device, _) = pollster::block_on(super::super::open_device(&instance, None));
    let second = Cache::open(&adapter, &device, &directory).expect("reload GPU cache");
    assert!(second.loaded);
    let _renderer = super::super::new_renderer(&device, Some(second.gpu.clone()));
    second.save().expect("resave reused pipeline cache");
    std::fs::remove_dir_all(directory).unwrap();
}
