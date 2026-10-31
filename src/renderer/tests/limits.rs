use super::*;

#[test]
fn instance_sizes_reject_overflow_and_excess_before_pointer_arithmetic() {
    assert_eq!(instance_bytes(0, 32), Some(0));
    assert_eq!(instance_bytes(1024, 32), Some(32768));
    assert_eq!(
        instance_bytes(MAX_INSTANCE_BUFFER_SIZE, 1),
        Some(MAX_INSTANCE_BUFFER_SIZE)
    );
    assert_eq!(instance_bytes(MAX_INSTANCE_BUFFER_SIZE + 1, 1), None);
    assert_eq!(instance_bytes(usize::MAX, 2), None);
}

#[test]
fn atlas_dimensions_and_channel_counts_are_bounded() {
    assert_eq!(atlas_byte_count(64, 32, true), Some(2048));
    assert_eq!(atlas_byte_count(64, 32, false), Some(8192));
    assert_eq!(
        atlas_byte_count(16384, 16384, false),
        Some(1024 * 1024 * 1024)
    );
    for (width, height) in [(0, 1), (1, 0), (-1, 1), (1, -1), (16385, 1), (1, 16385)] {
        assert_eq!(atlas_byte_count(width, height, false), None);
    }
}
