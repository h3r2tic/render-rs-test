use crate::shader_cache::ShaderCacheEntry;
use render_core::encoder::RenderCommandList;

pub use render_core::types::{RenderShaderArgument, RenderShaderType};

pub trait RgRenderCommandListExtension {
    fn rg_dispatch_2d(
        &mut self,
        shader: &ShaderCacheEntry,
        thread_count: [u32; 2],
        shader_arguments: &[RenderShaderArgument],
    ) -> anyhow::Result<()>;
}

impl RgRenderCommandListExtension for RenderCommandList<'_> {
    fn rg_dispatch_2d(
        &mut self,
        shader: &ShaderCacheEntry,
        thread_count: [u32; 2],
        shader_arguments: &[RenderShaderArgument],
    ) -> anyhow::Result<()> {
        self.dispatch_2d(
            shader.pipeline_handle,
            shader_arguments,
            thread_count[0],
            thread_count[1],
            Some(shader.group_size[0]),
            Some(shader.group_size[1]),
        )?;

        Ok(())
    }
}
