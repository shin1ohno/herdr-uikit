//! UIKit object contracts for the inline field, on the process main thread
//! without UIApplicationMain (`tests/native_inline_field_uikit.rs`): the
//! real controller, field, delegate and hint, with no window. What needs a
//! window and first responder (composition, outside taps, the gate) is the
//! UIApplicationMain probe's (`examples/native_inline_field_probe`).
#![allow(
    unsafe_code,
    reason = "main-thread Simulator contract sends UIKit delegate selectors"
)]

use std::rc::Weak;

use objc2::{MainThreadMarker, msg_send, rc::Retained, runtime::AnyObject};
use objc2_core_foundation::CGRect;
use objc2_foundation::{NSRange, NSString};
use objc2_ui_kit::{UIColor, UITextFieldViewMode, UIView};

use super::controller::{Binding, Controller, Hint};
use super::policy::Rect;
use super::{SessionId, gate};
use crate::native_inline_field::{NativeInlineFieldRequest, NativeInlineFieldStyle};

fn request(text: &str) -> NativeInlineFieldRequest {
    NativeInlineFieldRequest {
        text: text.into(),
        placeholder: "Automatic name".into(),
        accessibility_label: "Rename pane claude".into(),
        max_chars: 160,
        empty_hint: "Enter a name, or press Escape to cancel.".into(),
        length_hint: "Names are limited to 160 characters.".into(),
        ..NativeInlineFieldRequest::default()
    }
}

fn controller(text: &str, mtm: MainThreadMarker) -> Controller {
    let binding = || Binding {
        window: Weak::new(),
        session: SessionId(1),
    };
    Controller::new(binding, &request(text), mtm)
}

fn rgba(color: &UIColor) -> [f64; 4] {
    let (mut r, mut g, mut b, mut a) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    // SAFETY: UIColor's component getter with four out-pointers.
    let _: bool =
        unsafe { msg_send![color, getRed: &mut r, green: &mut g, blue: &mut b, alpha: &mut a] };
    [r, g, b, a]
}

fn same(color: Option<Retained<UIColor>>, expected: gpui::Rgba) -> bool {
    let Some(color) = color else {
        return false;
    };
    let [r, g, b, a] = rgba(&color);
    let near = |x: f64, y: f32| (x - f64::from(y)).abs() < 0.01;
    near(r, expected.r) && near(g, expected.g) && near(b, expected.b) && near(a, expected.a)
}

/// `view`'s frame in `root`'s coordinates.
fn frame_in(view: &UIView, root: &UIView) -> CGRect {
    view.convertRect_toView(view.bounds(), Some(root))
}

fn within(a: CGRect, b: Rect) -> bool {
    (a.origin.x - f64::from(b.x)).abs() <= 1.0
        && (a.origin.y - f64::from(b.y)).abs() <= 1.0
        && (a.size.width - f64::from(b.w)).abs() <= 1.0
        && (a.size.height - f64::from(b.h)).abs() <= 1.0
}

fn should_change(c: &Controller, location: usize, length: usize, text: &str) -> bool {
    let string = NSString::from_str(text);
    let range = NSRange::new(location, length);
    // SAFETY: UITextFieldDelegate's selector on the field's own delegate,
    // with the field, a range inside its text and a replacement string.
    unsafe {
        msg_send![
            &*c.target,
            textField: &*c.field,
            shouldChangeCharactersInRange: range,
            replacementString: &*string
        ]
    }
}

fn field_text(c: &Controller) -> String {
    c.field.text().map(|t| t.to_string()).unwrap_or_default()
}

fn style_applied(mtm: MainThreadMarker) {
    let c = controller("claude", mtm);
    let style = NativeInlineFieldStyle::default();
    c.place(
        Rect {
            x: 20.0,
            y: 30.0,
            w: 200.0,
            h: 20.0,
        },
        None,
    );
    let outer = c
        .field_view()
        .superview()
        .and_then(|inner| inner.superview());
    let inner = c.field_view().superview();
    assert!(
        same(
            outer.as_ref().and_then(|o| o.backgroundColor()),
            style.border
        ),
        "style_applied: the box shows the border color #88c0d0"
    );
    assert!(
        same(
            inner.as_ref().and_then(|i| i.backgroundColor()),
            style.background
        ),
        "style_applied: the field's background is #101419"
    );
    let radius = outer.as_ref().map_or(0.0, |o| o.layer().cornerRadius());
    assert!(
        (radius - 4.0).abs() < 0.01,
        "style_applied: corner radius 4 pt"
    );
    let border = match (&outer, &inner) {
        (Some(outer), Some(inner)) => frame_in(inner, outer).origin.x,
        _ => -1.0,
    };
    assert!((border - 1.0).abs() < 0.01, "style_applied: border 1 pt");
    assert!(
        same(c.field.textColor(), style.text),
        "style_applied: text #d8dee9"
    );
    assert!(
        same(c.field.tintColor(), style.tint),
        "style_applied: caret and selection #88c0d0"
    );
    assert_eq!(
        c.field.clearButtonMode(),
        UITextFieldViewMode::Never,
        "style_applied: no clear button"
    );
    assert!(
        c.hint.isHidden(),
        "style_applied: no hint before one is needed"
    );
    eprintln!("INLINE_UIKIT_PASS style_applied");
}

