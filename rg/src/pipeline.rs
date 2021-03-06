use render_core::{handles::RenderResourceHandle, state::RenderState};
use std::path::PathBuf;

pub struct ComputePipeline {
    pub handle: RenderResourceHandle,
    pub group_size: [u32; 3],
    pub srvs: Vec<String>,
    pub uavs: Vec<String>,
}

pub struct RasterPipeline {
    pub handle: RenderResourceHandle,
}

pub struct RasterPipelineDesc {
    pub vertex_shader: PathBuf,
    pub pixel_shader: PathBuf,
    pub render_state: RenderState,
}
