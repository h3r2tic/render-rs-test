mod file;
mod render_passes;
mod shader_cache;
mod shader_compiler;

use render_core::{
    backend::*, device::*, encoder::RenderCommandList, handles::*, system::*, types::*,
};
use std::{
    collections::VecDeque,
    env,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use turbosloth::*;

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
    handles: Vec<RenderResourceHandle>,
    present_done_fence: RenderResourceHandle,
}

impl FrameResources {
    pub fn destroy_now(self, device: &mut dyn RenderDevice) {
        for handle in self.handles {
            device.destroy_resource(handle).unwrap();
        }
    }
}

fn try_main() -> std::result::Result<(), anyhow::Error> {
    let mut renderer = Renderer::new();

    let render_system = &mut renderer.render_system;
    let registry = Arc::clone(&render_system.get_registry().unwrap());
    let registry_read = registry.read().unwrap();

    assert!(registry_read.len() > 0);

    for entry in registry_read.iter() {
        let device_info = Arc::new(
            render_system
                .enumerate_devices(&entry, false, None, None)
                .unwrap(),
        );
        let info_list = Arc::clone(&device_info);
        assert!(info_list.len() > 0);

        // println!("{:#?}", info_list);
    }

    let mut device = renderer.device.write().unwrap();
    let device = device.as_mut().expect("device");

    let width = 1280u32;
    let height = 720u32;

    let events_loop = winit::EventsLoop::new();
    let window = winit::WindowBuilder::new()
        .with_title("render-rs test")
        .with_dimensions(winit::dpi::LogicalSize::new(width as f64, height as f64))
        .build(&events_loop)
        .expect("window");
    let window = Arc::new(window);

    let swapchain = {
        let swapchain = renderer.allocate_handle(RenderResourceType::SwapChain);

        use raw_window_handle::{HasRawWindowHandle as _, RawWindowHandle};

        device.create_swap_chain(
            swapchain,
            &RenderSwapChainDesc {
                width,
                height,
                format: RenderFormat::R10g10b10a2Unorm,
                buffer_count: 3,
                window: match window.raw_window_handle() {
                    RawWindowHandle::Windows(handle) => RenderSwapChainWindow {
                        hinstance: handle.hinstance,
                        hwnd: handle.hwnd,
                    },
                    _ => todo!(),
                },
            },
            "Main swap chain".into(),
        )?;

        swapchain
    };

    let mut persistent_resources: Vec<RenderResourceHandle> = Default::default();
    let mut retired_frames: VecDeque<Option<FrameResources>> = Default::default();
    retired_frames.push_back(None);
    retired_frames.push_back(None);

    let shader_cache = shader_cache::TurboslothShaderCache::new(LazyCache::create());

    for _ in 0..5 {
        if let Some(frame_resources) = retired_frames.pop_front().unwrap() {
            device.wait_for_fence(frame_resources.present_done_fence)?;
            frame_resources.destroy_now(&mut **device);
        }
        let mut frame_resources = FrameResources::default();

        let main_command_list_handle =
            renderer.allocate_frame_handle(RenderResourceType::CommandList, &mut frame_resources);
        device.create_command_list(main_command_list_handle, "Main command list".into())?;

        let mut cb =
            RenderCommandList::new(renderer.handles.clone(), 1024 * 1024 * 16, 1024 * 1024)?;

        let output_texture = {
            let (rg, tex) = crate::render_passes::render_frame_rg();

            // println!("Recorded {} passes", rg.passes.len());
            let mut rg_execution_output = rg.execute(
                rg::RenderGraphExecutionParams {
                    handles: rg::TrackingResourceHandleAllocator::new(renderer.handles.clone()),
                    device: &**device,
                    shader_cache: &shader_cache,
                },
                &mut cb,
                tex,
            );

            frame_resources
                .handles
                .append(&mut rg_execution_output.allocated_resources.transient);

            persistent_resources.append(&mut rg_execution_output.allocated_resources.persistent);
            rg_execution_output.output_texture
        };

        device.compile_command_list(main_command_list_handle, &cb)?;

        let submit_done_fence =
            renderer.allocate_frame_handle(RenderResourceType::Fence, &mut frame_resources);
        device.create_fence(
            submit_done_fence,
            &RenderFenceDesc {
                cross_device: false,
            },
            "submit done fence".into(),
        )?;

        device.submit_command_list(main_command_list_handle, true, None, None, None)?;
        frame_resources.present_done_fence = submit_done_fence;

        device.present_swap_chain(swapchain, output_texture, Some(submit_done_fence))?;
        device.advance_frame()?;

        retired_frames.push_back(Some(frame_resources));

        // Slow down rendering so the window stays up for a while
        // Comment-out to test synchronization issues.
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    device.device_wait_idle()?;

    for frame_resources in retired_frames.drain(..) {
        if let Some(frame_resources) = frame_resources {
            frame_resources.destroy_now(&mut **device);
        }
    }

    for resource in persistent_resources.drain(..) {
        device.destroy_resource(resource)?;
    }

    device.destroy_resource(swapchain)?;

    Ok(())
}

fn main() {
    if let Err(err) = try_main() {
        eprintln!("ERROR: {:?}", err);
        /*err.chain()
        .skip(1)
        .for_each(|cause| eprintln!("because: {}", cause));*/
        std::process::exit(1);
    }
}
