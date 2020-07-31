use crate::{graph::RenderGraphExecutionParams, pipeline::ComputePipeline, resource::*};

use render_core::types::*;
use std::{path::Path, sync::Arc};

pub struct ResourceRegistry<'exec_params, 'device, 'pipeline_cache, 'res_alloc> {
    pub execution_params:
        &'exec_params RenderGraphExecutionParams<'device, 'pipeline_cache, 'res_alloc>,
    pub(crate) resources: Vec<RenderResourceHandle>,
}

impl<'exec_params, 'device, 'pipeline_cache, 'res_alloc>
    ResourceRegistry<'exec_params, 'device, 'pipeline_cache, 'res_alloc>
{
    pub fn get<T: Resource, GpuResType>(&self, resource: Ref<T, GpuResType>) -> GpuResType
    where
        GpuResType: ToGpuResourceView,
    {
        // println!("ResourceRegistry::get: {:?}", resource.handle);
        <GpuResType as ToGpuResourceView>::to_gpu_resource_view(
            self.resources[resource.handle.id as usize],
        )
    }

    pub fn compute_pipeline(
        &self,
        shader_path: impl AsRef<Path>,
    ) -> anyhow::Result<Arc<ComputePipeline>> {
        self.execution_params
            .pipeline_cache
            .get_or_load_compute(self.execution_params, shader_path.as_ref())
    }
}
