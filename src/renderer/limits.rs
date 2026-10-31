//! Bounds checked before allocating or handing raw memory to Metal.
pub(crate) const MAX_INSTANCE_BUFFER_SIZE: usize = 256 * 1024 * 1024;

pub(crate) fn instance_bytes(count: usize, element_size: usize) -> Option<usize> {
    count
        .checked_mul(element_size)
        .filter(|size| *size <= MAX_INSTANCE_BUFFER_SIZE)
}

pub(crate) fn atlas_byte_count(width: i32, height: i32, monochrome: bool) -> Option<usize> {
    if !(1..=16384).contains(&width) || !(1..=16384).contains(&height) {
        return None;
    }
    Some(width as usize * height as usize * if monochrome { 1 } else { 4 })
}

#[cfg(test)]
#[path = "tests/limits.rs"]
mod tests;
