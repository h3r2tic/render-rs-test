use crate::owned_resource::OwnedRenderResourceHandle;
use crate::render_device::MaybeRenderDevice;
use crate::shader_compiler::CompileRayTracingShader;
use crate::HandleAllocator;
use render_core::{constants::MAX_RAY_TRACING_SHADER_TYPE, types::*};
use std::{path::PathBuf, sync::Arc};
use turbosloth::*;

// TODO: more shaders
pub fn create_ray_tracing_pipeline_state(
    raygen_shader_path: PathBuf,
    miss_shader_path: PathBuf,
    hit_shader_path: PathBuf,
    device: &MaybeRenderDevice,
    handles: impl HandleAllocator,
    lazy_cache: Arc<LazyCache>,
) -> anyhow::Result<RenderResourceHandle> {
    let raygen_shader =
        OwnedRenderResourceHandle::new(handles.allocate(RenderResourceType::RayTracingProgram));
    device.read()?.create_ray_tracing_program(
        *raygen_shader,
        &RayTracingProgramDesc {
            program_type: RayTracingProgramType::RayGen,
            shaders: {
                let mut shaders: [_; MAX_RAY_TRACING_SHADER_TYPE] =
                    array_init::array_init(|_| None);
                shaders[RayTracingShaderType::RayGen as usize] = Some(RayTracingShaderDesc {
                    entry_point: "main".to_owned(),
                    shader_data: smol::block_on(
                        CompileRayTracingShader {
                            path: raygen_shader_path,
                        }
                        .into_lazy()
                        .eval(&lazy_cache),
                    )?
                    .spirv
                    .clone(),
                });
                shaders
            },
            signature: RenderShaderSignatureDesc::default(), // TODO
        },
        "raygen shader".into(),
    )?;

    let miss_shader =
        OwnedRenderResourceHandle::new(handles.allocate(RenderResourceType::RayTracingProgram));
    device.read()?.create_ray_tracing_program(
        *miss_shader,
        &RayTracingProgramDesc {
            program_type: RayTracingProgramType::Miss,
            shaders: {
                let mut shaders: [_; MAX_RAY_TRACING_SHADER_TYPE] =
                    array_init::array_init(|_| None);
                shaders[RayTracingShaderType::Miss as usize] = Some(RayTracingShaderDesc {
                    entry_point: "main".to_owned(),
                    shader_data: smol::block_on(
                        CompileRayTracingShader {
                            path: miss_shader_path,
                        }
                        .into_lazy()
                        .eval(&lazy_cache),
                    )?
                    .spirv
                    .clone(),
                });
                shaders
            },
            signature: RenderShaderSignatureDesc::default(), // TODO
        },
        "miss shader".into(),
    )?;

    let hit_shader =
        OwnedRenderResourceHandle::new(handles.allocate(RenderResourceType::RayTracingProgram));
    device.read()?.create_ray_tracing_program(
        *hit_shader,
        &RayTracingProgramDesc {
            program_type: RayTracingProgramType::Hit,
            shaders: {
                let mut shaders: [_; MAX_RAY_TRACING_SHADER_TYPE] =
                    array_init::array_init(|_| None);
                shaders[RayTracingShaderType::ClosestHit as usize] = Some(RayTracingShaderDesc {
                    entry_point: "main".to_owned(),
                    shader_data: smol::block_on(
                        CompileRayTracingShader {
                            path: hit_shader_path,
                        }
                        .into_lazy()
                        .eval(&lazy_cache),
                    )?
                    .spirv
                    .clone(),
                });
                shaders
            },
            signature: RenderShaderSignatureDesc::default(), // TODO
        },
        "hit shader".into(),
    )?;

    let rt_pipeline_state = handles.allocate(RenderResourceType::RayTracingPipelineState);
    device.read()?.create_ray_tracing_pipeline_state(
        rt_pipeline_state,
        &RayTracingPipelineStateDesc {
            programs: vec![*raygen_shader, *miss_shader, *hit_shader],
        },
        "rt pipeline state".into(),
    )?;

    Ok(rt_pipeline_state)
}
