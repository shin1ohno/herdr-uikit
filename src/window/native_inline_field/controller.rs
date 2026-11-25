//! The inline field's UIKit objects: the clear full-screen overlay (outside
//! taps), a clipping container, the field's box (background, border,
//! corners), the `UITextField` subclass (Escape), its delegate (Return,
//! length) and the one-line hint.
//!
//! Every callback here comes from UIKit outside GPUI's update and only
//! updates the window's gate or the session; none calls GPUI.

#![allow(
    unsafe_code,
    reason = "defines a UIView, a UITextField subclass and a delegate with objc2's define_class!"
)]

use std::cell::{Cell, RefCell};
use std::rc::Weak;

use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyObject, NSObject, ProtocolObject},
    sel,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSDictionary, NSObjectProtocol, NSRange, NSSet, NSString};
use objc2_ui_kit::{
    UIColor, UIControl, UIControlEvents, UIEvent, UIFont, UILabel, UIModalPresentationStyle,
    UIPress, UIPressesEvent, UIResponder, UIReturnKeyType, UITextBorderStyle, UITextField,
    UITextFieldDelegate, UITextFieldViewMode, UITouch, UIView, UIViewController,
};

use super::policy::{self, Decision, Rect};
use super::{SessionId, gate};
use crate::native_inline_field::{NativeInlineFieldRequest, NativeInlineFieldStyle};
use crate::window::native_modal::{
    color, committed_text, marked, set_accessibility_label, set_identifier, set_return_key_type,
};
use crate::window::state::Window;

/// Which session and window a UIKit object belongs to.
pub(super) struct Binding {
    pub(super) window: Weak<Window>,
    pub(super) session: SessionId,
}

impl Binding {
    fn with(&self, f: impl FnOnce(&Window, SessionId)) {
        if let Some(window) = self.window.upgrade() {
            f(&window, self.session);
        }
    }
}

define_class!(
    // SAFETY: a plain UIView subclass created with `initWithFrame:` after its
    // ivars are set; no Drop; the touch overrides keep UIResponder's
    // signatures and deliberately do not call super, so no touch on the
    // overlay travels up the responder chain.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrNativeInlineOverlay"]
    #[ivars = OverlayState]
    pub(super) struct Overlay;

    impl Overlay {
        // The overlay only receives touches that hit no subview: outside the
        // field (the hint takes no touches). One tap closes the field once;
        // the gate ignores the dismissals that follow.
        #[unsafe(method(touchesBegan:withEvent:))]
        fn touches_began(&self, _touches: &NSSet<UITouch>, _event: Option<&UIEvent>) {
            let state = self.ivars();
            state.taps.set(state.taps.get().saturating_add(1));
            state.binding.with(|window, session| {
                window.native_modal_dismiss(session, gate::DismissReason::OutsideTap);
            });
        }

        #[unsafe(method(touchesMoved:withEvent:))]
        fn touches_moved(&self, _touches: &NSSet<UITouch>, _event: Option<&UIEvent>) {}

        #[unsafe(method(touchesEnded:withEvent:))]
        fn touches_ended(&self, _touches: &NSSet<UITouch>, _event: Option<&UIEvent>) {}

        #[unsafe(method(touchesCancelled:withEvent:))]
        fn touches_cancelled(&self, _touches: &NSSet<UITouch>, _event: Option<&UIEvent>) {}
    }
);

pub(super) struct FieldState {
    binding: Binding,
    /// Escape presses that closed the field, for the UIKit contracts.
    escapes: Cell<u32>,
}

pub(super) struct OverlayState {
    binding: Binding,
    /// Touches the overlay took (outside taps), for QA.
    pub(super) taps: Cell<u32>,
}

