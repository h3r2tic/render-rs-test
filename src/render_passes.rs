use crate::{bytes::as_byte_slice, camera::CameraMatrices, mesh::GpuTriangleMesh};
use render_core::{
    state::RenderState,
    types::{
        RenderBindFlags, RenderBufferDesc, RenderDrawPacket, RenderFormat, RenderResourceType,
        RenderTargetInfo,
    },
};
use rg::{command_ext::*, resource_view::*, *};
use std::{mem::size_of, sync::Arc};

pub fn render_frame_rg(
    camera_matrices: CameraMatrices,
    mesh: Arc<GpuTriangleMesh>,
) -> (RenderGraph, Handle<Texture>) {
    let mut rg = RenderGraph::new();

    let mut tex = synth_gradients(
        &mut rg,
        TextureDesc {
            width: 1280,
            height: 720,
            format: RenderFormat::R16g16b16a16Float,
        },
    );

    raster_mesh(camera_matrices, mesh, &mut rg, &mut tex);

    let tex = blur(&mut rg, &tex);
    //let tex = into_ycbcr(&mut rg, tex);

    (rg, tex)
}

fn raster_mesh(
    camera_matrices: CameraMatrices,
    mesh: Arc<GpuTriangleMesh>,
    rg: &mut RenderGraph,
    output: &mut Handle<Texture>,
) {
    let mut pass = rg.add_pass();
    let output_ref = pass.raster(output);

    pass.render(move |cb, registry| {
        let render_target = RenderTarget::new([(output_ref, RenderTargetInfo::default())]);

        let pipeline = registry.raster_pipeline(
            RasterPipelineDesc {
                vertex_shader: "/assets/shaders/raster_simple_vs.hlsl".into(),
                pixel_shader: "/assets/shaders/raster_simple_ps.hlsl".into(),
                render_state: RenderState {
                    depth_enable: false,
                    ..Default::default()
                },
            },
            &render_target,
        )?;

        #[derive(Clone, Copy)]
        #[repr(C)]
        struct Constants {
            camera_matrices: CameraMatrices,
        }

        let constants = Constants { camera_matrices };
        let constants = {
            let handle = registry
                .execution_params
                .handles
                .allocate_transient(RenderResourceType::Buffer);

            registry.execution_params.device.create_buffer(
                handle,
                &RenderBufferDesc {
                    bind_flags: RenderBindFlags::CONSTANT_BUFFER,
                    size: size_of::<Constants>(),
                },
                Some(as_byte_slice(&constants)),
                "index buffer".into(),
            )?;
            handle
        };

        cb.begin_render_pass(registry.render_pass(&render_target)?)?;
        cb.draw(
            pipeline.handle,
            &[RenderShaderArgument {
                shader_views: Some(*mesh.shader_views),
                constant_buffer: Some(constants),
                constant_buffer_offset: 0,
            }],
            Some(*mesh.draw_binding),
            &render_target.to_draw_state(),
            &RenderDrawPacket {
                vertex_count: mesh.index_count,
                ..Default::default()
            },
        )?;
        cb.end_render_pass()?;

        Ok(())
    });
}

fn synth_gradients(rg: &mut RenderGraph, desc: TextureDesc) -> Handle<Texture> {
    let mut pass = rg.add_pass();
    let mut output = pass.create(&desc);
    let output_ref = pass.write(&mut output);

    pass.render(move |cb, registry| {
        let pipeline = registry.compute_pipeline("/assets/shaders/gradients.hlsl")?;
        cb.rg_dispatch_2d(
            &pipeline,
            output_ref.desc().dims(),
            &[RenderShaderArgument {
                shader_views: Some(pipeline.named_views(
                    registry,
                    &[],
                    &[("output_tex", uav::texture_2d(output_ref))],
                )),
                constant_buffer: None,
                constant_buffer_offset: 0,
            }],
        )
    });

    output
}

fn blur(rg: &mut RenderGraph, input: &Handle<Texture>) -> Handle<Texture> {
    let mut pass = rg.add_pass();
    let input_ref = pass.read(input);

    let mut output = pass.create(input.desc());
    let output_ref = pass.write(&mut output);

    pass.render(move |cb, registry| {
        let pipeline = registry.compute_pipeline("/assets/shaders/blur.hlsl")?;
        cb.rg_dispatch_2d(
            &pipeline,
            input_ref.desc().dims(),
            &[RenderShaderArgument {
                shader_views: Some(pipeline.named_views(
                    registry,
                    &[("input_tex", srv::texture_2d(input_ref))],
                    &[("output_tex", uav::texture_2d(output_ref))],
                )),
                constant_buffer: None,
                constant_buffer_offset: 0,
            }],
        )
    });

    output
}

#[allow(dead_code)]
fn into_ycbcr(rg: &mut RenderGraph, mut input: Handle<Texture>) -> Handle<Texture> {
    let mut pass = rg.add_pass();
    let input_ref = pass.write(&mut input);

    pass.render(move |cb, registry| {
        let pipeline = registry.compute_pipeline("/assets/shaders/into_ycbcr.hlsl")?;
        cb.rg_dispatch_2d(
            &pipeline,
            input_ref.desc().dims(),
            &[RenderShaderArgument {
                shader_views: Some(pipeline.named_views(
                    registry,
                    &[],
                    &[("input_tex", uav::texture_2d(input_ref))],
                )),
                constant_buffer: None,
                constant_buffer_offset: 0,
            }],
        )
    });

    input
}
