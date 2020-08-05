use render_core::{
    device::RenderDevice,
    encoder::RenderCommandList,
    handles::{RenderResourceHandle, RenderResourceHandleAllocator},
    types::{IntoConstantBufferWithOffset, RenderBindFlags, RenderBufferDesc, RenderResourceType},
};
use std::{
    mem::size_of,
    sync::{Arc, RwLock},
};

const CHUNK_SIZE: usize = 64 * 1024;
const ALIGNMENT: usize = 256;

fn as_byte_slice<'a, T>(t: &'a T) -> &'a [u8]
where
    T: Copy,
{
    unsafe { std::slice::from_raw_parts(t as *const T as *mut u8, std::mem::size_of::<T>()) }
}

enum ChunkBuffer {
    Unbacked(RenderResourceHandle),
    Backed(RenderResourceHandle),
}

impl ChunkBuffer {
    fn handle(&self) -> RenderResourceHandle {
        match self {
            Self::Unbacked(h) => *h,
            Self::Backed(h) => *h,
        }
    }
}

struct Chunk {
    data: Box<[u8]>,
    buffer: ChunkBuffer,
    write_head: u32,
}

impl Chunk {
    fn free_space(&self) -> usize {
        CHUNK_SIZE - self.write_head as usize
    }
}

#[derive(Copy, Clone)]
pub struct DynamicConstantsAllocation {
    pub buffer: RenderResourceHandle,
    pub offset: usize,
}

impl IntoConstantBufferWithOffset for DynamicConstantsAllocation {
    fn into_constant_buffer_with_offset(self) -> (RenderResourceHandle, usize) {
        (self.buffer, self.offset)
    }
}

pub struct DynamicConstants {
    chunks: Vec<Chunk>,
    free_chunks: Vec<Chunk>,
    handles: Arc<RwLock<RenderResourceHandleAllocator>>,
}

impl DynamicConstants {
    pub fn new(handles: Arc<RwLock<RenderResourceHandleAllocator>>) -> Self {
        Self {
            chunks: Default::default(),
            free_chunks: Default::default(),
            handles,
        }
    }

    fn alloc_chunk(&mut self) {
        if !self.free_chunks.is_empty() {
            self.chunks.push(self.free_chunks.pop().unwrap());
        } else {
            let buffer = self
                .handles
                .write()
                .unwrap()
                .allocate(RenderResourceType::Buffer);

            self.chunks.push(Chunk {
                data: vec![0u8; CHUNK_SIZE].into_boxed_slice(),
                buffer: ChunkBuffer::Unbacked(buffer),
                write_head: 0,
            });
        }
    }

    pub fn commit_and_reset(
        &mut self,
        command_list: &mut RenderCommandList<'_>,
        device: &dyn RenderDevice,
    ) {
        for chunk in self.chunks.iter_mut() {
            let buffer = match chunk.buffer {
                ChunkBuffer::Unbacked(handle) => {
                    device
                        .create_buffer(
                            handle,
                            &RenderBufferDesc {
                                bind_flags: RenderBindFlags::CONSTANT_BUFFER,
                                size: CHUNK_SIZE,
                            },
                            None,
                            "dynamic constant buffer chunk".into(),
                        )
                        .unwrap();

                    chunk.buffer = ChunkBuffer::Backed(handle);
                    handle
                }
                ChunkBuffer::Backed(handle) => handle,
            };

            command_list.update_buffer(buffer, 0, &*chunk.data).unwrap();
            chunk.write_head = 0;
        }

        self.free_chunks.append(&mut self.chunks);
    }

    pub fn push<T: Copy>(&mut self, t: T) -> DynamicConstantsAllocation {
        let t_size = size_of::<T>();
        assert!(t_size <= CHUNK_SIZE);

        if self.chunks.is_empty() || self.chunks.last().unwrap().free_space() < t_size {
            self.alloc_chunk();
        }

        let mut chunk = self.chunks.last_mut().unwrap();
        assert!(chunk.free_space() >= CHUNK_SIZE);

        let dst = &mut chunk.data[chunk.write_head as usize..chunk.write_head as usize + t_size];
        dst.copy_from_slice(as_byte_slice(&t));

        let allocation = DynamicConstantsAllocation {
            buffer: chunk.buffer.handle(),
            offset: chunk.write_head as usize,
        };

        let t_size_aligned = (t_size + ALIGNMENT - 1) & !(ALIGNMENT - 1);
        chunk.write_head += t_size_aligned as u32;

        allocation
    }

    pub fn destroy(&mut self, device: &mut dyn RenderDevice) {
        assert!(
            self.chunks.is_empty(),
            "live chunks still present; commit_and_reset() must be called before destroy()"
        );

        for chunk in self.free_chunks.drain(..) {
            if let ChunkBuffer::Backed(buffer) = chunk.buffer {
                device.destroy_resource(buffer).unwrap();
            } else {
                unreachable!();
            }
        }
    }
}
