use crate::{
    pipeline::{ComputePipeline, RasterPipeline},
    shader_cache::{ShaderCache, ShaderCacheEntry},
    RenderGraphExecutionParams,
};
use render_core::{
    handles::RenderResourceHandle,
    state::RenderComputePipelineStateDesc,
    types::{
        RenderResourceType, RenderShaderParameter, RenderShaderSignatureDesc, RenderShaderType,
    },
};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, RwLock},
};

#[derive(Hash, Eq, PartialEq)]
struct RasterPipelineKey {
    vertex_shader: RenderResourceHandle,
    pixel_shader: RenderResourceHandle,
}
#[derive(Clone, Copy, Hash, Eq, PartialEq, Default)]
struct RasterPipelineId(usize);

type CsToPipeline = HashMap<RenderResourceHandle, Arc<ComputePipeline>>;
type RasterToPipelines = HashMap<RenderResourceHandle, Vec<RasterPipelineId>>;

#[derive(Default)]
pub struct Pipelines {
    compute_shader_to_pipeline: CsToPipeline,

    // Map shaders to all pipelines which use them, so we can evict the pipelines
    // when shaders become invalidated
    raster_shader_to_pipelines: RasterToPipelines,

    // Actual storage of raster pipelines.
    // TODO: use small integers in conjunction with a Vec instead of the HashMap.
    raster_pipelines: HashMap<RasterPipelineId, Arc<RasterPipeline>>,

    // Next unused RasterPipelineId.
    next_raster_pipeline_id: RasterPipelineId,
}

pub struct PipelineCache {
    pub shader_cache: Box<dyn ShaderCache>,
    pub pipelines: Arc<RwLock<Pipelines>>,
}

impl PipelineCache {
    pub fn new(shader_cache: impl ShaderCache + 'static) -> Self {
        Self {
            shader_cache: Box::new(shader_cache),
            pipelines: Default::default(),
        }
    }

    pub fn get_or_load_compute(
        &self,
        params: &RenderGraphExecutionParams<'_, '_, '_>,
        path: &Path,
    ) -> anyhow::Result<Arc<ComputePipeline>> {
        let shader_cache_entry =
            self.shader_cache
                .get_or_load(params, RenderShaderType::Compute, path);

        let mut pipelines = self.pipelines.write().unwrap();
        let compute_pipes = &mut pipelines.compute_shader_to_pipeline;

        if let Some(retired) = shader_cache_entry.retired {
            compute_pipes.remove(&retired.shader_handle());
        }

        let shader = shader_cache_entry.entry?;

        Ok(match compute_pipes.entry(shader.shader_handle()) {
            std::collections::hash_map::Entry::Occupied(occupied) => occupied.get().clone(),
            std::collections::hash_map::Entry::Vacant(vacant) => {
                let shader = match &*shader {
                    ShaderCacheEntry::Compute(shader) => shader,
                    ShaderCacheEntry::Raster(..) => unreachable!(),
                };

                let shader_handle = shader.shader_handle;

                let pipeline_handle = params
                    .handles
                    .allocate_persistent(RenderResourceType::ComputePipelineState);

                params.device.create_compute_pipeline_state(
                    pipeline_handle,
                    &RenderComputePipelineStateDesc {
                        shader: shader_handle,
                        shader_signature: RenderShaderSignatureDesc::new(
                            &[RenderShaderParameter::new(
                                shader.srvs.len() as u32,
                                shader.uavs.len() as u32,
                            )],
                            &[],
                        ),
                    },
                    "gradients compute pipeline".into(),
                )?;

                let pipeline_entry = Arc::new(ComputePipeline {
                    handle: pipeline_handle,
                    group_size: shader.group_size,
                    srvs: shader.srvs.clone(),
                    uavs: shader.uavs.clone(),
                });

                vacant.insert(pipeline_entry.clone());
                pipeline_entry
            }
        })
    }
}
