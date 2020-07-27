#![allow(unused_imports)]

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
pub struct TextureDesc {
    pub width: u32,
    pub height: u32,
}

impl TextureDesc {
    pub fn dims(self) -> (u32, u32) {
        (self.width, self.height)
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
struct RawResourceHandle {
    id: u32,
    version: u32,
}

impl RawResourceHandle {
    fn next_version(self) -> Self {
        Self {
            id: self.id,
            version: self.version + 1,
        }
    }
}

pub trait ResourceDescTraits: std::fmt::Debug {}
impl<T> ResourceDescTraits for T where T: std::fmt::Debug {}

#[derive(Debug)]
pub struct ResourceHandle<T>
where
    T: ResourceDescTraits,
{
    raw: RawResourceHandle,
    pub desc: T,
}

impl<T> PartialEq for ResourceHandle<T>
where
    T: ResourceDescTraits,
{
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<T> Eq for ResourceHandle<T> where T: ResourceDescTraits {}

#[derive(Clone, Copy, Debug)]
enum GenericResourceDesc {
    Texture(TextureDesc),
}

#[derive(Debug)]
pub struct ResourceRef<DescType, GpuResType> {
    desc: DescType,
    handle: RawResourceHandle,
    marker: PhantomData<(DescType, GpuResType)>,
}

impl<DescType, GpuResType> std::ops::Deref for ResourceRef<DescType, GpuResType> {
    type Target = DescType;

    fn deref(&self) -> &Self::Target {
        &self.desc
    }
}

impl<DescType, GpuResType> Clone for ResourceRef<DescType, GpuResType>
where
    DescType: Clone,
    GpuResType: Clone,
{
    fn clone(&self) -> Self {
        Self {
            desc: self.desc.clone(),
            handle: self.handle.clone(),
            marker: PhantomData,
        }
    }
}

impl<DescType, GpuResType> Copy for ResourceRef<DescType, GpuResType>
where
    DescType: Copy,
    GpuResType: Copy,
{
}

impl<DescType, GpuResType> ResourceRef<DescType, GpuResType>
where
    DescType: Copy,
{
    fn internal_clone(&self) -> Self {
        Self {
            desc: self.desc,
            handle: self.handle,
            marker: PhantomData,
        }
    }
}

#[derive(Debug)]
pub struct PipelineDesc {
    pub shader: String,
}

struct ResourceCreateInfo {
    desc: GenericResourceDesc,
    create_pass_idx: usize,
}

pub enum GpuResource {
    Image(RenderResourceHandle),

    #[allow(dead_code)]
    Buffer(RenderResourceHandle),
}

pub struct RenderGraph {
    pub passes: Vec<RecordedPass>,
    resources: Vec<ResourceCreateInfo>,
}

impl RenderGraph {
    pub fn new() -> Self {
        Self {
            passes: Vec::new(),
            resources: Vec::new(),
        }
    }

    fn create_resource(&mut self, info: ResourceCreateInfo) -> RawResourceHandle {
        let res = RawResourceHandle {
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

pub struct RenderGraphExecutionParams<'device> {
    pub handles: TrackingResourceHandleAllocator,
    pub device: &'device dyn RenderDevice,
    pub shader_cache: Arc<RwLock<ShaderCache>>,
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

    pub fn execute<'device, 'cb, 'commands>(
        self,
        params: RenderGraphExecutionParams<'device>,
        cb: &'cb mut RenderCommandList<'commands>,
        // TODO: use exported/imported resources instead
        get_output_texture: TextureHandle,
    ) -> RenderGraphExecutionOutput {
        let resource_lifetimes = self.calculate_resource_lifetimes();

        println!(
            "Resources: {:#?}",
            self.resources
                .iter()
                .map(|info| info.desc)
                .zip(resource_lifetimes.iter())
                .collect::<Vec<_>>()
        );

        let handles = &params.handles;
        let device = params.device;

        let gpu_resources: Vec<GpuResource> = self
            .resources
            .iter()
            .map(|resource: &ResourceCreateInfo| match resource.desc {
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
    read: Vec<RawResourceHandle>,
    write: Vec<RawResourceHandle>,
    create: Vec<RawResourceHandle>,
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
            raw: self.rg.create_resource(ResourceCreateInfo {
                desc: GenericResourceDesc::Texture(desc),
                create_pass_idx: self.pass_idx,
            }),
            desc,
        });

        self.pass.as_mut().unwrap().create.push(handle.0.raw);

        let reference = ResourceRef {
            desc,
            handle: handle.0.raw,
            marker: PhantomData,
        };

        (handle, TextureRef(reference))
    }

    pub fn read<T>(
        &mut self,
        handle: &impl std::ops::Deref<Target = ResourceHandle<T>>,
    ) -> <T as ReferenceResource<GpuSrv>>::RefType
    where
        T: ReferenceResource<GpuSrv>,
        T: ResourceDescTraits,
    {
        self.pass.as_mut().unwrap().read.push(handle.raw);

        let reference = ResourceRef {
            desc: handle.desc,
            handle: handle.raw,
            marker: PhantomData,
        };

        <T as ReferenceResource<GpuSrv>>::create(reference)
    }

    pub fn write<T>(
        &mut self,
        handle: &mut impl std::ops::DerefMut<Target = ResourceHandle<T>>,
    ) -> <T as ReferenceResource<GpuUav>>::RefType
    where
        T: ReferenceResource<GpuUav>,
        T: ResourceDescTraits,
    {
        self.pass.as_mut().unwrap().write.push(handle.raw);

        let reference = ResourceRef {
            desc: handle.desc,
            handle: handle.raw.next_version(),
            marker: PhantomData,
        };

        <T as ReferenceResource<GpuUav>>::create(reference)
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
#[derive(Clone, Copy)]
pub struct GpuSrv(pub RenderResourceHandle);
pub struct GpuUav(pub RenderResourceHandle);

pub trait ToGpuResourceView {
    fn to_gpu_resource_view(gpu_res: &GpuResource) -> Self;
}

impl ToGpuResourceView for GpuSrv {
    fn to_gpu_resource_view(res: &GpuResource) -> Self {
        match res {
            GpuResource::Buffer(handle) => Self(*handle),
            GpuResource::Image(handle) => Self(*handle),
        }
    }
}

impl ToGpuResourceView for GpuUav {
    fn to_gpu_resource_view(res: &GpuResource) -> Self {
        match res {
            GpuResource::Buffer(handle) => Self(*handle),
            GpuResource::Image(handle) => Self(*handle),
        }
    }
}

#[derive(Hash, PartialEq, Eq)]
struct ShaderCacheKey {
    path: PathBuf,
    shader_type: RenderShaderType,
}

// TODO: figure out the ownership model -- should this release the resources?
pub struct ShaderCacheEntry {
    #[allow(dead_code)]
    shader_handle: RenderResourceHandle,
    pub pipeline_handle: RenderResourceHandle,
    pub srvs: Vec<String>,
    pub uavs: Vec<String>,
    pub group_size_x: u32,
    pub group_size_y: u32,
    pub group_size_z: u32,
}

#[derive(Default)]
pub struct ShaderCache {
    shaders: HashMap<ShaderCacheKey, Arc<ShaderCacheEntry>>,
}

impl ShaderCache {
    fn get_or_load(
        &mut self,
        params: &RenderGraphExecutionParams<'_>,
        shader_type: RenderShaderType,
        path: impl AsRef<Path>,
    ) -> Arc<ShaderCacheEntry> {
        let key = ShaderCacheKey {
            path: path.as_ref().to_owned(),
            shader_type,
        };

        self.shaders
            .entry(key)
            .or_insert_with(|| {
                let path = path.as_ref();
                let shader_data = std::fs::read(path).unwrap();

                let mut srvs = Vec::new();
                let mut uavs = Vec::new();

                match spirv_reflect::ShaderModule::load_u8_data(&shader_data) {
                    Ok(reflect_module) => {
                        let descriptor_sets =
                            reflect_module.enumerate_descriptor_sets(None).unwrap();
                        {
                            let set = &descriptor_sets[0];
                            for binding_index in 0..set.bindings.len() {
                                let binding = &set.bindings[binding_index];
                                assert_ne!(
                                    binding.resource_type,
                                    spirv_reflect::types::resource::ReflectResourceType::Undefined
                                );
                                match binding.resource_type {
							spirv_reflect::types::resource::ReflectResourceType::ShaderResourceView => {
                                srvs.push(binding.name.clone());
							},
							spirv_reflect::types::resource::ReflectResourceType::UnorderedAccessView => {
								uavs.push(binding.name.clone());
							},
							_ => {},
						}
                            }
                        }
                    }
                    Err(err) => panic!("failed to parse shader - {:?}", err),
                }

                let shader_handle = params
                    .handles
                    .allocate_persistent(RenderResourceType::Shader);
                params
                    .device
                    .create_shader(
                        shader_handle,
                        &RenderShaderDesc {
                            shader_type,
                            shader_data,
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
                                    srvs.len() as u32,
                                    uavs.len() as u32,
                                )],
                                &[],
                            ),
                        },
                        "gradients compute pipeline".into(),
                    )
                    .unwrap();

                Arc::new(ShaderCacheEntry {
                    shader_handle,
                    pipeline_handle,
                    srvs,
                    uavs,

                    // TODO
                    group_size_x: 8,
                    group_size_y: 8,
                    group_size_z: 1,
                })
            })
            .clone()
    }
}

pub struct ResourceRegistry<'exec_params, 'device> {
    pub execution_params: &'exec_params RenderGraphExecutionParams<'device>,
    resources: Vec<GpuResource>,
}

impl<'exec_params, 'device> ResourceRegistry<'exec_params, 'device> {
    pub fn get<T, GpuResType>(
        &self,
        resource: impl std::ops::Deref<Target = ResourceRef<T, GpuResType>>,
    ) -> GpuResType
    where
        GpuResType: ToGpuResourceView,
    {
        println!("ResourceRegistry::get: {:?}", resource.handle);
        <GpuResType as ToGpuResourceView>::to_gpu_resource_view(
            &self.resources[resource.handle.id as usize],
        )
    }

    pub fn shader(
        &self,
        shader_path: impl AsRef<str>,
        shader_type: RenderShaderType,
    ) -> Arc<ShaderCacheEntry> {
        self.execution_params
            .shader_cache
            .write()
            .unwrap()
            .get_or_load(self.execution_params, shader_type, shader_path.as_ref())
    }
}

pub trait ReferenceResource<GpuResType>: Sized + Copy {
    type RefType;

    fn create(r: ResourceRef<Self, GpuResType>) -> Self::RefType;
}

macro_rules! def_resource_handles {
    ($handle_type:ident, $ref_type:ident, $desc_type:ident) => {
        #[derive(Debug, PartialEq, Eq)]
        pub struct $handle_type(ResourceHandle<$desc_type>);

        impl std::ops::Deref for $handle_type {
            type Target = ResourceHandle<$desc_type>;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl std::ops::DerefMut for $handle_type {
            fn deref_mut(&mut self) -> &mut Self::Target {
                &mut self.0
            }
        }

        pub struct $ref_type<GpuResType>(ResourceRef<$desc_type, GpuResType>);

        impl<GpuResType> $ref_type<GpuResType> {
            pub(crate) fn internal_clone(&self) -> Self {
                Self(self.0.internal_clone())
            }
        }

        impl<GpuResType> ReferenceResource<GpuResType> for $desc_type {
            type RefType = $ref_type<GpuResType>;

            fn create(r: ResourceRef<Self, GpuResType>) -> Self::RefType {
                $ref_type(r)
            }
        }

        impl<GpuResType> std::ops::Deref for $ref_type<GpuResType> {
            type Target = ResourceRef<$desc_type, GpuResType>;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<GpuResType> Clone for $ref_type<GpuResType>
        where
            GpuResType: Clone,
        {
            fn clone(&self) -> Self {
                Self(self.0.clone())
            }
        }
        impl<GpuResType> Copy for $ref_type<GpuResType> where GpuResType: Copy {}
    };
}

def_resource_handles! { TextureHandle, TextureRef, TextureDesc }