fn paste_200_cut_to_160(mtm: MainThreadMarker) {
    let c = controller("", mtm);
    let pasted: String = "あ".repeat(200);
    let allowed = should_change(&c, 0, 0, &pasted);
    let text = field_text(&c);
    assert!(
        !allowed,
        "paste_200_cut_to_160: the delegate refuses the 200-character paste"
    );
    assert_eq!(
        text.chars().count(),
        160,
        "paste_200_cut_to_160: 160 characters kept"
    );
    assert_eq!(
        c.hint_shown(),
        Hint::Length,
        "paste_200_cut_to_160: length hint"
    );
    assert!(!c.hint.isHidden(), "paste_200_cut_to_160: hint visible");
    let more = should_change(&c, 160, 0, "x");
    assert!(
        !more && field_text(&c).chars().count() == 160,
        "paste_200_cut_to_160: nothing more fits"
    );
    let replaced = should_change(&c, 0, 160, "short");
    assert!(
        replaced,
        "paste_200_cut_to_160: a replacement within the limit goes through"
    );
    eprintln!("INLINE_UIKIT_PASS paste_200_cut_to_160");
}

fn typed_beyond_limit_cut(mtm: MainThreadMarker) {
    let c = controller("", mtm);
    c.field.setText(Some(&NSString::from_str(&"b".repeat(170))));
    // SAFETY: the field's editing-changed action on its own target.
    let () = unsafe {
        msg_send![
            &*c.target,
            herdrInlineEditingChanged: Some(AsRef::<AnyObject>::as_ref(&*c.field))
        ]
    };
    assert_eq!(
        field_text(&c).chars().count(),
        160,
        "typed_beyond_limit_cut: cut after composition"
    );
    assert_eq!(
        c.hint_shown(),
        Hint::Length,
        "typed_beyond_limit_cut: length hint"
    );
    eprintln!("INLINE_UIKIT_PASS typed_beyond_limit_cut");
}

fn set_frame_within_1pt(mtm: MainThreadMarker) {
    let c = controller("zsh", mtm);
    let style = NativeInlineFieldStyle::default();
    for label in [
        Rect {
            x: 40.0,
            y: 100.0,
            w: 50.0,
            h: 20.0,
        },
        Rect {
            x: 300.5,
            y: 412.25,
            w: 200.0,
            h: 18.0,
        },
    ] {
        c.place(label, None);
        let expected =
            super::policy::field_rect(label, style.outset_h, style.outset_v, style.min_width);
        let outer = c
            .field_view()
            .superview()
            .and_then(|inner| inner.superview());
        let shown = outer.map(|outer| frame_in(&outer, &c.overlay));
        assert!(
            shown.is_some_and(|shown| within(shown, expected)),
            "set_frame_within_1pt: box {shown:?} vs {expected:?}"
        );
        assert!(
            expected.w >= 120.0,
            "set_frame_within_1pt: at least 120 pt wide"
        );
    }
    eprintln!("INLINE_UIKIT_PASS set_frame_within_1pt");
}

fn hidden_outside_clip(mtm: MainThreadMarker) {
    let c = controller("zsh", mtm);
    let label = Rect {
        x: 40.0,
        y: 600.0,
        w: 80.0,
        h: 20.0,
    };
    let clip = Rect {
        x: 0.0,
        y: 0.0,
        w: 300.0,
        h: 400.0,
    };
    c.place(label, Some(clip));
    let container = c
        .field_view()
        .superview()
        .and_then(|inner| inner.superview())
        .and_then(|outer| outer.superview());
    assert!(
        container.as_ref().is_some_and(|v| v.isHidden()),
        "hidden_outside_clip: the field hides outside its clip"
    );
    c.place(label, None);
    assert!(
        container.as_ref().is_some_and(|v| !v.isHidden()),
        "hidden_outside_clip: and shows again inside it"
    );
    let partial = Rect {
        x: 0.0,
        y: 0.0,
        w: 300.0,
        h: 608.0,
    };
    c.place(label, Some(partial));
    let height = container.as_ref().map_or(0.0, |v| v.frame().size.height);
    assert!(
        (height - 10.0).abs() <= 1.0,
        "hidden_outside_clip: a partly visible field is cut to the clip ({height})"
    );
    eprintln!("INLINE_UIKIT_PASS hidden_outside_clip");
}

fn escape_press_decision(mtm: MainThreadMarker) {
    let c = controller("zsh", mtm);
    let before = c.escapes();
    assert!(
        !c.field.escape_closes(&[0x04]),
        "escape_press_decision: A does not close"
    );
    assert!(
        c.field.escape_closes(&[0x04, gate::ESCAPE_USAGE]),
        "escape_press_decision: Escape outside composition closes"
    );
    c.field.close_on_escape();
    assert_eq!(
        c.escapes(),
        before + 1,
        "escape_press_decision: the close is counted once"
    );
    assert!(
        !gate::escape_dismisses([gate::ESCAPE_USAGE], true),
        "escape_press_decision: Escape while composing does not close"
    );
    eprintln!("INLINE_UIKIT_PASS escape_press_decision");
}

pub(crate) fn run() {
    let mtm = MainThreadMarker::new()
        .unwrap_or_else(|| panic!("UIKit contracts must run on the main thread"));
    style_applied(mtm);
    paste_200_cut_to_160(mtm);
    typed_beyond_limit_cut(mtm);
    set_frame_within_1pt(mtm);
    hidden_outside_clip(mtm);
    escape_press_decision(mtm);
    eprintln!("INLINE_UIKIT_PASS all");
}
