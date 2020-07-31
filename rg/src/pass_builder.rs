use crate::{
    graph::{GraphResourceCreateInfo, RecordedPass, RenderGraph},
    resource::*,
    resource_registry::ResourceRegistry,
};

use render_core::encoder::RenderCommandList;
use std::marker::PhantomData;

pub struct PassBuilder<'rg> {
    pub(crate) rg: &'rg mut RenderGraph,
    pub(crate) pass_idx: usize,
    pub(crate) pass: Option<RecordedPass>,
}

impl<'s> Drop for PassBuilder<'s> {
    fn drop(&mut self) {
        self.rg.record_pass(self.pass.take().unwrap())
    }
}

pub trait TypeEquals {
    type Other;
    fn same(value: Self) -> Self::Other;
}

impl<T: Sized> TypeEquals for T {
    type Other = Self;
    fn same(value: Self) -> Self::Other {
        value
    }
}

impl<'rg> PassBuilder<'rg> {
    pub fn create<Desc: ResourceDesc>(
        &mut self,
        desc: &Desc,
    ) -> (
        Handle<<Desc as ResourceDesc>::Resource>,
        Ref<<Desc as ResourceDesc>::Resource, GpuUav>,
    )
    where
        Desc: TypeEquals<Other = <<Desc as ResourceDesc>::Resource as Resource>::Desc>,
    {
        let handle: Handle<<Desc as ResourceDesc>::Resource> = Handle {
            raw: self.rg.create_raw_resource(GraphResourceCreateInfo {
                desc: desc.clone().into(),
                create_pass_idx: self.pass_idx,
            }),
            desc: TypeEquals::same(desc.clone()),
            marker: PhantomData,
        };

        self.pass.as_mut().unwrap().write.push(handle.raw);

        let reference = Ref {
            desc: TypeEquals::same(desc.clone()),
            handle: handle.raw,
            marker: PhantomData,
        };

        (handle, reference)
    }

    pub fn write<Res: Resource>(&mut self, handle: &mut Handle<Res>) -> Ref<Res, GpuUav> {
        let pass = self.pass.as_mut().unwrap();

        // Don't know of a good way to use the borrow checker to verify that writes and reads
        // don't overlap, and that multiple writes don't happen to the same resource.
        // The borrow checker will at least check that resources don't alias each other,
        // but for the access in render passes, we resort to a runtime check.
        if pass.write.contains(&handle.raw) {
            panic!("Trying to write twice to the same resource within one render pass");
        } else if pass.read.contains(&handle.raw) {
            panic!("Trying to read and write to the same resource within one render pass");
        }

        pass.write.push(handle.raw);

        Ref {
            desc: handle.desc.clone(),
            handle: handle.raw.next_version(),
            marker: PhantomData,
        }
    }

    pub fn read<Res: Resource>(&mut self, handle: &Handle<Res>) -> Ref<Res, GpuSrv> {
        let pass = self.pass.as_mut().unwrap();

        // Runtime "borrow" check; see info in `write` above.
        if pass.write.contains(&handle.raw) {
            panic!("Trying to read and write to the same resource within one render pass");
        }

        pass.read.push(handle.raw);

        Ref {
            desc: handle.desc.clone(),
            handle: handle.raw,
            marker: PhantomData,
        }
    }

    pub fn render(
        mut self,
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
