use crate::{
    graph::{GenericResourceDesc, GraphResourceCreateInfo, RecordedPass, RenderGraph},
    resource::*,
    resource_registry::ResourceRegistry,
};

use render_core::encoder::RenderCommandList;
use std::marker::PhantomData;

pub struct RenderGraphContext<'rg> {
    pub(crate) rg: &'rg mut RenderGraph,
    pub(crate) pass_idx: usize,
    pub(crate) pass: Option<RecordedPass>,
}

impl<'s> Drop for RenderGraphContext<'s> {
    fn drop(&mut self) {
        self.rg.record_pass(self.pass.take().unwrap())
    }
}

impl<'rg> RenderGraphContext<'rg> {
    pub fn create<DescType>(
        &mut self,
        desc: DescType,
    ) -> (
        <DescType as CreateHandle>::HandleType,
        <DescType as CreateReference<GpuUav>>::RefType,
    )
    where
        DescType: CreateReference<GpuUav> + CreateHandle,
        DescType: Into<GenericResourceDesc>,
    {
        let handle = <DescType as CreateHandle>::create(ResourceHandle {
            raw: self.rg.create_raw_resource(GraphResourceCreateInfo {
                desc: desc.clone().into(),
                create_pass_idx: self.pass_idx,
            }),
            desc: desc.clone(),
        });

        self.pass.as_mut().unwrap().create.push(handle.raw);

        let reference = RawResourceRef {
            desc: desc.clone(),
            handle: handle.raw,
            marker: PhantomData,
        };
        let reference = <DescType as CreateReference<GpuUav>>::create(reference);

        (handle, reference)
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
        render: impl FnOnce(&mut RenderCommandList<'_>, &ResourceRegistry) -> anyhow::Result<()>
            + 'static,
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
