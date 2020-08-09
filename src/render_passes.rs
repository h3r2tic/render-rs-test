#![allow(dead_code)]

use crate::{camera::CameraMatrices, mesh::GpuTriangleMesh, RaytraceData};
use render_core::{
    state::{build, RenderShaderTableUpdateDesc, RenderShaderTableUpdateEntry, RenderState},
    types::{
        RenderDrawPacket, RenderFormat, RenderResourceType, RenderShaderViewsDesc, RenderTargetInfo,
    },
};
use rg::{command_ext::*, resource_view::*, *};
use std::sync::Arc;

#[allow(unused_variables)]
pub fn render_frame_rg(
    camera_matrices: CameraMatrices,
    mesh: Arc<GpuTriangleMesh>,
    rt_data: RaytraceData,
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

    //raster_mesh(camera_matrices, mesh, &mut rg, &mut tex);
    test_raytrace(rt_data, &mut rg, &mut tex);

    //let tex = blur(&mut rg, &tex);
    //let tex = into_ycbcr(&mut rg, tex);

    (rg, tex)
}

fn test_raytrace(rt_data: RaytraceData, rg: &mut RenderGraph, output: &mut Handle<Texture>) {
    let mut pass = rg.add_pass();
    let output_ref = pass.write(output);
    let output_desc = *output_ref.desc();

    pass.render(move |cb, resources| {
        let raygen_shader_views = {
            let resource_views = RenderShaderViewsDesc {
                shader_resource_views: vec![build::ray_tracing_acceleration(
                    rt_data.top_acceleration,
                )],
                unordered_access_views: vec![build::texture_2d_rw(
                    resources.resource(output_ref).0,
                    output_desc.format,
                    0,
                    0,
                )],
            };

            let shader_views = resources
                .execution_params
                .handles
                .allocate_transient(RenderResourceType::ShaderViews);

            resources
                .execution_params
                .device
                .create_shader_views(
                    shader_views,
                    &resource_views,
                    "shader resource views".into(),
                )
                .unwrap();
            shader_views
        };

        // TODO: supply`RenderShaderArgument`s to `ray_trace` instead of here (modify API).
        cb.update_shader_table(
            rt_data.shader_table,
            RenderShaderTableUpdateDesc {
                ray_gen_entries: vec![RenderShaderTableUpdateEntry {
                    program: None,
                    shader_arguments: vec![RenderShaderArgument::new(raygen_shader_views)],
                }],
                hit_entries: Default::default(),
                miss_entries: Default::default(),
                pipeline_state: rt_data.pipeline_state,
            },
        )?;

        cb.ray_trace(
            rt_data.pipeline_state,
            rt_data.shader_table,
            rt_data.top_acceleration,
            output_desc.width,
            output_desc.height,
            0,
        )?;
        Ok(())
    });
}

fn raster_mesh(
    camera_matrices: CameraMatrices,
    mesh: Arc<GpuTriangleMesh>,
    rg: &mut RenderGraph,
    output: &mut Handle<Texture>,
) {
    let mut pass = rg.add_pass();
    let output_ref = pass.raster(output);

    pass.render(move |cb, resources| {
        let render_target = RenderTarget::new([(output_ref, RenderTargetInfo::default())]);

        let pipeline = resources.raster_pipeline(
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

        let constants = resources
            .dynamic_constants
            .push(Constants { camera_matrices });

        cb.begin_render_pass(resources.render_pass(&render_target)?)?;
        cb.draw(
            pipeline.handle,
            &[RenderShaderArgument::new(*mesh.shader_views).constants(constants)],
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

    pass.render(move |cb, resources| {
        let pipeline = resources.compute_pipeline("/assets/shaders/gradients.hlsl")?;
        cb.rg_dispatch_2d(
            &pipeline,
            output_ref.desc().dims(),
            &[RenderShaderArgument::new(pipeline.named_views(
                resources,
                &[],
                &[("output_tex", uav::texture_2d(output_ref))],
            ))],
        )
    });

    output
}

fn blur(rg: &mut RenderGraph, input: &Handle<Texture>) -> Handle<Texture> {
    let mut pass = rg.add_pass();
    let input_ref = pass.read(input);

    let mut output = pass.create(input.desc());
    let output_ref = pass.write(&mut output);

    pass.render(move |cb, resources| {
        let pipeline = resources.compute_pipeline("/assets/shaders/blur.hlsl")?;
        cb.rg_dispatch_2d(
            &pipeline,
            input_ref.desc().dims(),
            &[RenderShaderArgument::new(pipeline.named_views(
                resources,
                &[("input_tex", srv::texture_2d(input_ref))],
                &[("output_tex", uav::texture_2d(output_ref))],
            ))],
        )
    });

    output
}

#[allow(dead_code)]
fn into_ycbcr(rg: &mut RenderGraph, mut input: Handle<Texture>) -> Handle<Texture> {
    let mut pass = rg.add_pass();
    let input_ref = pass.write(&mut input);

    pass.render(move |cb, resources| {
        let pipeline = resources.compute_pipeline("/assets/shaders/into_ycbcr.hlsl")?;
        cb.rg_dispatch_2d(
            &pipeline,
            input_ref.desc().dims(),
            &[RenderShaderArgument::new(pipeline.named_views(
                resources,
                &[],
                &[("input_tex", uav::texture_2d(input_ref))],
            ))],
        )
    });

    input
}
