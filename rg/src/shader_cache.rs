use crate::graph::RenderGraphExecutionParams;
use render_core::{handles::*, types::*};
use std::{path::Path, sync::Arc};

// TODO: figure out the ownership model -- should this release the resources?
// TODO: non-compute
pub struct ShaderCacheEntry {
    pub shader_handle: RenderResourceHandle,
    pub pipeline_handle: RenderResourceHandle,
    pub srvs: Vec<String>,
    pub uavs: Vec<String>,
    pub group_size: [u32; 3],
}

pub trait ShaderCache {
    fn get_or_load(
        &self,
        params: &RenderGraphExecutionParams<'_, '_, '_>,
        shader_type: RenderShaderType,
        path: &Path,
    ) -> anyhow::Result<Arc<ShaderCacheEntry>>;
}
