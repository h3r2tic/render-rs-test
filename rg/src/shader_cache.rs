use crate::graph::RenderGraphExecutionParams;
use render_core::{handles::*, types::*};
use std::{path::Path, sync::Arc};

// TODO: figure out the ownership model -- should this release the resources?
pub struct ShaderCacheEntry {
    pub shader_handle: RenderResourceHandle,
    pub srvs: Vec<String>,
    pub uavs: Vec<String>,
    pub group_size: [u32; 3],
}

impl std::hash::Hash for ShaderCacheEntry {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.shader_handle.hash(state)
    }
}
impl PartialEq for ShaderCacheEntry {
    fn eq(&self, other: &Self) -> bool {
        self.shader_handle == other.shader_handle
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
