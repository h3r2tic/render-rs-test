mod file;
mod render_device;
mod render_loop;
mod render_passes;
mod shader_cache;
mod shader_compiler;

use render_core::{
    device::RenderDevice, handles::RenderResourceHandleAllocator, system::RenderSystem, types::*,
};
use render_device::create_render_device;
use std::sync::{Arc, RwLock};
use turbosloth::*;

pub trait HandleAllocator {
    fn allocate(&self, kind: RenderResourceType) -> RenderResourceHandle;
}

impl HandleAllocator for Arc<RwLock<RenderResourceHandleAllocator>> {
    fn allocate(&self, kind: RenderResourceType) -> RenderResourceHandle {
        self.write().unwrap().allocate(kind)
    }
}

fn create_swap_chain(
    handles: impl HandleAllocator,
    device: &dyn RenderDevice,
    window: Arc<winit::Window>,
    width: u32,
    height: u32,
) -> anyhow::Result<RenderResourceHandle> {
    let swapchain = handles.allocate(RenderResourceType::SwapChain);
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

    Ok(swapchain)
}

fn try_main() -> std::result::Result<(), anyhow::Error> {
    let render_system = Arc::new(RwLock::new(RenderSystem::new()));
    let device = create_render_device(render_system)?;
    let handles = Arc::new(RwLock::new(RenderResourceHandleAllocator::new()));

    let width = 1280u32;
    let height = 720u32;

    let events_loop = winit::EventsLoop::new();
    let window = Arc::new(
        winit::WindowBuilder::new()
            .with_title("render-rs test")
            .with_dimensions(winit::dpi::LogicalSize::new(width as f64, height as f64))
            .build(&events_loop)
            .expect("window"),
    );

    let swapchain = create_swap_chain(
        handles.clone(),
        &*device.read()?,
        window.clone(),
        width,
        height,
    )?;

    let shader_cache = shader_cache::TurboslothShaderCache::new(LazyCache::create());

    let error_output_texture = handles.allocate(RenderResourceType::Texture);
    device.read()?.create_texture(
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

    let mut render_loop = render_loop::RenderLoop::new(device.clone(), error_output_texture);
    let mut last_error_text = None;

    for _ in 0..5 {
        match render_loop.render_frame(
            swapchain,
            &shader_cache,
            handles.clone(),
            crate::render_passes::render_frame_rg,
        ) {
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

    device.write()?.device_wait_idle()?;
    device.write()?.destroy_resource(error_output_texture)?;
    device.write()?.destroy_resource(swapchain)?;

    Ok(())
}

fn main() {
    if let Err(err) = try_main() {
        eprintln!("ERROR: {:?}", err);
        std::process::exit(1);
    }
}
