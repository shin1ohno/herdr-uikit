//! Checked allocation: metal's infallible constructors wrap nil without checking it.
#![allow(unsafe_code)]
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

#[cfg(test)]
#[path = "tests/resources.rs"]
mod tests;