define_class!(
    // SAFETY: UITextField may be subclassed; instances come from
    // `initWithFrame:` after ivars are set, there is no Drop, and the press
    // override keeps the superclass signature and calls super.
    #[unsafe(super(UITextField, UIControl, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrNativeInlineField"]
    #[ivars = FieldState]
    pub(super) struct Field;

    impl Field {
        // As the search field: Escape closes only while nothing composes
        // (the IME's Escape cancels the composition), read from the press
        // before UITextField handles it; a key command for Escape did not
        // fire while a field was first responder. The press goes on to super.
        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            let mtm = self.mtm();
            let usages: Vec<isize> = presses
                .iter()
                .filter_map(|press| press.key(mtm).map(|key| key.keyCode().0))
                .collect();
            let dismiss = self.escape_closes(&usages);
            // SAFETY: forwards UIKit's own arguments to UITextField's method.
            let () = unsafe { msg_send![super(self), pressesBegan: presses, withEvent: event] };
            if dismiss {
                self.close_on_escape();
            }
        }
    }
);

impl Field {
    /// Whether presses with `usages` close the field: one is Escape and
    /// nothing composes. Decided before UITextField sees the press.
    pub(super) fn escape_closes(&self, usages: &[isize]) -> bool {
        gate::escape_dismisses(usages.iter().copied(), marked(self))
    }

    pub(super) fn close_on_escape(&self) {
        let state = self.ivars();
        state.escapes.set(state.escapes.get().saturating_add(1));
        state.binding.with(|window, session| {
            window.native_modal_dismiss(session, gate::DismissReason::Escape);
        });
    }
}

define_class!(
    // SAFETY: NSObject subclass; its ivars are plain Rust state dropped with
    // it, no Drop, and each protocol method has objc2-ui-kit's signature.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrNativeInlineTarget"]
    #[ivars = TargetState]
    pub(super) struct Target;

    impl Target {
        #[unsafe(method(herdrInlineEditingChanged:))]
        fn editing_changed(&self, sender: Option<&AnyObject>) {
            let Some(field) = sender.and_then(|s| s.downcast_ref::<UITextField>()) else {
                return;
            };
            if marked(field) {
                return;
            }
            let state = self.ivars();
            let text = committed_text(field);
            let (kept, cut) = policy::clamp(&text, state.max_chars, false);
            if cut {
                field.setText(Some(&NSString::from_str(&kept)));
                state.show(Hint::Length);
            } else if state.shown.get() == Hint::Empty && !kept.trim().is_empty() {
                state.show(Hint::None);
            }
        }
    }

    unsafe impl NSObjectProtocol for Target {}

    unsafe impl UITextFieldDelegate for Target {
        #[unsafe(method(textFieldShouldReturn:))]
        fn should_return(&self, field: &UITextField) -> bool {
            self.ivars().should_return(field)
        }

        #[unsafe(method(textField:shouldChangeCharactersInRange:replacementString:))]
        fn should_change(&self, field: &UITextField, range: NSRange, string: &NSString) -> bool {
            self.ivars().should_change(field, range, string)
        }
    }
);

/// Which hint the label shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Hint {
    None,
    Empty,
    Length,
}

pub(super) struct TargetState {
    binding: Binding,
    max_chars: usize,
    empty_hint: String,
    length_hint: String,
    label: RefCell<Option<Retained<UILabel>>>,
    pub(super) shown: Cell<Hint>,
    /// `shouldChangeCharactersInRange:` calls while the field was marked,
    /// for QA (whether UIKit asks the delegate during composition).
    pub(super) changes_while_marked: Cell<u32>,
}

impl TargetState {
    /// UIKit does not call `textFieldShouldReturn:` while an IME composes
    /// (Return commits the composition then). Checked anyway: a marked
    /// field never closes. Always `false`: the field inserts nothing.
    fn should_return(&self, field: &UITextField) -> bool {
        if marked(field) {
            return false;
        }
        let text = committed_text(field);
        let mut decision = None;
        self.binding.with(|window, session| {
            decision = window.inline_return(session, &text);
        });
        if decision == Some(Decision::Hint) {
            self.show(Hint::Empty);
        }
        false
    }

