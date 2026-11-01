//! Checked allocation: metal's infallible constructors wrap nil without checking it.
#![allow(
    unsafe_code,
    reason = "checks nullable Metal constructors before wrapping their Objective-C objects"
)]
use super::{Error, Result};
use foreign_types::{ForeignType, ForeignTypeRef};
use objc2::{msg_send, runtime::AnyObject};
use std::ffi::c_void;

pub(super) fn buffer(device: &metal::DeviceRef, length: usize) -> Result<metal::Buffer> {
    // SAFETY: device is a live MTLDevice. The selector takes NSUInteger length and
    // MTLResourceOptions and returns an owned nullable MTLBuffer (+1).
    let raw: *mut AnyObject = unsafe {
        let object = &*device.as_ptr().cast::<AnyObject>();
        msg_send![object, newBufferWithLength: length as u64, options: options()]
    };
    wrap_buffer(raw)
}

pub(super) fn buffer_with_data(device: &metal::DeviceRef, data: &[u64]) -> Result<metal::Buffer> {
    // SAFETY: the live device copies the valid slice during this call. The selector
    // returns an owned nullable MTLBuffer; it does not retain the borrowed bytes.
    let raw: *mut AnyObject = unsafe {
        let object = &*device.as_ptr().cast::<AnyObject>();
        msg_send![object, newBufferWithBytes: data.as_ptr().cast::<c_void>(),
            length: size_of_val(data) as u64, options: options()]
    };
    wrap_buffer(raw)
}

fn options() -> u64 {
    (metal::MTLResourceOptions::StorageModeShared
        | metal::MTLResourceOptions::CPUCacheModeWriteCombined)
        .bits()
}

fn wrap_buffer(raw: *mut AnyObject) -> Result<metal::Buffer> {
    if raw.is_null() {
        return Err(Error::Allocation {
            resource: "Shared buffer",
        });
    }
    // SAFETY: checked non-null and returned at +1 by an MTLDevice buffer constructor.
    Ok(unsafe { metal::Buffer::from_ptr(raw.cast()) })
}

pub(super) fn texture(
    device: &metal::DeviceRef,
    descriptor: &metal::TextureDescriptorRef,
) -> Result<metal::Texture> {
    // SAFETY: both objects are live Metal objects; newTextureWithDescriptor: accepts
    // this descriptor and returns an owned nullable MTLTexture (+1).
    let raw: *mut AnyObject = unsafe {
        let object = &*device.as_ptr().cast::<AnyObject>();
        let descriptor = &*descriptor.as_ptr().cast::<AnyObject>();
        msg_send![object, newTextureWithDescriptor: descriptor]
    };
    wrap_texture(raw)
}

fn wrap_texture(raw: *mut AnyObject) -> Result<metal::Texture> {
    if raw.is_null() {
        return Err(Error::Allocation {
            resource: "texture",
        });
    }
    // SAFETY: checked non-null, owned +1 MTLTexture; the wrapper releases it once.
    Ok(unsafe { metal::Texture::from_ptr(raw.cast()) })
}

pub(super) fn command_queue(device: &metal::DeviceRef) -> Result<metal::CommandQueue> {
    // SAFETY: device is a live MTLDevice. newCommandQueue takes no arguments
    // and returns an owned nullable MTLCommandQueue (+1).
    let raw: *mut AnyObject = unsafe {
        let object = &*device.as_ptr().cast::<AnyObject>();
        msg_send![object, newCommandQueue]
    };
    wrap_command_queue(raw)
}

fn wrap_command_queue(raw: *mut AnyObject) -> Result<metal::CommandQueue> {
    if raw.is_null() {
        return Err(Error::Allocation {
            resource: "command queue",
        });
    }
    // SAFETY: checked non-null, owned +1 MTLCommandQueue; the wrapper releases it once.
    Ok(unsafe { metal::CommandQueue::from_ptr(raw.cast()) })
}

pub(super) fn command_buffer(queue: &metal::CommandQueueRef) -> Result<metal::CommandBuffer> {
    // SAFETY: queue is a live MTLCommandQueue. commandBuffer takes no arguments
    // and returns an autoreleased nullable MTLCommandBuffer (+0).
    let raw: *mut AnyObject = unsafe {
        let object = &*queue.as_ptr().cast::<AnyObject>();
        msg_send![object, commandBuffer]
    };
    wrap_command_buffer(raw)
}

fn wrap_command_buffer(raw: *mut AnyObject) -> Result<metal::CommandBuffer> {
    if raw.is_null() {
        return Err(Error::Allocation {
            resource: "command buffer",
        });
    }
    // SAFETY: checked non-null, live autoreleased MTLCommandBuffer. to_owned
    // retains the borrowed +0 object before the current autorelease pool drains.
    Ok(unsafe { metal::CommandBufferRef::from_ptr(raw.cast()) }.to_owned())
}

pub(super) fn render_command_encoder(
    command_buffer: &metal::CommandBufferRef,
    descriptor: &metal::RenderPassDescriptorRef,
) -> Result<metal::RenderCommandEncoder> {
    // SAFETY: both objects are live Metal objects. This selector accepts the
    // descriptor and returns an autoreleased nullable MTLRenderCommandEncoder (+0).
    let raw: *mut AnyObject = unsafe {
        let object = &*command_buffer.as_ptr().cast::<AnyObject>();
        let descriptor = &*descriptor.as_ptr().cast::<AnyObject>();
        msg_send![object, renderCommandEncoderWithDescriptor: descriptor]
    };
    wrap_render_command_encoder(raw)
}

fn wrap_render_command_encoder(raw: *mut AnyObject) -> Result<metal::RenderCommandEncoder> {
    if raw.is_null() {
        return Err(Error::Allocation {
            resource: "render command encoder",
        });
    }
    // SAFETY: checked non-null, live autoreleased MTLRenderCommandEncoder.
    // to_owned retains the +0 object; dropping the wrapper balances that retain.
    Ok(unsafe { metal::RenderCommandEncoderRef::from_ptr(raw.cast()) }.to_owned())
}

#[cfg(test)]
#[path = "tests/resources.rs"]
mod tests;
