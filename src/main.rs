use render_core::backend::*;
//use render_core::commands::*;
use render_core::device::*;
//use render_core::encoder::*;
use render_core::handles::*;
use render_core::{
    system::*,
    types::{
        RenderBindFlags, RenderFormat, RenderResourceType, RenderSwapChainDesc,
        RenderSwapChainWindow, RenderTextureDesc, RenderTextureSubResourceData, RenderTextureType,
    },
};
//use render_core::types::*;
//use render_core::utilities::*;
use std::env;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

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

pub struct SystemHarness {
    pub render_system: RenderSystem,
    pub device_info: Arc<Vec<RenderDeviceInfo>>,
    pub handles: Arc<RwLock<RenderResourceHandleAllocator>>,
    pub device: Arc<RwLock<Option<Box<dyn RenderDevice>>>>,
}

impl SystemHarness {
    pub fn new() -> SystemHarness {
        let render_system = RenderSystem::new();
        let mut harness = SystemHarness {
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

impl Drop for SystemHarness {
    fn drop(&mut self) {
        self.release();
    }
}

fn try_main() -> std::result::Result<(), failure::Error> {
    let mut harness = SystemHarness::new();

    let render_system = &mut harness.render_system;
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

        println!("{:#?}", info_list);
    }

    let mut device = harness.device.write().unwrap();
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

    let output_texture = harness
        .handles
        .write()
        .unwrap()
        .allocate(RenderResourceType::Texture);

    let initial_pixel_data_byte_count = width * height * 4 * 4;
    let initial_pixel_data: Vec<[f32; 4]> = (0..width * height)
        .map(|_| [0.8f32, 0.5f32, 0.1f32, 1.0f32])
        .collect();
    let initial_pixel_data_bytes = unsafe {
        std::slice::from_raw_parts(
            initial_pixel_data.as_ptr() as _,
            initial_pixel_data_byte_count as usize,
        )
    }
    .to_owned();

    let initial_texture_data = RenderTextureSubResourceData {
        data: &initial_pixel_data_bytes,
        row_pitch: width * 4 * 4,
        slice_pitch: 0,
    };

    device.create_texture(
        output_texture,
        &RenderTextureDesc {
            texture_type: RenderTextureType::Tex2d,
            bind_flags: RenderBindFlags::NONE,
            format: RenderFormat::R32g32b32a32Float,
            width,
            height,
            depth: 1,
            levels: 1,
            elements: 1,
        },
        Some(initial_texture_data),
        "Output texture".into(),
    )?;

    let swapchain = {
        let swapchain = harness
            .handles
            .write()
            .unwrap()
            .allocate(RenderResourceType::SwapChain);

        use raw_window_handle::{HasRawWindowHandle as _, RawWindowHandle};

        device.create_swap_chain(
            swapchain,
            &RenderSwapChainDesc {
                width,
                height,
                format: RenderFormat::R32g32b32a32Float,
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

    device.present_swap_chain(swapchain, output_texture)?;

    loop {}

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
