use super::*;

#[test]
fn nil_resources_are_rejected_before_foreign_wrapping() {
    assert!(matches!(
        wrap_buffer(std::ptr::null_mut()),
        Err(Error::Allocation {
            resource: "Shared buffer"
        })
    ));
    assert!(matches!(
        wrap_texture(std::ptr::null_mut()),
        Err(Error::Allocation {
            resource: "texture"
        })
    ));
}
