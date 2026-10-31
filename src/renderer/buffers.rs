//! Adapted from gpui-pre-apple 0.3.6 (zed@bcf6582), Apache-2.0; see NOTICE.md.
//! Source: metal_renderer.rs. Modified for UIKit, checked allocation, and iOS storage.
#![allow(unsafe_code)]
use super::*;
use std::{mem::MaybeUninit, ptr, slice};
pub(crate) struct InstanceBufferPool {
    buffer_size: usize,
    buffers: Vec<metal::Buffer>,
    shared: Option<Arc<shared::SharedResources>>,
}

impl Default for InstanceBufferPool {
    fn default() -> Self {
        Self {
            buffer_size: 2 * 1024 * 1024,
            buffers: Vec::new(),
            shared: None,
        }
    }
}

pub(super) struct InstanceBuffer {
    metal_buffer: metal::Buffer,
    size: usize,
}

impl InstanceBufferPool {
    fn reset(&mut self, buffer_size: usize) {
        self.buffer_size = buffer_size;
        self.buffers.clear();
    }

    pub(super) fn shared_resources(&mut self) -> Result<Arc<shared::SharedResources>> {
        if let Some(shared) = &self.shared {
            return Ok(shared.clone());
        }
        let shared = shared::SharedResources::get()?;
        self.shared = Some(shared.clone());
        Ok(shared)
    }

    fn acquire(&mut self, device: &metal::Device) -> Result<InstanceBuffer> {
        let buffer = match self.buffers.pop() {
            Some(buffer) => buffer,
            None => resources::buffer(device, self.buffer_size)?,
        };
        Ok(InstanceBuffer {
            metal_buffer: buffer,
            size: self.buffer_size,
        })
    }

    pub(super) fn release(&mut self, buffer: InstanceBuffer) {
        if buffer.size == self.buffer_size {
            self.buffers.push(buffer.metal_buffer)
        }
    }
}

#[derive(Clone)]
pub(super) struct InstanceBinding {
    pub(super) buffer: metal::Buffer,
    pub(super) offset: usize,
}

pub(super) struct InstanceBindings {
    pub(super) quads: InstanceBinding,
    pub(super) shadows: InstanceBinding,
    pub(super) underlines: InstanceBinding,
    pub(super) monochrome_sprites: InstanceBinding,
    pub(super) polychrome_sprites: InstanceBinding,
}

pub(super) fn write_instances(
    scene: &Scene,
    writer: &mut InstanceBufferWriter,
) -> Result<InstanceBindings> {
    Ok(InstanceBindings {
        quads: writer.write(&scene.quads)?,
        shadows: writer.write(&scene.shadows)?,
        underlines: writer.write(&scene.underlines)?,
        monochrome_sprites: writer.write(&scene.monochrome_sprites)?,
        polychrome_sprites: writer.write(&scene.polychrome_sprites)?,
    })
}

pub(super) struct InstanceBufferWriter {
    device: metal::Device,
    pool: Arc<Mutex<InstanceBufferPool>>,
    filled: Vec<InstanceBuffer>,
    current: InstanceBuffer,
    pub(super) offset: usize,
}

impl InstanceBufferWriter {
    pub(super) fn new(
        device: &metal::Device,
        pool: &Arc<Mutex<InstanceBufferPool>>,
    ) -> Result<Self> {
        let current = pool.lock().acquire(device)?;
        Ok(Self {
            device: device.clone(),
            pool: pool.clone(),
            filled: Vec::new(),
            current,
            offset: 0,
        })
    }

    fn allocate<T>(&mut self, count: usize) -> Result<(InstanceBinding, &mut [MaybeUninit<T>])> {
        let size =
            limits::instance_bytes(count, size_of::<T>()).ok_or(Error::InstanceBufferLimit {
                count,
                element_size: size_of::<T>(),
                limit: MAX_INSTANCE_BUFFER_SIZE,
            })?;
        let mut offset = self.offset.next_multiple_of(INSTANCE_BUFFER_ALIGNMENT);
        if offset + size > self.current.size {
            self.grow(size)?;
            offset = 0;
        }
        self.offset = offset + size;

        let binding = InstanceBinding {
            buffer: self.current.metal_buffer.clone(),
            offset,
        };
        // SAFETY: only non-null Shared buffers are mapped. Their contents are page
        // aligned; offsets are multiples of 256 and T's alignment is checked below.
        // The checked reservation is within this exclusively owned buffer and
        // never overlaps any earlier reservation. No GPU work uses it until commit.
        if align_of::<T>() > INSTANCE_BUFFER_ALIGNMENT {
            return Err(Error::InstanceBufferAlignment {
                alignment: align_of::<T>(),
            });
        }
        let values = unsafe {
            let start = (self.current.metal_buffer.contents() as *mut u8).add(offset);
            slice::from_raw_parts_mut(start.cast::<MaybeUninit<T>>(), count)
        };
        Ok((binding, values))
    }

    pub(super) fn write<T: Copy>(&mut self, values: &[T]) -> Result<InstanceBinding> {
        let (binding, destination) = self.allocate::<T>(values.len())?;
        // SAFETY: allocate checked size, non-null Shared storage, and 256-byte
        // alignment. This disjoint reservation fits the entire Copy slice and the
        // GPU cannot read it until the command buffer is committed.
        unsafe {
            ptr::copy_nonoverlapping(
                values.as_ptr(),
                destination.as_mut_ptr().cast::<T>(),
                values.len(),
            );
        }
        Ok(binding)
    }

    fn grow(&mut self, required: usize) -> Result<()> {
        let mut pool = self.pool.lock();
        let buffer_size = (pool.buffer_size * 2)
            .max(required.next_power_of_two())
            .min(MAX_INSTANCE_BUFFER_SIZE);
        if buffer_size < required || buffer_size <= self.current.size {
            return Err(Error::InstanceBufferLimit {
                count: required,
                element_size: 1,
                limit: MAX_INSTANCE_BUFFER_SIZE,
            });
        }
        if buffer_size != pool.buffer_size {
            tracing::info!("increased instance buffer size to {buffer_size}");
            pool.reset(buffer_size);
        }
        let buffer = pool.acquire(&self.device)?;
        drop(pool);

        let filled = mem::replace(&mut self.current, buffer);
        self.filled.push(filled);
        self.offset = 0;
        Ok(())
    }

    pub(super) fn finish(self) -> InstanceBuffer {
        // All CPU writes use Shared storage on iOS: didModifyRange: is macOS-only.
        // Metal retains encoded resources; only the final largest buffer is pooled.
        self.current
    }
}
