#![allow(unused_imports)]

use crate::{resource::*, shader_cache::*};

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
type Result<T> = std::result::Result<T, failure::Error>;

#[derive(Clone, Copy, Debug)]
enum GenericResourceDesc {
    Texture(TextureDesc),
}

struct GraphResourceCreateInfo {
    desc: GenericResourceDesc,
    create_pass_idx: usize,
}

pub struct RenderGraph {
    pub passes: Vec<RecordedPass>,
    resources: Vec<GraphResourceCreateInfo>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            resources: Vec::new(),
        }
    }

    fn create_raw_resource(&mut self, info: GraphResourceCreateInfo) -> GraphRawResourceHandle {
        let res = GraphRawResourceHandle {
            id: self.resources.len() as u32,
            version: 0,
        };

        self.resources.push(info);
        res
    }
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

    pub fn into_tracked(self) -> TrackedResourceHandles {
        self.tracked.into_inner().unwrap()
    }

    pub fn allocate_transient(&self, kind: RenderResourceType) -> RenderResourceHandle {
        let handle = self.handles.write().unwrap().allocate(kind);
        self.tracked.write().unwrap().transient.push(handle);
        handle
    }

    pub fn allocate_persistent(&self, kind: RenderResourceType) -> RenderResourceHandle {
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

pub struct RenderGraphExecutionParams<'device, 'shader_cache> {
    pub handles: TrackingResourceHandleAllocator,
    pub device: &'device dyn RenderDevice,
    pub shader_cache: &'shader_cache dyn ShaderCache,
}

#[derive(Default)]
pub struct RenderGraphExecutionOutput {
    pub allocated_resources: TrackedResourceHandles,
    pub output_texture: RenderResourceHandle,
}

impl RenderGraph {
    pub fn begin_pass<'s>(&'s mut self) -> RenderGraphContext<'s> {
        let pass_idx = self.passes.len();

        RenderGraphContext {
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
                let res = &mut resource_lifetimes[res_access.id as usize];
                res.last_access = res.last_access.max(pass_idx);
            }
        }

        resource_lifetimes
    }

    pub fn execute<'device, 'shader_cache, 'cb, 'commands>(
        self,
        params: RenderGraphExecutionParams<'device, 'shader_cache>,
        cb: &'cb mut RenderCommandList<'commands>,
        // TODO: use exported/imported resources instead
        get_output_texture: TextureHandle,
    ) -> RenderGraphExecutionOutput {
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

        let gpu_resources: Vec<GpuResource> = self
            .resources
            .iter()
            .map(|resource: &GraphResourceCreateInfo| match resource.desc {
                GenericResourceDesc::Texture(desc) => {
                    let handle = handles.allocate_transient(RenderResourceType::Texture);
                    device
                        .create_texture(
                            handle,
                            &RenderTextureDesc {
                                texture_type: RenderTextureType::Tex2d,
                                bind_flags: RenderBindFlags::UNORDERED_ACCESS
                                    | RenderBindFlags::SHADER_RESOURCE,
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

                    GpuResource::Image(handle)
                }
            })
            .collect();

        let resource_registry = ResourceRegistry {
            execution_params: &params,
            resources: gpu_resources,
        };

        for pass in self.passes.into_iter() {
            // TODO: partial barrier cmds (destination access modes)
            (pass.render_fn.unwrap())(cb, &resource_registry).expect("render pass");
        }

        // TODO: perform transitions
        //todo!("run the recorded commands");

        let output_texture = match resource_registry.resources[get_output_texture.0.raw.id as usize]
        {
            GpuResource::Image(tex) => tex,
            GpuResource::Buffer(_) => unimplemented!(),
        };

        RenderGraphExecutionOutput {
            allocated_resources: params.handles.into_tracked(),
            output_texture,
        }
    }

    fn record_pass(&mut self, pass: RecordedPass) {
        self.passes.push(pass);
    }
}

type DynRenderFn = dyn FnOnce(&mut RenderCommandList<'_>, &ResourceRegistry) -> Result<()>;

#[derive(Default)]
pub struct RecordedPass {
    read: Vec<GraphRawResourceHandle>,
    write: Vec<GraphRawResourceHandle>,
    create: Vec<GraphRawResourceHandle>,
    render_fn: Option<Box<DynRenderFn>>,
}

pub struct RenderGraphContext<'rg> {
    rg: &'rg mut RenderGraph,
    pass_idx: usize,
    pass: Option<RecordedPass>,
}

impl<'s> Drop for RenderGraphContext<'s> {
    fn drop(&mut self) {
        self.rg.record_pass(self.pass.take().unwrap())
    }
}

impl<'rg> RenderGraphContext<'rg> {
    pub fn create(&mut self, desc: TextureDesc) -> (TextureHandle, TextureRef<GpuUav>) {
        let handle: TextureHandle = TextureHandle(ResourceHandle {
            raw: self.rg.create_raw_resource(GraphResourceCreateInfo {
                desc: GenericResourceDesc::Texture(desc),
                create_pass_idx: self.pass_idx,
            }),
            desc,
        });

        self.pass.as_mut().unwrap().create.push(handle.0.raw);

        let reference = RawResourceRef {
            desc,
            handle: handle.0.raw,
            marker: PhantomData,
        };

        (handle, TextureRef(reference))
    }

    pub fn read<DescType>(
        &mut self,
        handle: &impl std::ops::Deref<Target = ResourceHandle<DescType>>,
    ) -> <DescType as CreateReference<GpuSrv>>::RefType
    where
        DescType: CreateReference<GpuSrv>,
        DescType: ResourceDescTraits,
    {
        self.pass.as_mut().unwrap().read.push(handle.raw);

        let reference = RawResourceRef {
            desc: handle.desc.clone(),
            handle: handle.raw,
            marker: PhantomData,
        };

        <DescType as CreateReference<GpuSrv>>::create(reference)
    }

    pub fn write<DescType>(
        &mut self,
        handle: &mut impl std::ops::DerefMut<Target = ResourceHandle<DescType>>,
    ) -> <DescType as CreateReference<GpuUav>>::RefType
    where
        DescType: CreateReference<GpuUav>,
        DescType: ResourceDescTraits,
    {
        self.pass.as_mut().unwrap().write.push(handle.raw);

        let reference = RawResourceRef {
            desc: handle.desc.clone(),
            handle: handle.raw.next_version(),
            marker: PhantomData,
        };

        <DescType as CreateReference<GpuUav>>::create(reference)
    }

    pub fn render(
        &mut self,
        render: impl FnOnce(&mut RenderCommandList<'_>, &ResourceRegistry) -> Result<()> + 'static,
    ) {
        let prev = self
            .pass
            .as_mut()
            .unwrap()
            .render_fn
            .replace(Box::new(render));

        assert!(prev.is_none());
    }
}

// Descriptor binding
pub struct ResourceRegistry<'exec_params, 'device, 'shader_cache> {
    pub execution_params: &'exec_params RenderGraphExecutionParams<'device, 'shader_cache>,
    resources: Vec<GpuResource>,
}

impl<'exec_params, 'device, 'shader_cache> ResourceRegistry<'exec_params, 'device, 'shader_cache> {
    pub fn get<T, GpuResType>(
        &self,
        resource: impl std::ops::Deref<Target = RawResourceRef<T, GpuResType>>,
    ) -> GpuResType
    where
        GpuResType: ToGpuResourceView,
    {
        // println!("ResourceRegistry::get: {:?}", resource.handle);
        <GpuResType as ToGpuResourceView>::to_gpu_resource_view(
            &self.resources[resource.handle.id as usize],
        )
    }

    pub fn shader(
        &self,
        shader_path: impl AsRef<Path>,
        shader_type: RenderShaderType,
    ) -> Arc<ShaderCacheEntry> {
        self.execution_params.shader_cache.get_or_load(
            self.execution_params,
            shader_type,
            shader_path.as_ref(),
        )
    }
}
