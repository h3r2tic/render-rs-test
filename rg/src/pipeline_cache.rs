use crate::{
    pipeline::ComputePipeline,
    shader_cache::{ShaderCache, ShaderCacheEntry},
    RenderGraphExecutionParams,
};
use render_core::{
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

type ComputePipelines = HashMap<Arc<ShaderCacheEntry>, Arc<ComputePipeline>>;

pub struct PipelineCache {
    pub shader_cache: Box<dyn ShaderCache>,
    compute: Arc<RwLock<ComputePipelines>>,
}

impl PipelineCache {
    pub fn new(shader_cache: impl ShaderCache + 'static) -> Self {
        Self {
            shader_cache: Box::new(shader_cache),
            compute: Default::default(),
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

        let mut compute_pipes = self.compute.write().unwrap();
        if let Some(retired) = shader_cache_entry.retired {
            compute_pipes.remove(&retired);
        }

        let shader = shader_cache_entry.entry?;

        Ok(match compute_pipes.entry(shader.clone()) {
            std::collections::hash_map::Entry::Occupied(occupied) => occupied.get().clone(),
            std::collections::hash_map::Entry::Vacant(vacant) => {
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
