//! Scoped native selector instrumentation, only during the main-thread fixture.
//! Every installed IMP has the original ABI and is restored on unwind.
use std::{cell::Cell, mem::transmute};

use objc2::{
    ClassType, MainThreadMarker,
    runtime::{AnyClass, AnyObject, Imp, Method, Sel},
    sel,
};
use objc2_foundation::NSInteger;
use objc2_ui_kit::{UITextPosition, UITextRange};

use super::super::super::{positions::TextRange, view_class::GPUITextInputView};

type Setter = unsafe extern "C-unwind" fn(*const AnyObject, Sel, *const UITextRange);
type Getter = unsafe extern "C-unwind" fn(*const AnyObject, Sel) -> *mut AnyObject;
type Offset = unsafe extern "C-unwind" fn(
    *const AnyObject,
    Sel,
    *const UITextPosition,
    *const UITextPosition,
) -> NSInteger;
type RangeGetter = unsafe extern "C-unwind" fn(
    *const AnyObject,
    Sel,
    *const UITextPosition,
    *const UITextPosition,
) -> *mut UITextRange;

thread_local! {
    static SETTER: Cell<Option<Imp>> = const { Cell::new(None) };
    static SETTERS: Cell<usize> = const { Cell::new(0) };
    static OFFSETS: Cell<usize> = const { Cell::new(0) };
}

pub(super) struct Override {
    method: &'static Method,
    previous: Imp,
}

pub(super) enum NilGetter {
    Beginning,
    DocumentEnd,
    RangeStart,
    RangeEnd,
}

impl Override {
    unsafe fn install(class: &'static AnyClass, selector: Sel, replacement: Imp) -> Self {
        MainThreadMarker::new().expect("selector override requires main thread");
        let method = class
            .instance_method(selector)
            .expect("native method exists");
        // SAFETY: caller supplies the exact signature, on the main thread before
        // the test body. No test pumps the run loop or leaves an override active.
        let previous = unsafe { method.set_implementation(replacement) };
        Self { method, previous }
    }

    pub fn count_setters() -> Self {
        SETTERS.set(0);
        // SAFETY: setter forwards even nil to its original implementation.
        let guard = unsafe {
            Self::install(
                GPUITextInputView::class(),
                sel!(setSelectedTextRange:),
                transmute::<Setter, Imp>(count_setter),
            )
        };
        SETTER.set(Some(guard.previous));
        guard
    }

    pub fn nil_getter(kind: NilGetter) -> Self {
        let (class, selector) = match kind {
            NilGetter::Beginning => (GPUITextInputView::class(), sel!(beginningOfDocument)),
            NilGetter::DocumentEnd => (GPUITextInputView::class(), sel!(endOfDocument)),
            NilGetter::RangeStart => (TextRange::class(), sel!(start)),
            NilGetter::RangeEnd => (TextRange::class(), sel!(end)),
        };
        // SAFETY: used only for no-argument object getters; nil needs no retain.
        unsafe { Self::install(class, selector, transmute::<Getter, Imp>(nil)) }
    }

    pub fn nil_range() -> Self {
        // SAFETY: exact two-position/object-return ABI; returning nil is allowed.
        unsafe {
            Self::install(
                GPUITextInputView::class(),
                sel!(textRangeFromPosition:toPosition:),
                transmute::<RangeGetter, Imp>(nil_range),
            )
        }
    }

    pub fn refuse_selection() -> Self {
        // SAFETY: the setter accepts the original arguments but ignores them.
        unsafe {
            Self::install(
                GPUITextInputView::class(),
                sel!(setSelectedTextRange:),
                transmute::<Setter, Imp>(ignore_setter),
            )
        }
    }

    pub fn offsets(reversed: bool) -> Self {
        OFFSETS.set(0);
        let imp: Offset = if reversed {
            reversed_offsets
        } else {
            negative_offset
        };
        // SAFETY: exact native NSInteger return and two-position argument ABI.
        unsafe {
            Self::install(
                GPUITextInputView::class(),
                sel!(offsetFromPosition:toPosition:),
                transmute::<Offset, Imp>(imp),
            )
        }
    }
}

impl Drop for Override {
    fn drop(&mut self) {
        // SAFETY: restore exactly the IMP returned by this method's installation.
        unsafe {
            self.method.set_implementation(self.previous);
        }
    }
}

pub(super) fn setters() -> usize {
    SETTERS.get()
}

unsafe extern "C-unwind" fn count_setter(
    this: *const AnyObject,
    selector: Sel,
    range: *const UITextRange,
) {
    SETTERS.set(SETTERS.get() + 1);
    // SAFETY: the preserved IMP is this setter with exactly the same arguments.
    let original =
        unsafe { transmute::<Imp, Setter>(SETTER.get().expect("original setter installed")) };
    unsafe {
        original(this, selector, range);
    }
}
unsafe extern "C-unwind" fn nil(_: *const AnyObject, _: Sel) -> *mut AnyObject {
    std::ptr::null_mut()
}
unsafe extern "C-unwind" fn ignore_setter(_: *const AnyObject, _: Sel, _: *const UITextRange) {
    SETTERS.set(SETTERS.get() + 1);
}
unsafe extern "C-unwind" fn nil_range(
    _: *const AnyObject,
    _: Sel,
    _: *const UITextPosition,
    _: *const UITextPosition,
) -> *mut UITextRange {
    std::ptr::null_mut()
}
unsafe extern "C-unwind" fn negative_offset(
    _: *const AnyObject,
    _: Sel,
    _: *const UITextPosition,
    _: *const UITextPosition,
) -> NSInteger {
    -1
}
unsafe extern "C-unwind" fn reversed_offsets(
    _: *const AnyObject,
    _: Sel,
    _: *const UITextPosition,
    _: *const UITextPosition,
) -> NSInteger {
    let count = OFFSETS.get();
    OFFSETS.set(count + 1);
    if count == 0 { 7 } else { 5 }
}
