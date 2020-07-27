use crate::rg::*;
use render_core::{encoder::RenderCommandList, handles::*, state::build, types::*};
use std::sync::Arc;

pub use render_core::types::{RenderShaderArgument, RenderShaderType};

pub trait RgRenderCommandListExtension {
    fn rg_dispatch_2d(
        &mut self,
        shader: &ShaderCacheEntry,
        thread_count: (u32, u32),
        shader_arguments: &[RenderShaderArgument],
    ) -> std::result::Result<(), failure::Error>;
}

impl RgRenderCommandListExtension for RenderCommandList<'_> {
    fn rg_dispatch_2d(
        &mut self,
        shader: &ShaderCacheEntry,
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

pub mod srv {
    use crate::rg::*;

    pub struct RgSrv {
        pub rg_ref: TextureRef<GpuSrv>,
    }

    pub fn texture_2d(rg_ref: TextureRef<GpuSrv>) -> RgSrv {
        RgSrv { rg_ref }
    }
}

pub mod uav {
    use crate::rg::*;

    pub struct RgUav {
        pub rg_ref: TextureRef<GpuUav>,
    }

    pub fn texture_2d(rg_ref: TextureRef<GpuUav>) -> RgUav {
        RgUav { rg_ref }
    }
}

pub trait NamedShaderViews {
    fn named_views(
        &self,
        registry: &ResourceRegistry,
        srvs: &[(&'static str, srv::RgSrv)],
        uavs: &[(&'static str, uav::RgUav)],
    ) -> RenderResourceHandle;
}

impl NamedShaderViews for Arc<ShaderCacheEntry> {
    fn named_views(
        &self,
        registry: &ResourceRegistry,
        srvs: &[(&'static str, srv::RgSrv)],
        uavs: &[(&'static str, uav::RgUav)],
    ) -> RenderResourceHandle {
        let mut resource_views = RenderShaderViewsDesc {
            shader_resource_views: vec![Default::default(); srvs.len()],
            unordered_access_views: vec![Default::default(); uavs.len()],
        };

        for (srv_name, srv) in srvs.into_iter() {
            let binding_idx = self
                .srvs
                .iter()
                .position(|name| name == srv_name)
                .expect(srv_name);

            // TODO: other binding types
            resource_views.shader_resource_views[binding_idx] = build::texture_2d(
                registry.get(srv.rg_ref.internal_clone()).0,
                RenderFormat::R32g32b32a32Float,
                0,
                1,
                0,
                0.0f32,
            );
        }

        for (uav_name, uav) in uavs.into_iter() {
            let binding_idx = self
                .uavs
                .iter()
                .position(|name| name == uav_name)
                .expect(uav_name);

            // TODO: other binding types
            resource_views.unordered_access_views[binding_idx] = build::texture_2d_rw(
                registry.get(uav.rg_ref.internal_clone()).0,
                RenderFormat::R32g32b32a32Float,
                0,
                0,
            );
        }

        // TODO: verify that all entries have been written to

        let resource_views_handle = registry
            .execution_params
            .handles
            .allocate_transient(RenderResourceType::ShaderViews);

        registry
            .execution_params
            .device
            .create_shader_views(
                resource_views_handle,
                &resource_views,
                "shader resource views".into(),
            )
            .unwrap();

        resource_views_handle
    }
}