    /// Typing and Paste outside composition stop at `max_chars`: a longer
    /// change is cut to fit, applied here, and the length hint shows.
    fn should_change(&self, field: &UITextField, range: NSRange, string: &NSString) -> bool {
        if marked(field) {
            // The IME owns marked text; the cut waits for its end.
            self.changes_while_marked
                .set(self.changes_while_marked.get().saturating_add(1));
            return true;
        }
        let current: Vec<u16> = field
            .text()
            .map(|t| t.to_string())
            .unwrap_or_default()
            .encode_utf16()
            .collect();
        let start = range.location.min(current.len());
        let end = range
            .location
            .saturating_add(range.length)
            .min(current.len());
        let before = String::from_utf16_lossy(&current[..start]);
        let after = String::from_utf16_lossy(&current[end..]);
        let insert = string.to_string();
        let kept = before.chars().count() + after.chars().count();
        if kept + insert.chars().count() <= self.max_chars {
            return true;
        }
        let room = self.max_chars.saturating_sub(kept);
        let insert: String = insert.chars().take(room).collect();
        let caret = before.encode_utf16().count() + insert.encode_utf16().count();
        field.setText(Some(&NSString::from_str(&format!(
            "{before}{insert}{after}"
        ))));
        place_caret(field, caret);
        self.show(Hint::Length);
        false
    }

    fn show(&self, hint: Hint) {
        self.shown.set(hint);
        let Some(label) = self.label.borrow().clone() else {
            return;
        };
        let text = match hint {
            Hint::None => None,
            Hint::Empty => Some(self.empty_hint.as_str()),
            Hint::Length => Some(self.length_hint.as_str()),
        };
        label.setText(text.map(NSString::from_str).as_deref());
        label.setHidden(text.is_none());
        label.sizeToFit();
    }
}

/// Moves the caret to UTF-16 offset `offset`.
fn place_caret(field: &UITextField, offset: usize) {
    let offset = isize::try_from(offset).unwrap_or(isize::MAX);
    // SAFETY: UITextInput's position and range getters and the selection
    // setter on UITextField, with positions the field itself returned.
    unsafe {
        let begin: Retained<AnyObject> = msg_send![field, beginningOfDocument];
        let position: Option<Retained<AnyObject>> =
            msg_send![field, positionFromPosition: &*begin, offset: offset];
        if let Some(position) = position {
            let range: Option<Retained<AnyObject>> =
                msg_send![field, textRangeFromPosition: &*position, toPosition: &*position];
            if let Some(range) = range {
                let () = msg_send![field, setSelectedTextRange: &*range];
            }
        }
    }
}

fn cg(rect: Rect) -> CGRect {
    CGRect::new(
        CGPoint::new(f64::from(rect.x), f64::from(rect.y)),
        CGSize::new(f64::from(rect.w), f64::from(rect.h)),
    )
}

/// `UIFont.systemFontOfSize:weight:` (objc2-ui-kit puts it behind a feature
/// this crate does not enable).
fn system_font(size: f32, weight: f32) -> Retained<UIFont> {
    // SAFETY: a class method taking two CGFloats and returning a font.
    unsafe {
        msg_send![
            UIFont::class(),
            systemFontOfSize: f64::from(size),
            weight: f64::from(weight)
        ]
    }
}

/// The placeholder in its own color (`NSForegroundColorAttributeName`).
fn set_placeholder(field: &UITextField, text: &str, rgba: gpui::Rgba) {
    let key = NSString::from_str("NSColor");
    let tint = color(rgba);
    let value: &AnyObject = &tint;
    let attributes: Retained<NSDictionary<NSString, AnyObject>> =
        NSDictionary::from_slices(&[&*key], &[value]);
    let text = NSString::from_str(text);
    // SAFETY: NSAttributedString's designated initializer with a string and
    // an attribute dictionary, then UITextField's attributed placeholder.
    unsafe {
        let allocated: Allocated<AnyObject> = msg_send![objc2::class!(NSAttributedString), alloc];
        let placeholder: Option<Retained<AnyObject>> =
            msg_send![allocated, initWithString: &*text, attributes: &*attributes];
        if let Some(placeholder) = placeholder {
            let () = msg_send![field, setAttributedPlaceholder: &*placeholder];
        }
    }
}

