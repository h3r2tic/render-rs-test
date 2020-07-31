use crate::{graph::RenderGraphExecutionParams, resource::*, shader_cache::*};

use render_core::types::*;
use std::{path::Path, sync::Arc};

pub struct ResourceRegistry<'exec_params, 'device, 'shader_cache, 'res_alloc> {
    pub execution_params:
        &'exec_params RenderGraphExecutionParams<'device, 'shader_cache, 'res_alloc>,
    pub(crate) resources: Vec<RenderResourceHandle>,
}

impl<'exec_params, 'device, 'shader_cache, 'res_alloc>
    ResourceRegistry<'exec_params, 'device, 'shader_cache, 'res_alloc>
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

    pub fn shader(
        &self,
        shader_path: impl AsRef<Path>,
        shader_type: RenderShaderType,
    ) -> anyhow::Result<Arc<ShaderCacheEntry>> {
        self.execution_params.shader_cache.get_or_load(
            self.execution_params,
            shader_type,
            shader_path.as_ref(),
        )
    }
}
