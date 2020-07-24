use render_core::backend::*;
//use render_core::commands::*;
use render_core::device::*;
//use render_core::encoder::*;
use render_core::handles::*;
use render_core::{
    encoder::RenderCommandList,
    state::{
        build, RenderBindingUnorderedAccessView, RenderBindingView, RenderComputePipelineStateDesc,
    },
    system::*,
    types::{
        RenderBindFlags, RenderFormat, RenderResourceType, RenderShaderArgument, RenderShaderDesc,
        RenderShaderParameter, RenderShaderSignatureDesc, RenderShaderType, RenderShaderViewsDesc,
        RenderSwapChainDesc, RenderSwapChainWindow, RenderTextureDesc,
        RenderTextureSubResourceData, RenderTextureType, RenderViewDimension,
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
    pub fn allocate_handle(&self, kind: RenderResourceType) -> RenderResourceHandle {
        self.handles.write().unwrap().allocate(kind)
    }

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
    let mut renderer = SystemHarness::new();

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

        println!("{:#?}", info_list);
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

    let output_texture = renderer.allocate_handle(RenderResourceType::Texture);

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
            bind_flags: RenderBindFlags::UNORDERED_ACCESS,
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
        let swapchain = renderer.allocate_handle(RenderResourceType::SwapChain);

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

    let compute_shader = renderer.allocate_handle(RenderResourceType::Shader);
    device.create_shader(
        compute_shader,
        &RenderShaderDesc {
            shader_type: RenderShaderType::Compute,
            shader_data: include_bytes!("../gradients.spv").as_ref().to_owned(),
        },
        "compute shader".into(),
    )?;

    let compute_pipeline = renderer.allocate_handle(RenderResourceType::ComputePipelineState);
    device.create_compute_pipeline_state(
        compute_pipeline,
        &RenderComputePipelineStateDesc {
            shader: compute_shader,
            shader_signature: RenderShaderSignatureDesc::new(
                &[RenderShaderParameter::new(0, 1)],
                &[],
            ),
        },
        "compute pipeline".into(),
    )?;

    let shader_views = renderer.allocate_handle(RenderResourceType::ShaderViews);
    device.create_shader_views(
        shader_views,
        &RenderShaderViewsDesc {
            shader_resource_views: Vec::new(),
            unordered_access_views: vec![build::texture_2d_rw(
                output_texture,
                RenderFormat::R32g32b32a32Float,
                0,
                0,
            )],
        },
        "compute shader resource views".into(),
    )?;

    let main_command_list_handle = renderer.allocate_handle(RenderResourceType::CommandList);
    device.create_command_list(main_command_list_handle, "Main command list".into())?;

    let mut cb = RenderCommandList::new(renderer.handles.clone(), 1024 * 1024 * 16, 1024 * 1024)?;

    cb.dispatch_2d(
        compute_pipeline,
        &[RenderShaderArgument {
            constant_buffer: None,
            shader_views: Some(shader_views),
            constant_buffer_offset: 0,
        }],
        width,
        height,
        Some(8),
        Some(8),
    )?;

    device.compile_command_list(main_command_list_handle, &cb)?;
    device.submit_command_list(main_command_list_handle, true, None, None)?;

    device.present_swap_chain(swapchain, output_texture)?;
    device.advance_frame()?;

    device.destroy_resource(output_texture)?;
    device.destroy_resource(main_command_list_handle)?;
    device.destroy_resource(swapchain)?;

    loop {}

    //Ok(())
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
