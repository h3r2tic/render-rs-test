use render_core::handles::RenderResourceHandle;
use std::sync::Mutex;

lazy_static::lazy_static! {
    static ref RENDER_RESOURCES_PENDING_RELEASE: Mutex<Vec<RenderResourceHandle>> =
        Mutex::new(Vec::new());
}

pub(crate) fn get_resources_pending_release() -> Vec<RenderResourceHandle> {
    let mut pending_release = RENDER_RESOURCES_PENDING_RELEASE.lock().unwrap();
    std::mem::take(&mut *pending_release)
}

pub struct OwnedRenderResourceHandle(RenderResourceHandle);

impl OwnedRenderResourceHandle {
    pub fn new(h: RenderResourceHandle) -> Self {
        Self(h)
    }
}

impl std::ops::Deref for OwnedRenderResourceHandle {
    type Target = RenderResourceHandle;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl Drop for OwnedRenderResourceHandle {
    fn drop(&mut self) {
        let mut pending_release = RENDER_RESOURCES_PENDING_RELEASE.lock().unwrap();
        pending_release.push(self.0);
    }
}
