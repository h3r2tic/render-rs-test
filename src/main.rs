mod file;
mod render_loop;
mod render_passes;
mod renderer;
mod shader_cache;
mod shader_compiler;

use render_core::types::*;
use std::sync::Arc;
use turbosloth::*;

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

    let mut render_loop = render_loop::RenderLoop::new(error_output_texture);
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
        std::process::exit(1);
    }
}
