use render_core::{state::RenderComputePipelineStateDesc, types::*};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
    sync::{Arc, RwLock},
};
use turbosloth::*;

#[derive(Hash, PartialEq, Eq)]
struct ShaderCacheKey {
    path: PathBuf,
    shader_type: RenderShaderType,
}

struct TurboslothShaderCacheEntry {
    lazy_handle: Lazy<crate::shader_compiler::ComputeShader>,
    entry: Arc<rg::shader_cache::ShaderCacheEntry>,
}

pub struct TurboslothShaderCache {
    shaders: RwLock<HashMap<ShaderCacheKey, TurboslothShaderCacheEntry>>,
    lazy_cache: Arc<LazyCache>,
}

impl TurboslothShaderCache {
    pub fn new(lazy_cache: Arc<LazyCache>) -> Self {
        Self {
            shaders: Default::default(),
            lazy_cache,
        }
    }
}

impl rg::shader_cache::ShaderCache for TurboslothShaderCache {
    fn get_or_load(
        &self,
        params: &rg::RenderGraphExecutionParams<'_, '_>,
        shader_type: RenderShaderType,
        path: &Path,
    ) -> Arc<rg::shader_cache::ShaderCacheEntry> {
        let key = ShaderCacheKey {
            path: path.to_owned(),
            shader_type,
        };

        let mut shaders = self.shaders.write().unwrap();

        // If the shader's lazy handle is stale, force re-compilation
        if let Some(entry) = shaders.get(&key) {
            if !entry.lazy_handle.is_up_to_date() {
                shaders.remove(&key);
            }
        }

        let lazy_cache = &self.lazy_cache;
        shaders
            .entry(key)
            .or_insert_with(|| {
                let path = path;

                let lazy_shader = crate::shader_compiler::CompileComputeShader {
                    path: path.to_owned(),
                }
                .into_lazy();

                let shader_data = smol::block_on(lazy_shader.eval(lazy_cache)).unwrap();

                let shader_handle = params
                    .handles
                    .allocate_persistent(RenderResourceType::Shader);
                params
                    .device
                    .create_shader(
                        shader_handle,
                        &RenderShaderDesc {
                            shader_type,
                            shader_data: shader_data.spirv.clone(),
                        },
                        "compute shader".into(),
                    )
                    .unwrap();

                let pipeline_handle = params
                    .handles
                    .allocate_persistent(RenderResourceType::ComputePipelineState);

                params
                    .device
                    .create_compute_pipeline_state(
                        pipeline_handle,
                        &RenderComputePipelineStateDesc {
                            shader: shader_handle,
                            shader_signature: RenderShaderSignatureDesc::new(
                                &[RenderShaderParameter::new(
                                    shader_data.srvs.len() as u32,
                                    shader_data.uavs.len() as u32,
                                )],
                                &[],
                            ),
                        },
                        "gradients compute pipeline".into(),
                    )
                    .unwrap();

                TurboslothShaderCacheEntry {
                    lazy_handle: lazy_shader,
                    entry: Arc::new(rg::shader_cache::ShaderCacheEntry {
                        shader_handle,
                        pipeline_handle,
                        srvs: shader_data.srvs.clone(),
                        uavs: shader_data.uavs.clone(),
                        group_size: shader_data.group_size,
                    }),
                }
            })
            .entry
            .clone()
    }
}
