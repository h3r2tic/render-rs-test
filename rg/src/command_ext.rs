use crate::pipeline::ComputePipeline;
use render_core::encoder::RenderCommandList;

pub use render_core::types::{RenderShaderArgument, RenderShaderType};
use std::sync::Arc;

pub trait RgRenderCommandListExtension {
    fn rg_dispatch_2d(
        &mut self,
        pipeline: &Arc<ComputePipeline>,
        thread_count: [u32; 2],
        shader_arguments: &[RenderShaderArgument],
    ) -> anyhow::Result<()>;
}

impl RgRenderCommandListExtension for RenderCommandList<'_> {
    fn rg_dispatch_2d(
        &mut self,
        pipeline: &Arc<ComputePipeline>,
        thread_count: [u32; 2],
        shader_arguments: &[RenderShaderArgument],
    ) -> anyhow::Result<()> {
        self.dispatch_2d(
            pipeline.handle,
            shader_arguments,
            thread_count[0],
            thread_count[1],
            Some(pipeline.group_size[0]),
            Some(pipeline.group_size[1]),
        )?;

        Ok(())
    }
}
