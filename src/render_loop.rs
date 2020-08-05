use crate::{
    owned_resource::get_resources_pending_release,
    render_device::{FrameResources, MaybeRenderDevice},
};

use render_core::{encoder::RenderCommandList, handles::*, types::*};
use rg::{DynamicConstants, ResourceHandleAllocator};
use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
};

pub struct RenderLoop<'a> {
    device: MaybeRenderDevice,
    persistent_resources: Vec<RenderResourceHandle>,
    retired_frames: VecDeque<Option<FrameResources>>,
    error_output_texture: RenderResourceHandle,
    main_command_list: RenderCommandList<'a>,
    early_command_list: RenderCommandList<'a>,
    dynamic_constants: DynamicConstants,
    handles: Arc<RwLock<RenderResourceHandleAllocator>>,
}

impl<'a> RenderLoop<'a> {
    pub fn new(
        device: MaybeRenderDevice,
        handles: Arc<RwLock<RenderResourceHandleAllocator>>,
        error_output_texture: RenderResourceHandle,
    ) -> Self {
        let mut retired_frames: VecDeque<Option<FrameResources>> = Default::default();
        retired_frames.push_back(None);
        retired_frames.push_back(None);

        let main_command_list =
            RenderCommandList::new(handles.clone(), 1024 * 1024 * 16, 1024 * 1024).unwrap();

        let early_command_list =
            RenderCommandList::new(handles.clone(), 1024 * 1024 * 16, 1024 * 1024).unwrap();

        let dynamic_constants = DynamicConstants::new(handles.clone());

        Self {
            device,
            persistent_resources: Default::default(),
            retired_frames,
            error_output_texture,
            main_command_list,
            early_command_list,
            dynamic_constants,
            handles,
        }
    }

    pub fn render_frame(
        &mut self,
        swapchain: RenderResourceHandle,
        pipeline_cache: &rg::pipeline_cache::PipelineCache,
        graph_gen_fn: impl FnOnce() -> (rg::RenderGraph, rg::Handle<rg::Texture>),
    ) -> anyhow::Result<()> {
        let device = &mut *self.device.write()?;

        if let Some(frame_resources) = self.retired_frames.pop_front().unwrap() {
            if let Some(fence) = frame_resources.resources_used_fence {
                device.wait_for_fence(fence)?;
            }

            frame_resources.destroy_now(&*device);
        }

        let mut frame_resources = FrameResources::default();
        let handle_allocator = rg::TrackingResourceHandleAllocator::new(self.handles.clone());

        let command_list_handle =
            handle_allocator.allocate_transient(RenderResourceType::CommandList);

        device.create_command_list(command_list_handle, "Main command list".into())?;

        let early_command_list = &mut self.early_command_list;
        early_command_list.reset();

        let mut main_command_list = &mut self.main_command_list;
        main_command_list.reset();

        let resources_used_fence = handle_allocator.allocate_transient(RenderResourceType::Fence);

        let output_texture = {
            let (rg, tex) = (graph_gen_fn)();

            // println!("Recorded {} passes", rg.passes.len());
            let execution_output = rg.execute(
                rg::RenderGraphExecutionParams {
                    handles: &handle_allocator,
                    device: &*device,
                    pipeline_cache,
                },
                &mut self.dynamic_constants,
                &mut main_command_list,
                tex,
            );

            let mut allocated_resources = handle_allocator.into_allocated_resources();

            frame_resources
                .handles
                .append(&mut allocated_resources.transient);

            frame_resources
                .handles
                .append(&mut get_resources_pending_release());

            self.persistent_resources
                .append(&mut allocated_resources.persistent);

            execution_output.map(|execution_output| execution_output.output_texture)
        };

        device.create_fence(
            resources_used_fence,
            &RenderFenceDesc {
                cross_device: false,
            },
            "resource usage fence".into(),
        )?;

        frame_resources.resources_used_fence = Some(resources_used_fence);

        self.dynamic_constants
            .commit_and_reset(early_command_list, device);

        device.compile_command_lists(
            command_list_handle,
            &[early_command_list, main_command_list],
        )?;

        device.submit_command_list(command_list_handle, true, None, None, None)?;

        let result = match output_texture {
            Ok(output_texture) => {
                device.present_swap_chain(
                    swapchain,
                    output_texture,
                    frame_resources.resources_used_fence,
                )?;
                Ok(())
            }
            Err(e) => {
                device.present_swap_chain(
                    swapchain,
                    self.error_output_texture,
                    frame_resources.resources_used_fence,
                )?;
                Err(e)
            }
        };

        device.advance_frame()?;
        self.retired_frames.push_back(Some(frame_resources));

        result
    }

    pub fn destroy_resources(&mut self) -> std::result::Result<(), anyhow::Error> {
        let device = &mut *self.device.write()?;

        device.device_wait_idle()?;

        self.dynamic_constants.destroy(device);

        for frame_resources in self.retired_frames.drain(..) {
            if let Some(frame_resources) = frame_resources {
                frame_resources.destroy_now(&*device);
            }
        }

        for resource in self.persistent_resources.drain(..) {
            device.destroy_resource(resource)?;
        }

        Ok(())
    }
}

impl<'a> Drop for RenderLoop<'a> {
    fn drop(&mut self) {
        self.destroy_resources().ok();
    }
}