/// The presented controller and the views the window moves.
pub(crate) struct Controller {
    pub(crate) controller: Retained<UIViewController>,
    pub(super) overlay: Retained<Overlay>,
    /// Clips the field to the label's visible area.
    container: Retained<UIView>,
    /// The field's box: the border color, with `inner` inset by its width.
    outer: Retained<UIView>,
    inner: Retained<UIView>,
    pub(super) field: Retained<Field>,
    pub(super) hint: Retained<UILabel>,
    pub(super) target: Retained<Target>,
    style: NativeInlineFieldStyle,
    /// The field's rectangle and whether it is hidden, in overlay points.
    pub(super) placed: Cell<(Rect, bool)>,
}

impl Controller {
    pub(super) fn new(
        binding: impl Fn() -> Binding,
        request: &NativeInlineFieldRequest,
        mtm: MainThreadMarker,
    ) -> Self {
        let style = request.style;
        let ids = &request.identifiers;
        let zero = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(0.0, 0.0));
        let overlay = Overlay::alloc(mtm).set_ivars(OverlayState {
            binding: binding(),
            taps: Cell::new(0),
        });
        // SAFETY: UIView's designated initializer after the ivars.
        let overlay: Retained<Overlay> = unsafe { msg_send![super(overlay), initWithFrame: zero] };
        overlay.setBackgroundColor(Some(&UIColor::clearColor()));
        overlay.setUserInteractionEnabled(true);
        set_identifier(&overlay, &ids.overlay);

        let controller = UIViewController::new(mtm);
        controller.setView(Some(&overlay));
        controller.setModalPresentationStyle(UIModalPresentationStyle::OverFullScreen);

        let container = UIView::initWithFrame(UIView::alloc(mtm), zero);
        container.setClipsToBounds(true);
        overlay.addSubview(&container);
        let outer = UIView::initWithFrame(UIView::alloc(mtm), zero);
        outer.setBackgroundColor(Some(&color(style.border)));
        outer
            .layer()
            .setCornerRadius(f64::from(style.corner_radius));
        container.addSubview(&outer);
        let inner = UIView::initWithFrame(UIView::alloc(mtm), zero);
        inner.setBackgroundColor(Some(&color(style.background)));
        inner.layer().setCornerRadius(f64::from(
            (style.corner_radius - style.border_width).max(0.0),
        ));
        outer.addSubview(&inner);

        let target = Target::alloc(mtm).set_ivars(TargetState {
            binding: binding(),
            max_chars: request.max_chars,
            empty_hint: request.empty_hint.clone(),
            length_hint: request.length_hint.clone(),
            label: RefCell::new(None),
            shown: Cell::new(Hint::None),
            changes_while_marked: Cell::new(0),
        });
        // SAFETY: NSObject's designated initializer after the ivars.
        let target: Retained<Target> = unsafe { msg_send![super(target), init] };

