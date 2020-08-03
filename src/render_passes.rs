use render_core::{
    state::RenderState,
    types::{RenderDrawPacket, RenderFormat, RenderTargetInfo},
};
use rg::{command_ext::*, resource_view::*, *};

pub fn render_frame_rg() -> (RenderGraph, Handle<Texture>) {
    let mut rg = RenderGraph::new();

    let mut tex = synth_gradients(
        &mut rg,
        TextureDesc {
            width: 1280,
            height: 720,
            format: RenderFormat::R16g16b16a16Float,
        },
    );
    raster_triangle(&mut rg, &mut tex);

    let tex = blur(&mut rg, &tex);
    let tex = into_ycbcr(&mut rg, tex);

    (rg, tex)
}

fn raster_triangle(rg: &mut RenderGraph, output: &mut Handle<Texture>) {
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
            &[RenderShaderArgument::default()],
            None,
            &render_target.to_draw_state(),
            &RenderDrawPacket {
                vertex_count: 3,
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
