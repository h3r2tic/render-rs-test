use crate::renderer;

use render_core::{device::RenderDevice, encoder::RenderCommandList, handles::*, types::*};
use std::collections::VecDeque;

pub struct RenderLoop {
    persistent_resources: Vec<RenderResourceHandle>,
    retired_frames: VecDeque<Option<renderer::FrameResources>>,
    error_output_texture: RenderResourceHandle,
}

impl RenderLoop {
    pub fn new(error_output_texture: RenderResourceHandle) -> Self {
        let mut retired_frames: VecDeque<Option<renderer::FrameResources>> = Default::default();
        retired_frames.push_back(None);
        retired_frames.push_back(None);

        Self {
            persistent_resources: Default::default(),
            retired_frames,
            error_output_texture,
        }
    }

    pub fn render_frame(
        &mut self,
        device: &mut dyn RenderDevice,
        swapchain: RenderResourceHandle,
        renderer: &renderer::Renderer,
        shader_cache: &dyn rg::shader_cache::ShaderCache,
    ) -> anyhow::Result<()> {
        if let Some(frame_resources) = self.retired_frames.pop_front().unwrap() {
            if let Some(fence) = frame_resources.resources_used_fence {
                device.wait_for_fence(fence)?;
            }

            frame_resources.destroy_now(&*device);
        }

        let mut frame_resources = renderer::FrameResources::default();

        let main_command_list_handle =
            renderer.allocate_frame_handle(RenderResourceType::CommandList, &mut frame_resources);
        device.create_command_list(main_command_list_handle, "Main command list".into())?;

        let mut cb =
            RenderCommandList::new(renderer.handles.clone(), 1024 * 1024 * 16, 1024 * 1024)?;

        let handle_allocator = rg::TrackingResourceHandleAllocator::new(renderer.handles.clone());

        let output_texture = {
            let (rg, tex) = crate::render_passes::render_frame_rg();

            // println!("Recorded {} passes", rg.passes.len());
            let execution_output = rg.execute(
                rg::RenderGraphExecutionParams {
                    handles: &handle_allocator,
                    device: &*device,
                    shader_cache: shader_cache,
                },
                &mut cb,
                tex,
            );

            let mut allocated_resources = handle_allocator.into_allocated_resources();

            frame_resources
                .handles
                .append(&mut allocated_resources.transient);

            self.persistent_resources
                .append(&mut allocated_resources.persistent);

            execution_output.map(|execution_output| execution_output.output_texture)
        };

        let resources_used_fence =
            renderer.allocate_frame_handle(RenderResourceType::Fence, &mut frame_resources);
        device.create_fence(
            resources_used_fence,
            &RenderFenceDesc {
                cross_device: false,
            },
            "resource usage fence".into(),
        )?;
        frame_resources.resources_used_fence = Some(resources_used_fence);

        device.compile_command_list(main_command_list_handle, &cb)?;
        device.submit_command_list(main_command_list_handle, true, None, None, None)?;

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

    pub fn destroy_resources(
        &mut self,
        device: &dyn RenderDevice,
    ) -> std::result::Result<(), anyhow::Error> {
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
