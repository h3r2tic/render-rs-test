mod file;
mod mesh;
mod render_device;
mod render_loop;
mod render_passes;
mod shader_cache;
mod shader_compiler;

use mesh::*;
use render_core::{
    device::RenderDevice, handles::RenderResourceHandleAllocator, system::RenderSystem, types::*,
};
use render_device::create_render_device;
use std::{
    mem::size_of,
    sync::{Arc, RwLock},
};
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

pub fn into_byte_vec<T>(mut v: Vec<T>) -> Vec<u8>
where
    T: Copy,
{
    unsafe {
        let p = v.as_mut_ptr();
        let item_sizeof = std::mem::size_of::<T>();
        let len = v.len() * item_sizeof;
        let cap = v.capacity() * item_sizeof;
        std::mem::forget(v);
        Vec::from_raw_parts(p as *mut u8, len, cap)
    }
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

    let pipeline_cache = rg::pipeline_cache::PipelineCache::new(
        shader_cache::TurboslothShaderCache::new(LazyCache::create()),
    );

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

    let vertex_count = 3;
    let mesh = TriangleMesh {
        positions: (0..vertex_count)
            .map(|i| {
                let a = i as f32 * std::f32::consts::PI * 2.0 / (vertex_count as f32);
                [a.cos() * 0.5, a.sin() * 0.5, 0.0]
            })
            .collect(),
    };
    let gpu_mesh = Arc::new(GpuTriangleMesh {
        vertex_count,
        vertex_buffer: {
            let mut verts: Vec<RasterGpuVertex> = Vec::with_capacity(mesh.positions.len());
            for (_i, pos) in mesh.positions.iter().enumerate() {
                //let n = mesh.normals[i];
                let n = [0.0f32, 0.0, 1.0];

                verts.push(RasterGpuVertex {
                    pos: *pos,
                    normal: pack_unit_direction_11_10_11(n[0], n[1], n[2]),
                });
            }

            let handle = handles.allocate(RenderResourceType::Buffer);
            device.read()?.create_buffer(
                handle,
                &RenderBufferDesc {
                    bind_flags: RenderBindFlags::SHADER_RESOURCE,
                    size: size_of::<RasterGpuVertex>() * vertex_count as usize,
                },
                Some(&into_byte_vec(verts)),
                "vertex buffer".into(),
            )?;
            handle
        },
    });

    for _ in 0..5 {
        match render_loop.render_frame(swapchain, &pipeline_cache, handles.clone(), || {
            crate::render_passes::render_frame_rg(gpu_mesh.clone())
        }) {
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
    device
        .write()?
        .destroy_resource(Arc::try_unwrap(gpu_mesh).ok().unwrap().vertex_buffer)?;
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
