use rg::{command_ext::*, resource_view::*};

pub fn render_frame_rg() -> (rg::RenderGraph, rg::TextureHandle) {
    let mut rg = rg::RenderGraph::new();

    let tex = synth_gradients(
        &mut rg,
        rg::TextureDesc {
            width: 1280,
            height: 720,
        },
    );

    let tex = blur(&mut rg, &tex);
    let tex = into_ycbcr(&mut rg, tex);

    (rg, tex)
}

fn synth_gradients(rg: &mut rg::RenderGraph, desc: rg::TextureDesc) -> rg::TextureHandle {
    let mut pass = rg.begin_pass();
    let (output, output_ref) = pass.create(desc);

    pass.render(move |cb, registry| {
        let shader = registry.shader("/assets/shaders/gradients.hlsl", RenderShaderType::Compute);
        cb.rg_dispatch_2d(
            &shader,
            output_ref.dims(),
            &[RenderShaderArgument {
                shader_views: Some(shader.named_views(
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

fn blur(rg: &mut rg::RenderGraph, input: &rg::TextureHandle) -> rg::TextureHandle {
    let mut pass = rg.begin_pass();
    let input_ref = pass.read(input);
    let (output, output_ref) = pass.create(input.desc);

    pass.render(move |cb, registry| {
        let shader = registry.shader("/assets/shaders/blur.hlsl", RenderShaderType::Compute);
        cb.rg_dispatch_2d(
            &shader,
            input_ref.dims(),
            &[RenderShaderArgument {
                shader_views: Some(shader.named_views(
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

fn into_ycbcr(rg: &mut rg::RenderGraph, mut input: rg::TextureHandle) -> rg::TextureHandle {
    let mut pass = rg.begin_pass();
    let input_ref = pass.write(&mut input);

    pass.render(move |cb, registry| {
        let shader = registry.shader("/assets/shaders/into_ycbcr.hlsl", RenderShaderType::Compute);
        cb.rg_dispatch_2d(
            &shader,
            input_ref.dims(),
            &[RenderShaderArgument {
                shader_views: Some(shader.named_views(
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
