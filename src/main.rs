use render_core::backend::*;
//use render_core::commands::*;
use render_core::device::*;
//use render_core::encoder::*;
use render_core::handles::*;
use render_core::{
    encoder::RenderCommandList,
    state::{build, RenderComputePipelineStateDesc},
    system::*,
    types::*,
};
//use render_core::types::*;
//use render_core::utilities::*;
use std::collections::VecDeque;
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
    submit_done_fence: RenderResourceHandle,
}

impl FrameResources {
    pub fn destroy_now(self, device: &mut dyn RenderDevice) {
        for handle in self.handles {
            device.destroy_resource(handle).unwrap();
        }
    }
}

fn try_main() -> std::result::Result<(), failure::Error> {
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

    let gradients_texture = renderer.allocate_handle(RenderResourceType::Texture);
    let output_texture = renderer.allocate_handle(RenderResourceType::Texture);

    device.create_texture(
        gradients_texture,
        &RenderTextureDesc {
            texture_type: RenderTextureType::Tex2d,
            bind_flags: RenderBindFlags::UNORDERED_ACCESS | RenderBindFlags::SHADER_RESOURCE,
            format: RenderFormat::R32g32b32a32Float,
            width,
            height,
            depth: 1,
            levels: 1,
            elements: 1,
        },
        None,
        "Gradients texture".into(),
    )?;

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
        None,
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

    let gradients_shader = renderer.allocate_handle(RenderResourceType::Shader);
    device.create_shader(
        gradients_shader,
        &RenderShaderDesc {
            shader_type: RenderShaderType::Compute,
            shader_data: include_bytes!("../gradients.spv").as_ref().to_owned(),
        },
        "gradients compute shader".into(),
    )?;

    let gradients_pipeline = renderer.allocate_handle(RenderResourceType::ComputePipelineState);
    device.create_compute_pipeline_state(
        gradients_pipeline,
        &RenderComputePipelineStateDesc {
            shader: gradients_shader,
            shader_signature: RenderShaderSignatureDesc::new(
                &[RenderShaderParameter::new(0, 1)],
                &[],
            ),
        },
        "gradients compute pipeline".into(),
    )?;

    let blur_shader = renderer.allocate_handle(RenderResourceType::Shader);
    device.create_shader(
        blur_shader,
        &RenderShaderDesc {
            shader_type: RenderShaderType::Compute,
            shader_data: include_bytes!("../blur.spv").as_ref().to_owned(),
        },
        "blur compute shader".into(),
    )?;

    let blur_pipeline = renderer.allocate_handle(RenderResourceType::ComputePipelineState);
    device.create_compute_pipeline_state(
        blur_pipeline,
        &RenderComputePipelineStateDesc {
            shader: blur_shader,
            shader_signature: RenderShaderSignatureDesc::new(
                &[RenderShaderParameter::new(1, 1)],
                &[],
            ),
        },
        "blur compute pipeline".into(),
    )?;

    let mut retired_frames: VecDeque<Option<FrameResources>> = Default::default();
    retired_frames.push_back(None);
    retired_frames.push_back(None);

    for _ in 0..5 {
        if let Some(frame_resources) = retired_frames.pop_front().unwrap() {
            device.wait_for_fence(frame_resources.submit_done_fence)?;
            frame_resources.destroy_now(&mut **device);
        }

        let mut frame_resources = FrameResources::default();

        let gradients_shader_views =
            renderer.allocate_frame_handle(RenderResourceType::ShaderViews, &mut frame_resources);
        device.create_shader_views(
            gradients_shader_views,
            &RenderShaderViewsDesc {
                shader_resource_views: Vec::new(),
                unordered_access_views: vec![build::texture_2d_rw(
                    gradients_texture,
                    RenderFormat::R32g32b32a32Float,
                    0,
                    0,
                )],
            },
            "gradients compute shader resource views".into(),
        )?;

        let blur_shader_views =
            renderer.allocate_frame_handle(RenderResourceType::ShaderViews, &mut frame_resources);
        device.create_shader_views(
            blur_shader_views,
            &RenderShaderViewsDesc {
                shader_resource_views: vec![build::texture_2d(
                    gradients_texture,
                    RenderFormat::R32g32b32a32Float,
                    0,
                    1,
                    0,
                    0.0f32,
                )],
                unordered_access_views: vec![build::texture_2d_rw(
                    output_texture,
                    RenderFormat::R32g32b32a32Float,
                    0,
                    0,
                )],
            },
            "blur compute shader resource views".into(),
        )?;
        let main_command_list_handle =
            renderer.allocate_frame_handle(RenderResourceType::CommandList, &mut frame_resources);
        device.create_command_list(main_command_list_handle, "Main command list".into())?;

        let mut cb =
            RenderCommandList::new(renderer.handles.clone(), 1024 * 1024 * 16, 1024 * 1024)?;

        cb.dispatch_2d(
            gradients_pipeline,
            &[RenderShaderArgument {
                constant_buffer: None,
                shader_views: Some(gradients_shader_views),
                constant_buffer_offset: 0,
            }],
            width,
            height,
            Some(8),
            Some(8),
        )?;

        cb.transitions(&[(
            gradients_texture,
            RenderResourceStates::NON_PIXEL_SHADER_RESOURCE,
        )])?;

        cb.dispatch_2d(
            blur_pipeline,
            &[RenderShaderArgument {
                constant_buffer: None,
                shader_views: Some(blur_shader_views),
                constant_buffer_offset: 0,
            }],
            width,
            height,
            Some(8),
            Some(8),
        )?;

        let submit_done_fence =
            renderer.allocate_frame_handle(RenderResourceType::Fence, &mut frame_resources);
        device.create_fence(
            submit_done_fence,
            &RenderFenceDesc {
                cross_device: false,
            },
            "submit done fence".into(),
        )?;

        device.compile_command_list(main_command_list_handle, &cb)?;
        device.submit_command_list(
            main_command_list_handle,
            true,
            None,
            None,
            Some(submit_done_fence),
        )?;
        frame_resources.submit_done_fence = submit_done_fence;

        device.present_swap_chain(swapchain, output_texture)?;
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

    device.destroy_resource(gradients_shader)?;
    device.destroy_resource(gradients_pipeline)?;
    device.destroy_resource(blur_shader)?;
    device.destroy_resource(blur_pipeline)?;
    device.destroy_resource(output_texture)?;
    device.destroy_resource(gradients_texture)?;
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
