use render_core::{backend::*, device::*, handles::*, system::*, types::*};
use std::{
    env,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
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

pub struct Renderer {
    pub render_system: RenderSystem,
    pub device_info: Arc<Vec<RenderDeviceInfo>>,
    pub handles: Arc<RwLock<RenderResourceHandleAllocator>>,
    pub device: Arc<RwLock<Option<Box<dyn RenderDevice>>>>,
}

impl Renderer {
    pub fn allocate_handle(&self, kind: RenderResourceType) -> RenderResourceHandle {
        self.handles.write().unwrap().allocate(kind)
    }

    pub fn allocate_frame_handle(
        &self,
        kind: RenderResourceType,
        frame_resources: &mut FrameResources,
    ) -> RenderResourceHandle {
        let handle = self.handles.write().unwrap().allocate(kind);
        frame_resources.handles.push(handle);
        handle
    }

    pub fn new() -> Renderer {
        let render_system = RenderSystem::new();
        let mut harness = Renderer {
            render_system,
            device_info: Arc::new(Vec::new()),
            handles: Arc::new(RwLock::new(RenderResourceHandleAllocator::new())),
            device: Arc::new(RwLock::new(None)),
        };

        harness.initialize(&get_render_module_path(), &get_render_backend_settings());
        harness
    }

    pub fn initialize(&mut self, module_path: &Path, backend_settings: &[RenderBackendSettings]) {
        let render_system = &mut self.render_system;
        render_system
            .initialize(&module_path, &backend_settings)
            .unwrap();
        assert!(render_system.is_initialized());
        let registry = Arc::clone(&render_system.get_registry().unwrap());
        let registry_read = registry.read().unwrap();
        if registry_read.len() == 0 {
            panic!("no registry entries");
        } else {
            let backend_registry = &registry_read[0];
            self.device_info = Arc::new(
                render_system
                    .enumerate_devices(&backend_registry, false, None, None)
                    .unwrap(),
            );
            render_system.create_device(&backend_registry, 0).unwrap();
            self.device = render_system.get_device(&backend_registry, 0).unwrap();
        }
    }

    pub fn release(&mut self) {
        // Need to release this reference before the render system (TODO: solve lifetimes)
        self.device = Arc::new(RwLock::new(None));
        self.render_system
            .release()
            .expect("failed to release render system");
    }
}

impl Drop for Renderer {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Default)]
pub struct FrameResources {
    pub handles: Vec<RenderResourceHandle>,
    pub present_done_fence: RenderResourceHandle,
}

impl FrameResources {
    pub fn destroy_now(self, device: &dyn RenderDevice) {
        for handle in self.handles {
            device.destroy_resource(handle).unwrap();
        }
    }
}
