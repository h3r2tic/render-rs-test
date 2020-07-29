use crate::{graph::RenderGraphExecutionParams, resource::*, shader_cache::*};

use render_core::types::*;
use std::{path::Path, sync::Arc};

pub struct ResourceRegistry<'exec_params, 'device, 'shader_cache> {
    pub execution_params: &'exec_params RenderGraphExecutionParams<'device, 'shader_cache>,
    pub(crate) resources: Vec<GpuResource>,
}

impl<'exec_params, 'device, 'shader_cache> ResourceRegistry<'exec_params, 'device, 'shader_cache> {
    pub fn get<T, GpuResType>(
        &self,
        resource: impl std::ops::Deref<Target = RawResourceRef<T, GpuResType>>,
    ) -> GpuResType
    where
        GpuResType: ToGpuResourceView,
    {
        // println!("ResourceRegistry::get: {:?}", resource.handle);
        <GpuResType as ToGpuResourceView>::to_gpu_resource_view(
            &self.resources[resource.handle.id as usize],
        )
    }

    pub fn shader(
        &self,
        shader_path: impl AsRef<Path>,
        shader_type: RenderShaderType,
    ) -> Arc<ShaderCacheEntry> {
        self.execution_params.shader_cache.get_or_load(
            self.execution_params,
            shader_type,
            shader_path.as_ref(),
        )
    }
}
