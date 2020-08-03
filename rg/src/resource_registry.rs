use crate::{
    graph::RenderGraphExecutionParams,
    pipeline::{ComputePipeline, RasterPipeline, RasterPipelineDesc},
    resource::*,
};

use render_core::{
    constants::MAX_RENDER_TARGET_COUNT,
    state::{RenderBindingRenderTargetView, RenderBindingView},
    types::*,
};
use std::{path::Path, sync::Arc};

pub struct ResourceRegistry<'exec_params, 'device, 'pipeline_cache, 'res_alloc> {
    pub execution_params:
        &'exec_params RenderGraphExecutionParams<'device, 'pipeline_cache, 'res_alloc>,
    pub(crate) resources: Vec<RenderResourceHandle>,
}

impl<'exec_params, 'device, 'pipeline_cache, 'res_alloc>
    ResourceRegistry<'exec_params, 'device, 'pipeline_cache, 'res_alloc>
{
    pub fn get<T: Resource, GpuResType>(&self, resource: Ref<T, GpuResType>) -> GpuResType
    where
        GpuResType: ToGpuResourceView,
    {
        // println!("ResourceRegistry::get: {:?}", resource.handle);
        <GpuResType as ToGpuResourceView>::to_gpu_resource_view(
            self.resources[resource.handle.id as usize],
        )
    }

    pub fn compute_pipeline(
        &self,
        shader_path: impl AsRef<Path>,
    ) -> anyhow::Result<Arc<ComputePipeline>> {
        self.execution_params
            .pipeline_cache
            .get_or_load_compute(self.execution_params, shader_path.as_ref())
    }

    pub fn render_pass(&self, rts: &[Ref<Texture, GpuRt>]) -> anyhow::Result<RenderResourceHandle> {
        let device = self.execution_params.device;

        let frame_binding_set_handle = self
            .execution_params
            .handles
            .allocate_transient(RenderResourceType::FrameBindingSet);

        let mut render_target_views = [None; MAX_RENDER_TARGET_COUNT];
        for (i, rt) in rts.iter().enumerate() {
            render_target_views[i] = Some(RenderBindingRenderTargetView {
                base: RenderBindingView {
                    resource: self.resources[rt.handle.id as usize],
                    format: RenderFormat::R32g32b32a32Float, // TODO
                    dimension: RenderViewDimension::Tex2d,
                },
                mip_slice: 0,
                first_array_slice: 0,
                plane_slice_first_w_slice: 0,
                array_size: 0,
                w_size: 0,
            });
        }

        device.create_frame_binding_set(
            frame_binding_set_handle,
            &RenderFrameBindingSetDesc {
                render_target_views,
                depth_stencil_view: None,
            },
            "draw binding set".into(),
        )?;

        let render_pass_handle = self
            .execution_params
            .handles
            .allocate_transient(RenderResourceType::RenderPass);

        device.create_render_pass(
            render_pass_handle,
            &RenderPassDesc {
                frame_binding: frame_binding_set_handle,
                // TODO
                render_target_info: [RenderTargetInfo {
                    load_op: RenderLoadOp::Discard,
                    store_op: RenderStoreOp::Store,
                    clear_color: [0.0f32; 4],
                }; MAX_RENDER_TARGET_COUNT],
                depth_stencil_target_info: DepthStencilTargetInfo {
                    load_op: RenderLoadOp::Discard,
                    store_op: RenderStoreOp::Discard,
                    clear_depth: 1.0,
                    clear_stencil: 0,
                },
            },
            "render pass".into(),
        )?;

        Ok(render_pass_handle)
    }

    pub fn raster_pipeline(&self, desc: RasterPipelineDesc) -> anyhow::Result<Arc<RasterPipeline>> {
        self.execution_params
            .pipeline_cache
            .get_or_load_raster(self.execution_params, desc)
    }
}
