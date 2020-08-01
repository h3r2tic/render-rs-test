use crate::graph::RenderGraphExecutionParams;
use render_core::{handles::*, types::*};
use std::{path::Path, sync::Arc};

pub struct ComputeShaderCacheEntry {
    pub shader_handle: RenderResourceHandle,
    pub srvs: Vec<String>,
    pub uavs: Vec<String>,
    pub group_size: [u32; 3],
}

pub struct RasterShaderCacheEntry {
    pub shader_handle: RenderResourceHandle,
    pub stage: RenderShaderType,
}

// TODO: figure out the ownership model -- should this release the resources?
pub enum ShaderCacheEntry {
    Compute(ComputeShaderCacheEntry),
    Raster(RasterShaderCacheEntry),
}

impl ShaderCacheEntry {
    pub fn shader_handle(&self) -> RenderResourceHandle {
        match self {
            Self::Compute(ComputeShaderCacheEntry { shader_handle, .. })
            | Self::Raster(RasterShaderCacheEntry { shader_handle, .. }) => *shader_handle,
        }
    }
}

impl std::hash::Hash for ShaderCacheEntry {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.shader_handle().hash(state)
    }
}
impl PartialEq for ShaderCacheEntry {
    fn eq(&self, other: &Self) -> bool {
        self.shader_handle() == other.shader_handle()
    }
}
impl Eq for ShaderCacheEntry {}

pub struct ShaderCacheOutput {
    pub entry: anyhow::Result<Arc<ShaderCacheEntry>>,
    pub retired: Option<Arc<ShaderCacheEntry>>,
}

pub trait ShaderCache {
    fn get_or_load(
        &self,
        params: &RenderGraphExecutionParams<'_, '_, '_>,
        shader_type: RenderShaderType,
        path: &Path,
    ) -> ShaderCacheOutput;
}
