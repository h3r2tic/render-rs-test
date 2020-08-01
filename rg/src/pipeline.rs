use render_core::handles::RenderResourceHandle;
use std::path::PathBuf;

pub struct ComputePipeline {
    pub(crate) handle: RenderResourceHandle,
    pub(crate) group_size: [u32; 3],
    pub(crate) srvs: Vec<String>,
    pub(crate) uavs: Vec<String>,
}

pub struct RasterPipeline {
    pub(crate) handle: RenderResourceHandle,
}

#[derive(Hash, Eq, PartialEq)]
pub struct RasterPipelineDesc {
    pub vertex_shader: PathBuf,
    pub pixel_shader: PathBuf,
}
