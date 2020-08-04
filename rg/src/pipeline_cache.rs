use crate::{
    pipeline::{ComputePipeline, RasterPipeline},
    shader_cache::{ShaderCache, ShaderCacheEntry},
    RasterPipelineDesc, RenderGraphExecutionParams, RenderTarget,
};
use render_core::{
    constants::{MAX_RENDER_TARGET_COUNT, MAX_SHADER_TYPE},
    handles::RenderResourceHandle,
    state::*,
    types::{
        RenderFormat, RenderPrimitiveType, RenderResourceType, RenderShaderParameter,
        RenderShaderSignatureDesc, RenderShaderType,
    },
};
use std::{
    collections::HashMap,
    path::Path,
    sync::{Arc, RwLock},
};

#[derive(Clone, Copy, Hash, Eq, PartialEq)]
struct RasterPipelineKey {
    vertex_shader: RenderResourceHandle,
    pixel_shader: RenderResourceHandle,
    render_target_formats: [RenderFormat; MAX_RENDER_TARGET_COUNT],
    render_state_hash: u64,
}

impl RasterPipelineKey {
    fn shaders(&self) -> Vec<RenderResourceHandle> {
        vec![self.vertex_shader, self.pixel_shader]
    }
}

#[derive(Clone, Copy, Hash, Eq, PartialEq, Default)]
struct RasterPipelineId(usize);

type CsToPipeline = HashMap<RenderResourceHandle, Arc<ComputePipeline>>;
type RasterToPipelines = HashMap<RenderResourceHandle, Vec<RasterPipelineId>>;

struct RasterPipelineEntry {
    pipeline: Arc<RasterPipeline>,
    key: RasterPipelineKey,
}

#[derive(Default)]
pub struct Pipelines {
    compute_shader_to_pipeline: CsToPipeline,

    raster_pipeline_key_to_pipeline_id: HashMap<RasterPipelineKey, RasterPipelineId>,

    // Map shaders to all pipelines which use them, so we can evict the pipelines
    // when shaders become invalidated
    raster_shader_to_pipelines: RasterToPipelines,

    // Actual storage of raster pipelines.
    // TODO: use small integers in conjunction with a Vec instead of the HashMap.
    raster_pipelines: HashMap<RasterPipelineId, RasterPipelineEntry>,

    // Next unused RasterPipelineId.
    next_raster_pipeline_id: RasterPipelineId,
}

pub struct PipelineCache {
    pub shader_cache: Box<dyn ShaderCache>,
    pub pipelines: Arc<RwLock<Pipelines>>,
}

impl PipelineCache {
    pub fn new(shader_cache: impl ShaderCache + 'static) -> Self {
        Self {
            shader_cache: Box::new(shader_cache),
            pipelines: Default::default(),
        }
    }

    pub fn get_or_load_compute(
        &self,
        params: &RenderGraphExecutionParams<'_, '_, '_>,
        path: &Path,
    ) -> anyhow::Result<Arc<ComputePipeline>> {
        let shader_cache_entry =
            self.shader_cache
                .get_or_load(params, RenderShaderType::Compute, path);

        let mut pipelines = self.pipelines.write().unwrap();
        let compute_pipes = &mut pipelines.compute_shader_to_pipeline;

        if let Some(retired) = shader_cache_entry.retired {
            compute_pipes.remove(&retired.shader_handle());
        }

        let shader = shader_cache_entry.entry?;

        Ok(match compute_pipes.entry(shader.shader_handle()) {
            std::collections::hash_map::Entry::Occupied(occupied) => occupied.get().clone(),
            std::collections::hash_map::Entry::Vacant(vacant) => {
                let shader = match &*shader {
                    ShaderCacheEntry::Compute(shader) => shader,
                    ShaderCacheEntry::Raster(..) => unreachable!(),
                };

                let shader_handle = shader.shader_handle;

                let pipeline_handle = params
                    .handles
                    .allocate_persistent(RenderResourceType::ComputePipelineState);

                params.device.create_compute_pipeline_state(
                    pipeline_handle,
                    &RenderComputePipelineStateDesc {
                        shader: shader_handle,
                        shader_signature: RenderShaderSignatureDesc::new(
                            &[RenderShaderParameter::new(
                                shader.srvs.len() as u32,
                                shader.uavs.len() as u32,
                            )],
                            &[],
                        ),
                    },
                    "compute pipeline".into(),
                )?;

                let pipeline_entry = Arc::new(ComputePipeline {
                    handle: pipeline_handle,
                    group_size: shader.group_size,
                    srvs: shader.srvs.clone(),
                    uavs: shader.uavs.clone(),
                });

                vacant.insert(pipeline_entry.clone());
                pipeline_entry
            }
        })
    }

