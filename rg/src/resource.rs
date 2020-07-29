use render_core::handles::*;
use std::marker::PhantomData;

#[derive(Clone, Copy, Debug)]
pub struct TextureDesc {
    pub width: u32,
    pub height: u32,
}

impl TextureDesc {
    pub fn dims(self) -> [u32; 2] {
        [self.width, self.height]
    }
}

pub trait ResourceDescTraits: std::fmt::Debug {}
impl<DescType> ResourceDescTraits for DescType where DescType: std::fmt::Debug {}

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

#[derive(Debug)]
pub struct ResourceHandle<DescType>
where
    DescType: ResourceDescTraits,
{
    pub(crate) raw: GraphRawResourceHandle,
    pub desc: DescType,
}

impl<DescType> PartialEq for ResourceHandle<DescType>
where
    DescType: ResourceDescTraits,
{
    fn eq(&self, other: &Self) -> bool {
        self.raw == other.raw
    }
}

impl<DescType> Eq for ResourceHandle<DescType> where DescType: ResourceDescTraits {}

#[derive(Debug)]
pub struct RawResourceRef<DescType, AccessMode> {
    pub(crate) desc: DescType,
    pub(crate) handle: GraphRawResourceHandle,
    pub(crate) marker: PhantomData<(DescType, AccessMode)>,
}

impl<DescType, AccessMode> std::ops::Deref for RawResourceRef<DescType, AccessMode> {
    type Target = DescType;

    fn deref(&self) -> &Self::Target {
        &self.desc
    }
}

impl<DescType, AccessMode> Clone for RawResourceRef<DescType, AccessMode>
where
    DescType: Clone,
    AccessMode: Clone,
{
    fn clone(&self) -> Self {
        Self {
            desc: self.desc.clone(),
            handle: self.handle.clone(),
            marker: PhantomData,
        }
    }
}

impl<DescType, AccessMode> Copy for RawResourceRef<DescType, AccessMode>
where
    DescType: Copy,
    AccessMode: Copy,
{
}

impl<DescType, AccessMode> RawResourceRef<DescType, AccessMode>
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

pub trait CreateReference<AccessMode>: Sized + Clone {
    type RefType;

    fn create(r: RawResourceRef<Self, AccessMode>) -> Self::RefType;
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

macro_rules! def_resource_handles {
    ($handle_type:ident, $ref_type:ident, $desc_type:ident) => {
        #[derive(Debug, PartialEq, Eq)]
        pub struct $handle_type(pub(crate) ResourceHandle<$desc_type>);

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

        pub struct $ref_type<AccessMode>(pub(crate) RawResourceRef<$desc_type, AccessMode>);

        impl<AccessMode> $ref_type<AccessMode> {
            pub(crate) fn internal_clone(&self) -> Self {
                Self(self.0.internal_clone())
            }
        }

        impl<AccessMode> CreateReference<AccessMode> for $desc_type {
            type RefType = $ref_type<AccessMode>;

            fn create(r: RawResourceRef<Self, AccessMode>) -> Self::RefType {
                $ref_type(r)
            }
        }

        impl<AccessMode> std::ops::Deref for $ref_type<AccessMode> {
            type Target = RawResourceRef<$desc_type, AccessMode>;

            fn deref(&self) -> &Self::Target {
                &self.0
            }
        }

        impl<AccessMode> Clone for $ref_type<AccessMode>
        where
            AccessMode: Clone,
        {
            fn clone(&self) -> Self {
                Self(self.0.clone())
            }
        }
        impl<AccessMode> Copy for $ref_type<AccessMode> where AccessMode: Copy {}
    };
}

def_resource_handles! { TextureHandle, TextureRef, TextureDesc }
