use crate::rg;
use render_core::{
    encoder::RenderCommandList,
    handles::RenderResourceHandle,
    state::build,
    types::{RenderFormat, RenderShaderArgument, RenderShaderViewsDesc},
};
use rg::ShaderCacheEntry;
use std::sync::Arc;

pub trait AddShaderPipelineBinding {
    fn add_shader_pipeline_binding(
        self,
        bindings: &mut RenderShaderViewsDesc,
        binding_ordinal: u32,
    );
}

impl AddShaderPipelineBinding for rg::GpuSrv {
    fn add_shader_pipeline_binding(
        self,
        bindings: &mut RenderShaderViewsDesc,
        binding_ordinal: u32,
    ) {
        // TODO
        bindings.shader_resource_views[binding_ordinal as usize] =
            build::texture_2d(self.0, RenderFormat::R32g32b32a32Float, 0, 1, 0, 0.0f32);
    }
}

impl AddShaderPipelineBinding for rg::GpuUav {
    fn add_shader_pipeline_binding(
        self,
        bindings: &mut RenderShaderViewsDesc,
        binding_ordinal: u32,
    ) {
        // TODO
        bindings.unordered_access_views[binding_ordinal as usize] =
            build::texture_2d_rw(self.0, RenderFormat::R32g32b32a32Float, 0, 0);
    }
}

#[macro_export]
macro_rules! dispatch_2d {
    (
        $cb:expr,
        $registry:expr,
        $thread_count:expr,
        $shader_path:expr,
        $($binding:expr => $binding_ident:ident),*
        $(,)?
    ) => {{
        let thread_count = $thread_count;

        struct BindingOrdinals {
            $($binding_ident: u32),*
        }

        let pipeline_info = $registry.compute_shader(
            $shader_path,
            &[
                $(stringify!($binding_ident)),*
            ]
        );
        let ordinals: &BindingOrdinals = unsafe {
            (pipeline_info.queried_binding_ordinals.as_ptr() as *const BindingOrdinals).as_ref().unwrap()
        };

        let mut resource_views = RenderShaderViewsDesc {
            shader_resource_views: vec![Default::default(); pipeline_info.srv_count],
            unordered_access_views: vec![Default::default(); pipeline_info.uav_count],
        };

        $(
            let resource = $registry.get($binding);
            $crate::rg_helper::AddShaderPipelineBinding::add_shader_pipeline_binding(resource, &mut resource_views, ordinals.$binding_ident);
        )*

        let resource_views_handle =
            $registry.execution_params.handles.allocate_transient(RenderResourceType::ShaderViews);

        $registry.execution_params.device.create_shader_views(
            resource_views_handle,
            &resource_views,
            "shader resource views".into(),
        )?;

        $cb.dispatch_2d(
            pipeline_info.pipeline_handle,
            &[RenderShaderArgument {
                constant_buffer: None,
                shader_views: Some(resource_views_handle),
                constant_buffer_offset: 0,
            }],
            thread_count.0,
            thread_count.1,
            // TODO
            Some(8),
            Some(8),
        )?;

        std::result::Result::<_, failure::Error>::Ok(())
    }};
}

pub trait RgRenderCommandListExtension {
    fn rg_dispatch_2d(
        &mut self,
        shader: Arc<ShaderCacheEntry>,
        thread_count: (u32, u32),
        shader_arguments: &[RenderShaderArgument],
    ) -> std::result::Result<(), failure::Error>;
}

impl RgRenderCommandListExtension for RenderCommandList<'_> {
    fn rg_dispatch_2d(
        &mut self,
        shader: Arc<ShaderCacheEntry>,
        thread_count: (u32, u32),
        shader_arguments: &[RenderShaderArgument],
    ) -> Result<(), failure::Error> {
        self.dispatch_2d(
            shader.pipeline_handle,
            shader_arguments,
            thread_count.0,
            thread_count.1,
            Some(shader.group_size_x),
            Some(shader.group_size_y),
        )?;

        Ok(())
    }
}
