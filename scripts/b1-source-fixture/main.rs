//! Extracted-source test HOST, not UIKit or a shipping-target compilation.
//! The runner copies the production operation/API/coordinator/legacy-slot code.
//! Only external imports/error derives are adapted; native endpoint, registry,
//! MainThreadMarker and GPUI handle types below are explicit test doubles.
#![allow(dead_code)]

pub type AnyWindowHandle = u64;
#[derive(Clone, Copy)]
pub struct MainThreadMarker;
impl MainThreadMarker {
    pub fn new() -> Option<Self> {
        Some(Self)
    }
}

mod gpui_call;
mod native_operation;
mod native_retirement_request;
mod text_input_retirement;
pub use native_retirement_request::*;
pub use text_input_retirement::*;

mod platform {
    mod slot;
}
mod cases;
mod raw_bodies;
mod window;
