//! The system picker mediates access to user-installed fonts. Core Text's
//! enumeration alone intentionally exposes only the app's accessible fonts.
#![allow(
    unsafe_code,
    reason = "UIKit delegate subclass and borrowed raw window handle"
)]

mod error;
pub use error::Error;

use futures_channel::oneshot;
use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, rc::Retained,
    runtime::ProtocolObject,
};
use objc2_foundation::{NSObject, NSObjectProtocol};
use objc2_ui_kit::{
    UIFont, UIFontPickerViewController, UIFontPickerViewControllerConfiguration,
    UIFontPickerViewControllerDelegate, UIViewController,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use std::cell::RefCell;

type Reply = oneshot::Sender<Option<String>>;

define_class!(
    // SAFETY: NSObject subclass initialized with init; UIKit invokes its
    // main-thread-only delegate methods with a live picker.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrFontPickerDelegate"]
    #[ivars = RefCell<Option<Reply>>]
    struct Delegate;
    unsafe impl NSObjectProtocol for Delegate {}
    unsafe impl UIFontPickerViewControllerDelegate for Delegate {
        #[unsafe(method(fontPickerViewControllerDidCancel:))]
        fn cancelled(&self, _picker: &UIFontPickerViewController) {
            self.finish(None);
        }
        #[unsafe(method(fontPickerViewControllerDidPickFont:))]
        fn selected(&self, picker: &UIFontPickerViewController) {
            let family = picker.selectedFontDescriptor().map(|descriptor| {
                let font = UIFont::fontWithDescriptor_size(&descriptor, 0.);
                // SAFETY: UIFont retains its family name; copied to Rust here.
                unsafe { font.familyName() }.to_string()
            });
            self.finish(family);
        }
    }
);

impl Delegate {
    fn finish(&self, family: Option<String>) {
        if let Some(reply) = self.ivars().borrow_mut().take() {
            let _ = reply.send(family);
        }
    }
}

/// Keeps the weak delegate alive and dismisses the native sheet if its GPUI
/// task is cancelled, including when the owning window closes.
struct Session {
    picker: Retained<UIFontPickerViewController>,
    _delegate: Retained<Delegate>,
}
impl Drop for Session {
    fn drop(&mut self) {
        self.picker
            .dismissViewControllerAnimated_completion(true, None);
    }
}

/// Open a system font sheet in this exact GPUI window. `None` means cancel;
/// a returned family becomes available through normal Core Text matching.
/// Call and poll on the UI thread. The returned future retains its delegate.
pub fn pick_font<W: HasWindowHandle>(
    window: &W,
) -> Result<impl Future<Output = Option<String>> + use<W>, Error> {
    let mtm = MainThreadMarker::new().ok_or(Error::NotMainThread)?;
    let handle = window.window_handle()?;
    let RawWindowHandle::UiKit(handle) = handle.as_raw() else {
        return Err(Error::NotUIKit);
    };
    let pointer = handle.ui_view_controller.ok_or(Error::NoController)?;
    // SAFETY: the borrowed UIKit handle guarantees a live UIViewController
    // for the duration of the borrow. UIKit retains the presented controller.
    let controller = unsafe { pointer.cast::<UIViewController>().as_ref() };
    if controller.presentedViewController().is_some() {
        return Err(Error::AlreadyPresented);
    }
    let configuration = UIFontPickerViewControllerConfiguration::new(mtm);
    let picker = UIFontPickerViewController::initWithConfiguration(
        UIFontPickerViewController::alloc(mtm),
        &configuration,
    );
    let (reply, receiver) = oneshot::channel();
    let allocated = Delegate::alloc(mtm).set_ivars(RefCell::new(Some(reply)));
    // SAFETY: NSObject's init is the designated initializer for this subclass.
    let delegate: Retained<Delegate> = unsafe { msg_send![super(allocated), init] };
    picker.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    // Prevent swipe dismissal from leaving the async reply pending; the
    // picker's Cancel button still cancels normally.
    picker.setModalInPresentation(true);
    controller.presentViewController_animated_completion(&picker, true, None);
    let session = Session {
        picker,
        _delegate: delegate,
    };
    Ok(async move {
        let result = receiver.await.ok().flatten();
        drop(session);
        result
    })
}
