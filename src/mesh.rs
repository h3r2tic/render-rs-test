use render_core::handles::RenderResourceHandle;

pub struct TriangleMesh {
    pub positions: Vec<[f32; 3]>,
}
pub struct GpuTriangleMesh {
    pub vertex_count: u32,
    pub vertex_buffer: RenderResourceHandle,
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct RasterGpuVertex {
    pub pos: [f32; 3],
    pub normal: u32,
}

pub fn pack_unit_direction_11_10_11(x: f32, y: f32, z: f32) -> u32 {
    let x = ((x.max(-1.0).min(1.0) * 0.5 + 0.5) * ((1u32 << 11u32) - 1u32) as f32) as u32;
    let y = ((y.max(-1.0).min(1.0) * 0.5 + 0.5) * ((1u32 << 10u32) - 1u32) as f32) as u32;
    let z = ((z.max(-1.0).min(1.0) * 0.5 + 0.5) * ((1u32 << 11u32) - 1u32) as f32) as u32;

    (z << 21) | (y << 11) | x
}