    pub fn get_or_load_raster(
        &self,
        params: &RenderGraphExecutionParams<'_, '_, '_>,
        desc: RasterPipelineDesc,
        render_target: &RenderTarget,
    ) -> anyhow::Result<Arc<RasterPipeline>> {
        let vertex_shader =
            self.shader_cache
                .get_or_load(params, RenderShaderType::Vertex, &desc.vertex_shader);

        let pixel_shader =
            self.shader_cache
                .get_or_load(params, RenderShaderType::Pixel, &desc.pixel_shader);

        let mut pipelines = self.pipelines.write().unwrap();

        // Invalidate any stale pipelines
        {
            // Find all shaders that need to be retired
            let retired_shaders = vertex_shader
                .retired
                .into_iter()
                .chain(pixel_shader.retired.into_iter());

            // Find all pipelines that use the retired shdaers
            let mut pipelines_to_retire = Vec::new();
            for retired_shader in retired_shaders
                .into_iter()
                .map(|shader| shader.shader_handle())
            {
                if let Some(pipelines) =
                    pipelines.raster_shader_to_pipelines.remove(&retired_shader)
                {
                    for pipeline in pipelines.iter() {
                        pipelines_to_retire.push(*pipeline);
                    }
                }
            }

            let mut shaders_using_removed_pipelines = Vec::new();

            // Remove all of the pipelines that we found to be stale, and note which shaders were using them
            for pipeline in pipelines_to_retire.iter() {
                if let Some(pipeline) = pipelines.raster_pipelines.remove(pipeline) {
                    pipelines
                        .raster_pipeline_key_to_pipeline_id
                        .remove(&pipeline.key);

                    shaders_using_removed_pipelines.append(&mut pipeline.key.shaders());
                }
            }

            // Remove entries of the now-gone pipelines from any shaders that were using the stale pipelines.
            // Invalidating a pipeline does not mean invalidating all shaders using that pipeline, since
            // for example a vertex shader could be shared across all pipelines -- when a pixel shader becomes
            // stale, all pipelines that pixel shader uses will go away, and links from the vertex shader
            // need to be cleaned up too. This ensures we don't leak dangling pipeline IDs.
            for shader in shaders_using_removed_pipelines {
                pipelines
                    .raster_shader_to_pipelines
                    .get_mut(&shader)
                    .unwrap()
                    .retain(|item| !pipelines_to_retire.contains(item));
            }
        }

        let vertex_shader = vertex_shader.entry?.shader_handle();
        let pixel_shader = pixel_shader.entry?.shader_handle();

        let render_state_blob = bincode::serialize(&desc.render_state).unwrap();
        let render_state_hash = wyhash::wyhash(&render_state_blob, 0);

        let mut render_target_count = 0;
        let mut render_target_formats = [RenderFormat::Unknown; MAX_RENDER_TARGET_COUNT];

        for (i, color) in render_target.color.iter().enumerate() {
            if let Some(color) = color {
                render_target_formats[i] = color.texture.desc().format;
                render_target_count += 1;
            }
        }

        let pipeline_key = RasterPipelineKey {
            vertex_shader,
            pixel_shader,
            render_state_hash,
            render_target_formats,
        };

        if let Some(key) = pipelines
            .raster_pipeline_key_to_pipeline_id
            .get(&pipeline_key)
        {
            return Ok(pipelines.raster_pipelines[&key].pipeline.clone());
        }

        println!("Creating a new raster pipeline");

        let pipeline_handle = params
            .handles
            .allocate_persistent(RenderResourceType::GraphicsPipelineState);

        let mut shaders: [RenderResourceHandle; MAX_SHADER_TYPE] = Default::default();
        shaders[RenderShaderType::Vertex as usize] = vertex_shader;
        shaders[RenderShaderType::Pixel as usize] = pixel_shader;

        params.device.create_graphics_pipeline_state(
            pipeline_handle,
            &RenderGraphicsPipelineStateDesc {
                shaders,
                shader_signature: RenderShaderSignatureDesc::new(
                    &[RenderShaderParameter::new(
                        0, //shader.srvs.len() as u32,
                        0, // shader.uavs.len() as u32,
                    )],
                    &[],
                ),
                render_state: desc.render_state,
                vertex_element_count: 0,
                vertex_elements: Default::default(),
                vertex_buffer_strides: Default::default(),
                primitive_type: RenderPrimitiveType::TriangleList, // TODO
                render_target_count,
                render_target_write_masks: Default::default(),
                render_target_formats,
                //depth_stencil_format: RenderFormat::D32Float, // TODO
                depth_stencil_format: RenderFormat::Unknown,
            },
            "raster pipeline".into(),
        )?;

        let pipeline = Arc::new(RasterPipeline {
            handle: pipeline_handle,
        });

        let entry = RasterPipelineEntry {
            pipeline,
            key: pipeline_key,
        };

        let id = pipelines.next_raster_pipeline_id;
        pipelines.next_raster_pipeline_id.0 += 1;

        let res = entry.pipeline.clone();
        pipelines
            .raster_pipeline_key_to_pipeline_id
            .insert(entry.key, id);
        pipelines.raster_pipelines.insert(id, entry);

        Ok(res)
    }
}
