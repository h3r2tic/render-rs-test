mod bytes;
mod camera;
mod file;
mod math;
mod mesh;
mod owned_resource;
mod render_device;
mod render_loop;
mod render_passes;
mod rt_util;
mod shader_cache;
mod shader_compiler;

use camera::*;
use math::*;
use mesh::*;
use owned_resource::{get_resources_pending_release, OwnedRenderResourceHandle};
use render_core::{
    device::RenderDevice, handles::RenderResourceHandleAllocator, system::RenderSystem, types::*,
};
use render_device::{create_render_device, MaybeRenderDevice};
use std::sync::{Arc, RwLock};
use turbosloth::*;

#[derive(Copy, Clone)]
pub struct RaytraceData {
    pub pipeline_state: RenderResourceHandle,
    pub shader_table: RenderResourceHandle,
    pub top_acceleration: RenderResourceHandle,
}

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

    let mesh = smol::run(
        LoadGltfScene {
            path: "assets/scenes/the_lighthouse/scene.gltf".into(),
            scale: 0.01,
        }
        .into_lazy()
        .eval(&lazy_cache),
    )?;

    let gpu_mesh = Arc::new(upload_mesh_to_gpu(
        &*device.read()?,
        &handles,
        pack_triangle_mesh(&mesh),
    )?);

    let bottom_as = handles.allocate(RenderResourceType::RayTracingBottomAcceleration);
    device.read()?.create_ray_tracing_bottom_acceleration(
        bottom_as,
        &RayTracingBottomAccelerationDesc {
            geometries: vec![gpu_mesh.to_ray_tracing_geometry()],
        },
        "BLAS".into(),
    )?;

    let top_as = handles.allocate(RenderResourceType::RayTracingTopAcceleration);
    device.read()?.create_ray_tracing_top_acceleration(
        top_as,
        &RayTracingTopAccelerationDesc {
            instances: vec![bottom_as],
        },
        "TLAS".into(),
    )?;

    let rt_pipeline_state = rt_util::create_ray_tracing_pipeline_state(
        "/assets/shaders/rt/triangle.rgen.hlsl".into(),
        "/assets/shaders/rt/triangle.rmiss.hlsl".into(),
        "/assets/shaders/rt/triangle.rchit.hlsl".into(),
        &device,
        handles.clone(),
        lazy_cache.clone(),
    )?;

    let sbt = handles.allocate(RenderResourceType::RayTracingShaderTable);
    device.read()?.create_ray_tracing_shader_table(
        sbt,
        &RayTracingShaderTableDesc {
            pipeline_state: rt_pipeline_state,
            raygen_entry_count: 1,
            hit_entry_count: 1,
            miss_entry_count: 1,
        },
        "sbt".into(),
    )?;

    let rt_data = RaytraceData {
        pipeline_state: rt_pipeline_state,
        shader_table: sbt,
        top_acceleration: top_as,
    };

    #[allow(unused_mut)]
    let mut camera = FirstPersonCamera::new(Vec3::new(0.0, 2.0, 10.0));

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

    let mut render_loop =
        render_loop::RenderLoop::new(device.clone(), handles.clone(), *error_output_texture);
    let mut last_error_text = None;

    for _ in 0..1000 {
        let camera_matrices = camera.calc_matrices();

        match render_loop.render_frame(*swapchain, &pipeline_cache, || {
            crate::render_passes::render_frame_rg(camera_matrices, gpu_mesh.clone(), rt_data)
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
