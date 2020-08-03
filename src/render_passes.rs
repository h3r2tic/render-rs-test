use render_core::{
    state::{RenderDrawState, RenderScissorRect, RenderViewportRect},
    types::{RenderDrawPacket, RenderResourceStates},
};
use rg::{command_ext::*, resource_view::*, *};

pub fn render_frame_rg() -> (RenderGraph, Handle<Texture>) {
    let mut rg = RenderGraph::new();

    let tex = /*synth_gradients*/raster_triangle(
        &mut rg,
        TextureDesc {
            width: 1280,
            height: 720,
        },
    );

    let tex = blur(&mut rg, &tex);
    //let tex = into_ycbcr(&mut rg, tex);

    (rg, tex)
}

fn raster_triangle(rg: &mut RenderGraph, desc: TextureDesc) -> Handle<Texture> {
    let mut pass = rg.add_pass();
    let mut output = pass.create(&desc);
    let output_ref = pass.raster(&mut output);

    pass.render(move |cb, registry| {
        let pipeline = registry.raster_pipeline(RasterPipelineDesc {
            vertex_shader: "/assets/shaders/raster_simple_vs.hlsl".into(),
            pixel_shader: "/assets/shaders/raster_simple_ps.hlsl".into(),
        })?;

        let rt = registry.get(output_ref);

        let rt_handle = rt.0;
        cb.transitions(&[(rt_handle, RenderResourceStates::RENDER_TARGET)])?;
        cb.begin_render_pass(registry.render_pass(&[rt])?)?;

        // TODO
        let width = 1280;
        let height = 720;

        cb.draw(
            pipeline.handle,
            &[RenderShaderArgument {
                shader_views: None,
                constant_buffer: None,
                constant_buffer_offset: 0,
            }],
            None,
            &RenderDrawState {
                viewport: Some(RenderViewportRect {
                    x: 0.0,
                    y: 0.0,
                    width: width as f32,
                    height: height as f32,
                    min_z: 0.0,
                    max_z: 1.0,
                }),
                scissor: Some(RenderScissorRect {
                    x: 0,
                    y: 0,
                    width,
                    height,
                }),
                stencil_ref: 0,
            },
            &RenderDrawPacket {
                index_offset: 0,
                vertex_offset: 0,
                vertex_count: 3,
                first_instance: 0,
                instance_count: 1,
            },
        )?;
        cb.end_render_pass()?;
        cb.transitions(&[(rt_handle, RenderResourceStates::NON_PIXEL_SHADER_RESOURCE)])?;

        Ok(())
    });

    output
}

/*fn synth_gradients(rg: &mut RenderGraph, desc: TextureDesc) -> Handle<Texture> {
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
}*/

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
