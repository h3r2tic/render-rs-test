#![allow(unused_imports)]

use crate::{
    pass_builder::PassBuilder, pipeline_cache::PipelineCache, resource::*,
    resource_registry::ResourceRegistry, shader_cache::*,
};

use render_core::{
    backend::*,
    device::*,
    encoder::RenderCommandList,
    handles::*,
    state::{build, RenderComputePipelineStateDesc},
    system::*,
    types::*,
};

use std::{
    collections::HashMap,
    marker::PhantomData,
    path::{Path, PathBuf},
    sync::{Arc, Mutex, RwLock},
};

#[derive(Clone, Copy, Debug)]
pub enum GraphResourceDesc {
    Texture(TextureDesc),
}

impl From<TextureDesc> for GraphResourceDesc {
    fn from(desc: TextureDesc) -> Self {
        Self::Texture(desc)
    }
}

pub(crate) struct GraphResourceCreateInfo {
    pub desc: GraphResourceDesc,
    pub create_pass_idx: usize,
}

pub struct RenderGraph {
    passes: Vec<RecordedPass>,
    resources: Vec<GraphResourceCreateInfo>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            resources: Vec::new(),
        }
    }

    pub(crate) fn create_raw_resource(
        &mut self,
        info: GraphResourceCreateInfo,
    ) -> GraphRawResourceHandle {
        let res = GraphRawResourceHandle {
            id: self.resources.len() as u32,
            version: 0,
        };

        self.resources.push(info);
        res
    }
}

pub trait ResourceHandleAllocator {
    fn allocate_transient(&self, kind: RenderResourceType) -> RenderResourceHandle;
    fn allocate_persistent(&self, kind: RenderResourceType) -> RenderResourceHandle;
}

#[derive(Default)]
pub struct TrackedResourceHandles {
    pub transient: Vec<RenderResourceHandle>,
    pub persistent: Vec<RenderResourceHandle>,
}

pub struct TrackingResourceHandleAllocator {
    handles: Arc<RwLock<RenderResourceHandleAllocator>>,
    tracked: RwLock<TrackedResourceHandles>,
}

impl TrackingResourceHandleAllocator {
    pub fn new(handles: Arc<RwLock<RenderResourceHandleAllocator>>) -> Self {
        Self {
            handles,
            tracked: Default::default(),
        }
    }

    pub fn into_allocated_resources(self) -> TrackedResourceHandles {
        self.tracked.into_inner().unwrap()
    }
}

impl ResourceHandleAllocator for TrackingResourceHandleAllocator {
    fn allocate_transient(&self, kind: RenderResourceType) -> RenderResourceHandle {
        let handle = self.handles.write().unwrap().allocate(kind);
        self.tracked.write().unwrap().transient.push(handle);
        handle
    }

    fn allocate_persistent(&self, kind: RenderResourceType) -> RenderResourceHandle {
        let handle = self.handles.write().unwrap().allocate(kind);
        self.tracked.write().unwrap().persistent.push(handle);
        handle
    }
}

#[derive(Debug)]
struct ResourceLifetime {
    first_access: usize,
    last_access: usize,
}

pub struct RenderGraphExecutionParams<'device, 'pipeline_cache, 'res_alloc> {
    pub device: &'device dyn RenderDevice,
    pub pipeline_cache: &'pipeline_cache PipelineCache,
    pub handles: &'res_alloc dyn ResourceHandleAllocator,
}

#[derive(Default)]
pub struct RenderGraphExecutionOutput {
    pub output_texture: RenderResourceHandle,
}

impl RenderGraph {
    pub fn add_pass<'s>(&'s mut self) -> PassBuilder<'s> {
        let pass_idx = self.passes.len();

        PassBuilder {
            rg: self,
            pass_idx,
            pass: Some(Default::default()),
        }
    }

    fn calculate_resource_lifetimes(&self) -> Vec<ResourceLifetime> {
        let mut resource_lifetimes: Vec<ResourceLifetime> = self
            .resources
            .iter()
            .map(|res| ResourceLifetime {
                first_access: res.create_pass_idx,
                last_access: res.create_pass_idx,
            })
            .collect();

        for (pass_idx, pass) in self.passes.iter().enumerate() {
            for res_access in pass.read.iter().chain(pass.write.iter()) {
                let res = &mut resource_lifetimes[res_access.handle.id as usize];
                res.last_access = res.last_access.max(pass_idx);
            }
        }

        resource_lifetimes
    }

    pub fn execute<'device, 'pipeline_cache, 'cb, 'commands, 'res_alloc>(
        self,
        params: RenderGraphExecutionParams<'device, 'pipeline_cache, 'res_alloc>,
        cb: &'cb mut RenderCommandList<'commands>,
        // TODO: use exported/imported resources instead
        get_output_texture: Handle<Texture>,
    ) -> anyhow::Result<RenderGraphExecutionOutput> {
        let resource_lifetimes = self.calculate_resource_lifetimes();

        /* println!(
            "Resources: {:#?}",
            self.resources
                .iter()
                .map(|info| info.desc)
                .zip(resource_lifetimes.iter())
                .collect::<Vec<_>>()
        ); */

        let handles = &params.handles;
        let device = params.device;

        let gpu_resources: Vec<RenderResourceHandle> = self
            .resources
            .iter()
            .map(|resource: &GraphResourceCreateInfo| match resource.desc {
                GraphResourceDesc::Texture(desc) => {
                    let handle = handles.allocate_transient(RenderResourceType::Texture);
                    device
                        .create_texture(
                            handle,
                            &RenderTextureDesc {
                                texture_type: RenderTextureType::Tex2d,
                                bind_flags: RenderBindFlags::UNORDERED_ACCESS
                                    | RenderBindFlags::SHADER_RESOURCE
                                    | RenderBindFlags::RENDER_TARGET,
                                format: RenderFormat::R32g32b32a32Float,
                                width: desc.width,
                                height: desc.height,
                                depth: 1,
                                levels: 1,
                                elements: 1,
                            },
                            None,
                            "rg texture".into(),
                        )
                        .unwrap();

                    handle
                }
            })
            .collect();

        let resource_registry = ResourceRegistry {
            execution_params: &params,
            resources: gpu_resources,
        };

        for pass in self.passes.into_iter() {
            // TODO: partial barrier cmds (destination access modes)
            (pass.render_fn.unwrap())(cb, &resource_registry)?;
        }

        // TODO: perform transitions
        //todo!("run the recorded commands");

        let output_texture = resource_registry.resources[get_output_texture.raw.id as usize];
        assert!(output_texture.get_type() == RenderResourceType::Texture);

        Ok(RenderGraphExecutionOutput { output_texture })
    }

    pub(crate) fn record_pass(&mut self, pass: RecordedPass) {
        self.passes.push(pass);
    }
}

type DynRenderFn = dyn FnOnce(&mut RenderCommandList<'_>, &ResourceRegistry) -> anyhow::Result<()>;

pub(crate) struct PassResourceRef {
    pub handle: GraphRawResourceHandle,
    pub access_mode: RenderResourceStates,
}

#[derive(Default)]
pub(crate) struct RecordedPass {
    pub read: Vec<PassResourceRef>,
    pub write: Vec<PassResourceRef>,
    pub render_fn: Option<Box<DynRenderFn>>,
}
