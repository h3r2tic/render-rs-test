use rg::{command_ext::*, resource_view::*, *};

pub fn render_frame_rg() -> (RenderGraph, Handle<Texture>) {
    let mut rg = RenderGraph::new();

    let tex = synth_gradients(
        &mut rg,
        TextureDesc {
            width: 1280,
            height: 720,
        },
    );

    let tex = blur(&mut rg, &tex);
    let tex = into_ycbcr(&mut rg, tex);

    (rg, tex)
}

fn synth_gradients(rg: &mut RenderGraph, desc: TextureDesc) -> Handle<Texture> {
    let mut pass = rg.add_pass();
    let (output, output_ref) = pass.create(&desc);

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
    let (output, output_ref) = pass.create(input.desc());

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
