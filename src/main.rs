mod file;
mod mesh;
mod owned_resource;
mod render_device;
mod render_loop;
mod render_passes;
mod shader_cache;
mod shader_compiler;

use mesh::*;
use owned_resource::{get_resources_pending_release, OwnedRenderResourceHandle};
use render_core::{
    constants::MAX_VERTEX_STREAMS, device::RenderDevice, handles::RenderResourceHandleAllocator,
    state::RenderBindingBuffer, system::RenderSystem, types::*,
};
use render_device::{create_render_device, MaybeRenderDevice};
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

fn try_main(device: &MaybeRenderDevice) -> std::result::Result<(), anyhow::Error> {
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

    let swapchain = OwnedRenderResourceHandle::new(create_swap_chain(
        handles.clone(),
        &*device.read()?,
        window.clone(),
        width,
        height,
    )?);

    let lazy_cache = LazyCache::create();
    let pipeline_cache = rg::pipeline_cache::PipelineCache::new(
        shader_cache::TurboslothShaderCache::new(lazy_cache.clone()),
    );

    let error_output_texture =
        OwnedRenderResourceHandle::new(handles.allocate(RenderResourceType::Texture));
    device.read()?.create_texture(
        *error_output_texture,
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

    let mut render_loop = render_loop::RenderLoop::new(device.clone(), *error_output_texture);
    let mut last_error_text = None;

    let mesh = LoadGltfScene {
        path: "assets/scenes/the_lighthouse/scene.gltf".into(),
        scale: 0.003,
    }
    .into_lazy();
    let mesh = smol::block_on(mesh.eval(&lazy_cache))?;
    let mesh = pack_triangle_mesh(&mesh);

    let index_count = mesh.indices.len() as u32;
    let index_buffer = {
        let handle = handles.allocate(RenderResourceType::Buffer);
        device.read()?.create_buffer(
            handle,
            &RenderBufferDesc {
                bind_flags: RenderBindFlags::INDEX_BUFFER,
                size: size_of::<u32>() * mesh.indices.len(),
            },
            Some(&into_byte_vec(mesh.indices)),
            "index buffer".into(),
        )?;
        handle
    };

    let index_buffer_binding = RenderBindingBuffer {
        resource: index_buffer,
        offset: 0,
        size: 0,
        stride: 4,
    };

    let gpu_mesh = Arc::new(GpuTriangleMesh {
        index_count,
        vertex_count: mesh.verts.len() as u32,
        index_buffer: OwnedRenderResourceHandle::new(index_buffer),
        vertex_buffer: {
            let handle = handles.allocate(RenderResourceType::Buffer);
            device.read()?.create_buffer(
                handle,
                &RenderBufferDesc {
                    bind_flags: RenderBindFlags::SHADER_RESOURCE,
                    size: size_of::<PackedVertex>() * mesh.verts.len(),
                },
                Some(&into_byte_vec(mesh.verts)),
                "vertex buffer".into(),
            )?;
            OwnedRenderResourceHandle::new(handle)
        },
        draw_binding: {
            let handle = handles.allocate(RenderResourceType::DrawBindingSet);
            device.read()?.create_draw_binding_set(
                handle,
                &RenderDrawBindingSetDesc {
                    vertex_buffers: [None; MAX_VERTEX_STREAMS],
                    index_buffer: Some(index_buffer_binding),
                },
                "draw binding".into(),
            )?;
            OwnedRenderResourceHandle::new(handle)
        },
    });

    for _ in 0..5 {
        match render_loop.render_frame(*swapchain, &pipeline_cache, handles.clone(), || {
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

    Ok(())
}

fn main() {
    let render_system = Arc::new(RwLock::new(RenderSystem::new()));
    let device = create_render_device(render_system).unwrap();

    if let Err(err) = try_main(&device) {
        eprintln!("ERROR: {:?}", err);
        std::process::exit(1);
    }

    if let Ok(device) = device.write() {
        device.device_wait_idle().unwrap();
        for res in get_resources_pending_release() {
            device.destroy_resource(res).unwrap();
        }
    }

    drop(device);
}
