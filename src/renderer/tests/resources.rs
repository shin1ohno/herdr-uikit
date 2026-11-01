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

#[test]
fn nil_command_queue_is_rejected_before_foreign_wrapping() {
    assert!(matches!(
        wrap_command_queue(std::ptr::null_mut()),
        Err(Error::Allocation {
            resource: "command queue"
        })
    ));
}

#[test]
fn nil_command_buffer_is_rejected_before_borrowing_or_retaining() {
    assert!(matches!(
        wrap_command_buffer(std::ptr::null_mut()),
        Err(Error::Allocation {
            resource: "command buffer"
        })
    ));
}

#[test]
fn nil_render_command_encoder_is_rejected_before_borrowing_or_retaining() {
    assert!(matches!(
        wrap_render_command_encoder(std::ptr::null_mut()),
        Err(Error::Allocation {
            resource: "render command encoder"
        })
    ));
}