        let field = Field::alloc(mtm).set_ivars(FieldState {
            binding: binding(),
            escapes: Cell::new(0),
        });
        // SAFETY: UITextField's designated initializer after the ivars.
        let field: Retained<Field> = unsafe { msg_send![super(field), initWithFrame: zero] };
        set_identifier(&field, &ids.field);
        set_accessibility_label(&field, &request.accessibility_label);
        field.setText(Some(&NSString::from_str(&request.text)));
        set_placeholder(&field, &request.placeholder, style.placeholder);
        field.setBorderStyle(UITextBorderStyle::None);
        field.setBackgroundColor(Some(&UIColor::clearColor()));
        field.setTextColor(Some(&color(style.text)));
        field.setFont(Some(&system_font(style.font_size, style.font_weight)));
        field.setClearButtonMode(UITextFieldViewMode::Never);
        // SAFETY: a UIColor for the caret and the selection.
        unsafe { field.setTintColor(Some(&color(style.tint))) };
        set_return_key_type(&field, UIReturnKeyType::Done);
        field.setDelegate(Some(ProtocolObject::from_ref(&*target)));
        // SAFETY: the target implements the action, taking the sender; UIKit
        // keeps a weak reference and the controller keeps the target alive.
        unsafe {
            field.addTarget_action_forControlEvents(
                Some(&*target),
                sel!(herdrInlineEditingChanged:),
                UIControlEvents::EditingChanged,
            );
        }
        inner.addSubview(&field);

        let hint = UILabel::initWithFrame(UILabel::alloc(mtm), zero);
        set_identifier(&hint, &ids.hint);
        // SAFETY: a UIColor and a UIFont for the label.
        unsafe {
            hint.setTextColor(Some(&color(style.hint_color)));
            hint.setFont(Some(&UIFont::systemFontOfSize(f64::from(style.hint_size))));
        }
        hint.setHidden(true);
        overlay.addSubview(&hint);
        *target.ivars().label.borrow_mut() = Some(hint.clone());

        Self {
            controller,
            overlay,
            container,
            outer,
            inner,
            field,
            hint,
            target,
            style,
            placed: Cell::new((Rect::default(), true)),
        }
    }

    /// Lays the field over `label` (overlay points), showing only its part
    /// inside `clip`; with nothing inside, the field hides and keeps editing.
    pub(super) fn place(&self, label: Rect, clip: Option<Rect>) {
        let style = self.style;
        let frame = policy::field_rect(label, style.outset_h, style.outset_v, style.min_width);
        let (shown, hidden) = policy::visible(frame, clip);
        self.placed.set((frame, hidden));
        self.container.setHidden(hidden);
        self.container.setFrame(cg(shown));
        let local = Rect {
            x: frame.x - shown.x,
            y: frame.y - shown.y,
            ..frame
        };
        self.outer.setFrame(cg(local));
        let border = style.border_width;
        let inner = Rect {
            x: border,
            y: border,
            w: (frame.w - 2.0 * border).max(0.0),
            h: (frame.h - 2.0 * border).max(0.0),
        };
        self.inner.setFrame(cg(inner));
        let pad_h = (style.padding_h - border).max(0.0);
        let pad_v = (style.padding_v - border).max(0.0);
        self.field.setFrame(cg(Rect {
            x: pad_h,
            y: pad_v,
            w: (inner.w - 2.0 * pad_h).max(0.0),
            h: (inner.h - 2.0 * pad_v).max(0.0),
        }));
        let mut hint = self.hint.frame();
        hint.origin = CGPoint::new(
            f64::from(frame.x),
            f64::from(shown.y + shown.h + style.hint_gap),
        );
        self.hint.setFrame(hint);
        if hidden {
            self.hint.setHidden(true);
        } else {
            self.hint
                .setHidden(self.target.ivars().shown.get() == Hint::None);
        }
    }

    /// Selects the whole text, so typing replaces it.
    pub(super) fn select_all(&self) {
        // SAFETY: UIResponderStandardEditActions' `selectAll:` with no sender.
        let () = unsafe { msg_send![&*self.field, selectAll: None::<&AnyObject>] };
    }

    pub(crate) fn field_view(&self) -> &UIView {
        &self.field
    }

    #[cfg(feature = "test-support")]
    pub(super) fn hint_shown(&self) -> Hint {
        self.target.ivars().shown.get()
    }

    #[cfg(feature = "test-support")]
    pub(super) fn escapes(&self) -> u32 {
        self.field.ivars().escapes.get()
    }
}
