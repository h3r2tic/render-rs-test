mod file;
mod render_passes;
mod renderer;
mod shader_cache;
mod shader_compiler;

use render_core::{device::RenderDevice, encoder::RenderCommandList, handles::*, types::*};
use std::{collections::VecDeque, sync::Arc};
use turbosloth::*;

struct RenderLoop {
    persistent_resources: Vec<RenderResourceHandle>,
    retired_frames: VecDeque<Option<renderer::FrameResources>>,
    error_output_texture: RenderResourceHandle,
}

impl RenderLoop {
    fn new(error_output_texture: RenderResourceHandle) -> Self {
        let mut retired_frames: VecDeque<Option<renderer::FrameResources>> = Default::default();
        retired_frames.push_back(None);
        retired_frames.push_back(None);

        Self {
            persistent_resources: Default::default(),
            retired_frames,
            error_output_texture,
        }
    }

    fn render_frame(
        &mut self,
        device: &mut dyn RenderDevice,
        swapchain: RenderResourceHandle,
        renderer: &renderer::Renderer,
        shader_cache: &dyn rg::shader_cache::ShaderCache,
    ) -> anyhow::Result<()> {
        if let Some(frame_resources) = self.retired_frames.pop_front().unwrap() {
            if let Some(fence) = frame_resources.resources_used_fence {
                device.wait_for_fence(fence)?;
            }

            frame_resources.destroy_now(&*device);
        }

        let mut frame_resources = renderer::FrameResources::default();

        let main_command_list_handle =
            renderer.allocate_frame_handle(RenderResourceType::CommandList, &mut frame_resources);
        device.create_command_list(main_command_list_handle, "Main command list".into())?;

        let mut cb =
            RenderCommandList::new(renderer.handles.clone(), 1024 * 1024 * 16, 1024 * 1024)?;

        let handle_allocator = rg::TrackingResourceHandleAllocator::new(renderer.handles.clone());

        let output_texture = {
            let (rg, tex) = crate::render_passes::render_frame_rg();

            // println!("Recorded {} passes", rg.passes.len());
            let execution_output = rg.execute(
                rg::RenderGraphExecutionParams {
                    handles: &handle_allocator,
                    device: &*device,
                    shader_cache: shader_cache,
                },
                &mut cb,
                tex,
            );

            let mut allocated_resources = handle_allocator.into_allocated_resources();

            frame_resources
                .handles
                .append(&mut allocated_resources.transient);

            self.persistent_resources
                .append(&mut allocated_resources.persistent);

            execution_output.map(|execution_output| execution_output.output_texture)
        };

        let resources_used_fence =
            renderer.allocate_frame_handle(RenderResourceType::Fence, &mut frame_resources);
        device.create_fence(
            resources_used_fence,
            &RenderFenceDesc {
                cross_device: false,
            },
            "resource usage fence".into(),
        )?;
        frame_resources.resources_used_fence = Some(resources_used_fence);

        device.compile_command_list(main_command_list_handle, &cb)?;
        device.submit_command_list(main_command_list_handle, true, None, None, None)?;

        let result = match output_texture {
            Ok(output_texture) => {
                device.present_swap_chain(
                    swapchain,
                    output_texture,
                    frame_resources.resources_used_fence,
                )?;
                Ok(())
            }
            Err(e) => {
                device.present_swap_chain(
                    swapchain,
                    self.error_output_texture,
                    frame_resources.resources_used_fence,
                )?;
                Err(e)
            }
        };

        device.advance_frame()?;
        self.retired_frames.push_back(Some(frame_resources));

        result
    }

    fn destroy_resources(
        &mut self,
        device: &dyn RenderDevice,
    ) -> std::result::Result<(), anyhow::Error> {
        for frame_resources in self.retired_frames.drain(..) {
            if let Some(frame_resources) = frame_resources {
                frame_resources.destroy_now(&*device);
            }
        }

        for resource in self.persistent_resources.drain(..) {
            device.destroy_resource(resource)?;
        }

        Ok(())
    }
}

fn try_main() -> std::result::Result<(), anyhow::Error> {
    let mut renderer = renderer::Renderer::new();

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

    let shader_cache = shader_cache::TurboslothShaderCache::new(LazyCache::create());

    let error_output_texture = renderer.allocate_handle(RenderResourceType::Texture);
    device.create_texture(
        error_output_texture,
        &RenderTextureDesc {
            texture_type: RenderTextureType::Tex2d,
            bind_flags: RenderBindFlags::NONE,
            format: RenderFormat::R10g10b10a2Unorm,
            width,
            height,
            depth: 1,
            levels: 1,
            elements: 1,
        },
        None,
        "error output texture".into(),
    )?;

    let mut render_loop = RenderLoop::new(error_output_texture);
    let mut last_error_text = None;

    for _ in 0..50 {
        match render_loop.render_frame(&mut **device, swapchain, &renderer, &shader_cache) {
            Ok(()) => {
                last_error_text = None;
            }
            Err(e) => {
                let error_text = Some(format!("{:?}", e));
                if error_text != last_error_text {
                    println!("{}", error_text.as_ref().unwrap());
                    last_error_text = error_text;
                }
            }
        }

        // Slow down rendering so the window stays up for a while
        // Comment-out to test synchronization issues.
        std::thread::sleep(std::time::Duration::from_millis(200));
    }

    device.device_wait_idle()?;
    render_loop.destroy_resources(&**device)?;
    device.destroy_resource(error_output_texture)?;
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
