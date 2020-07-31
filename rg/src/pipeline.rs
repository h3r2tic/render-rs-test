use render_core::handles::RenderResourceHandle;

pub struct ComputePipeline {
    pub(crate) handle: RenderResourceHandle,
    pub(crate) group_size: [u32; 3],
    pub(crate) srvs: Vec<String>,
    pub(crate) uavs: Vec<String>,
}
