use crate::mesh::{GpuTriangleMesh, RasterGpuVertex};
use render_core::{
    state::{build, RenderState},
    types::{
        RenderDrawPacket, RenderFormat, RenderResourceType, RenderShaderViewsDesc, RenderTargetInfo,
    },
};
use rg::{command_ext::*, resource_view::*, *};
use std::{mem::size_of, sync::Arc};

pub fn render_frame_rg(mesh: Arc<GpuTriangleMesh>) -> (RenderGraph, Handle<Texture>) {
    let mut rg = RenderGraph::new();

    let mut tex = synth_gradients(
        &mut rg,
        TextureDesc {
            width: 1280,
            height: 720,
            format: RenderFormat::R16g16b16a16Float,
        },
    );
    raster_mesh(mesh, &mut rg, &mut tex);

    let tex = blur(&mut rg, &tex);
    let tex = into_ycbcr(&mut rg, tex);

    (rg, tex)
}

fn raster_mesh(mesh: Arc<GpuTriangleMesh>, rg: &mut RenderGraph, output: &mut Handle<Texture>) {
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

        cb.begin_render_pass(registry.render_pass(&render_target)?)?;
        cb.draw(
            pipeline.handle,
            &[RenderShaderArgument {
                shader_views: Some({
                    let resource_views = RenderShaderViewsDesc {
                        shader_resource_views: vec![build::buffer(
                            mesh.vertex_buffer,
                            RenderFormat::Unknown,
                            0,
                            mesh.vertex_count,
                            size_of::<RasterGpuVertex>() as _,
                        )],
                        unordered_access_views: Vec::new(),
                    };

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
                }),
                ..Default::default()
            }],
            None,
            &render_target.to_draw_state(),
            &RenderDrawPacket {
                vertex_count: mesh.vertex_count,
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
