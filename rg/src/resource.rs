use render_core::handles::*;
use std::marker::PhantomData;

#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
pub struct Texture;

pub trait Resource {
    type Desc: ResourceDesc;
}

impl Resource for Texture {
    type Desc = TextureDesc;
}

pub trait ResourceDesc: Clone + std::fmt::Debug + Into<crate::graph::GraphResourceDesc> {
    type Resource: Resource;
}

#[derive(Clone, Copy, Debug)]
pub struct TextureDesc {
    pub width: u32,
    pub height: u32,
}

impl ResourceDesc for TextureDesc {
    type Resource = Texture;
}

impl TextureDesc {
    pub fn dims(self) -> [u32; 2] {
        [self.width, self.height]
    }
}

#[derive(Clone, Copy, Eq, PartialEq, Debug)]
pub(crate) struct GraphRawResourceHandle {
    pub(crate) id: u32,
    pub(crate) version: u32,
}

impl GraphRawResourceHandle {
    pub(crate) fn next_version(self) -> Self {
        Self {
            id: self.id,
            version: self.version + 1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Handle<ResType: Resource> {
    pub(crate) raw: GraphRawResourceHandle,
    pub(crate) desc: <ResType as Resource>::Desc,
    pub(crate) marker: PhantomData<ResType>,
}

impl<ResType: Resource> PartialEq for Handle<ResType> {
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<ResType: Resource> Handle<ResType> {
    pub fn desc(&self) -> &<ResType as Resource>::Desc {
        &self.desc
    }
}

impl<ResType: Resource> Eq for Handle<ResType> {}

#[derive(Debug)]
pub struct Ref<ResType: Resource, AccessMode> {
    pub(crate) handle: GraphRawResourceHandle,
    pub(crate) desc: <ResType as Resource>::Desc,
    pub(crate) marker: PhantomData<(ResType, AccessMode)>,
}

impl<ResType: Resource, AccessMode> Ref<ResType, AccessMode> {
    pub fn desc(&self) -> &<ResType as Resource>::Desc {
        &self.desc
    }
}

impl<ResType: Resource, AccessMode> Clone for Ref<ResType, AccessMode>
where
    <ResType as Resource>::Desc: Clone,
    AccessMode: Clone,
{
    fn clone(&self) -> Self {
        Self {
            handle: self.handle.clone(),
            desc: self.desc.clone(),
            marker: PhantomData,
        }
    }
}

impl<ResType: Resource, AccessMode> Copy for Ref<ResType, AccessMode>
where
    <ResType as Resource>::Desc: Copy,
    AccessMode: Copy,
{
}

impl<ResType: Resource, AccessMode> Ref<ResType, AccessMode>
where
    <ResType as Resource>::Desc: Copy,
{
    pub(crate) fn internal_clone(&self) -> Ref<ResType, AccessMode> {
        Ref {
            handle: self.handle,
            desc: self.desc,
            marker: PhantomData,
        }
    }
}

pub enum GpuResource {
    Image(RenderResourceHandle),

    #[allow(dead_code)]
    Buffer(RenderResourceHandle),
}

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
