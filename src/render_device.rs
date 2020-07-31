use render_core::{backend::*, device::*, handles::*, system::*};
use std::{
    env,
    path::PathBuf,
    sync::{Arc, RwLock, RwLockReadGuard, RwLockWriteGuard},
};

pub fn get_render_debug_flags() -> RenderDebugFlags {
    RenderDebugFlags::CPU_VALIDATION
}

pub fn get_render_backend_settings() -> Vec<RenderBackendSettings> {
    //let backends = ["mock", "vk", "dx12", "mtl", "proxy"];
    //let backends = ["mock", "vk"];
    let backends = ["vk"];
    let mut backend_settings: Vec<RenderBackendSettings> = Vec::new();
    for backend in backends.iter() {
        backend_settings.push(RenderBackendSettings {
            api: backend.to_string(),
            address: None, // TODO: Specify for proxy
            debug_flags: get_render_debug_flags(),
        });
    }
    backend_settings
}

pub fn get_render_module_path() -> PathBuf {
    let exe_path = env::current_exe().unwrap();
    let module_path = exe_path.parent().unwrap();
    let mut path = module_path.to_path_buf();
    path.push("deps");
    path
}

#[derive(Clone)]
pub struct MaybeRenderDevice {
    device: Arc<RwLock<Option<Box<dyn RenderDevice>>>>,

    // Keeps a reference to the RenderSystem, preventing it from dropping before the device
    #[allow(dead_code)]
    render_system: Arc<RwLock<RenderSystem>>,
}

impl Drop for MaybeRenderDevice {
    fn drop(&mut self) {
        // Need to release this reference before the render system (TODO: solve lifetimes)
        self.device = Arc::new(RwLock::new(None));
    }
}

pub struct RenderDeviceReadLock<'a>(RwLockReadGuard<'a, Option<Box<dyn RenderDevice>>>);
pub struct RenderDeviceWriteLock<'a>(RwLockWriteGuard<'a, Option<Box<dyn RenderDevice>>>);

impl<'a> std::ops::Deref for RenderDeviceReadLock<'a> {
    type Target = dyn RenderDevice;

    fn deref(&self) -> &Self::Target {
        &**self.0.as_ref().unwrap()
    }
}

impl<'a> std::ops::Deref for RenderDeviceWriteLock<'a> {
    type Target = dyn RenderDevice;

    fn deref(&self) -> &Self::Target {
        &**self.0.as_ref().unwrap()
    }
}
impl<'a> std::ops::DerefMut for RenderDeviceWriteLock<'a> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut **self.0.as_mut().unwrap()
    }
}

impl MaybeRenderDevice {
    pub fn read<'s>(
        &'s self,
    ) -> anyhow::Result<impl std::ops::Deref<Target = dyn RenderDevice> + 's> {
        let guard = self.device.read().unwrap();
        if guard.is_some() {
            Ok(RenderDeviceReadLock(guard))
        } else {
            Err(anyhow::anyhow!("Device lost"))
        }
    }

    pub fn write<'s>(
        &'s self,
    ) -> anyhow::Result<impl std::ops::DerefMut<Target = dyn RenderDevice> + 's> {
        let guard = self.device.write().unwrap();
        if guard.is_some() {
            Ok(RenderDeviceWriteLock(guard))
        } else {
            Err(anyhow::anyhow!("Device lost"))
        }
    }
}

pub fn create_render_device(
    render_system: Arc<RwLock<RenderSystem>>,
) -> anyhow::Result<MaybeRenderDevice> {
    let mut render_system_lock = render_system.write().unwrap();

    let module_path = get_render_module_path();
    let backend_settings = get_render_backend_settings();

    render_system_lock
        .initialize(&module_path, &backend_settings)
        .unwrap();

    assert!(render_system_lock.is_initialized());
    let registry = Arc::clone(&render_system_lock.get_registry().unwrap());
    let registry_read = registry.read().unwrap();

    if registry_read.len() == 0 {
        Err(anyhow::anyhow!("no registry entries"))
    } else {
        let backend_registry = &registry_read[0];
        let _device_info = Arc::new(
            render_system_lock
                .enumerate_devices(&backend_registry, false, None, None)
                .unwrap(),
        );
        render_system_lock
            .create_device(&backend_registry, 0)
            .unwrap();

        let device = render_system_lock.get_device(&backend_registry, 0).unwrap();
        drop(render_system_lock);

        Ok(MaybeRenderDevice {
            device,
            render_system,
        })
    }
}

#[derive(Default)]
pub struct FrameResources {
    pub handles: Vec<RenderResourceHandle>,
    pub resources_used_fence: Option<RenderResourceHandle>,
}

impl FrameResources {
    pub fn destroy_now(self, device: &dyn RenderDevice) {
        for handle in self.handles {
            device.destroy_resource(handle).unwrap();
        }
    }
}
