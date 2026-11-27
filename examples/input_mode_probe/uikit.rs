//! The fixture's UIKit side: the reference text field, a search for the
//! system's input mode indicator in every window, a view hierarchy dump for
//! a missed switch, and a screenshot of the caret's neighbourhood. The
//! `_UICursor…` class names are UIKit's private ones, read for QA only.
#![allow(
    unsafe_code,
    reason = "creates the reference field and a screenshot through standard UIKit selectors"
)]

use super::native;
use block2::RcBlock;
use objc2::{
    ClassType, MainThreadMarker, MainThreadOnly, class, msg_send,
    rc::{Allocated, Retained},
    runtime::{AnyObject, NSObjectProtocol},
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::NSString;
use objc2_ui_kit::{
    UIApplication, UIColor, UIImage, UITextField, UITextInput, UITextSelectionDisplayInteraction,
    UIView, UIWindow, UIWindowScene,
};

/// UIKit's view for one input mode indicator. Private; QA only.
pub(super) const ACCESSORY_CLASS: &str = "_UICursorAccessoryView";

/// Where the reference field sits, in window points: below the caret.
pub(super) const FIELD_FRAME: (f64, f64, f64, f64) = (300.0, 520.0, 360.0, 44.0);

/// A standard UIKit text field (the premise probe's search field, which
/// counts the presses reaching it), on the window beside GPUI's root view so
/// the SDK's QA never sees its indicator.
pub(super) fn reference_field(window: &UIWindow, mtm: MainThreadMarker) -> Retained<UITextField> {
    let this = native::PremiseField::alloc(mtm).set_ivars(());
    let (x, y, w, h) = FIELD_FRAME;
    let frame = CGRect::new(CGPoint::new(x, y), CGSize::new(w, h));
    // SAFETY: UITextField's designated initializer.
    let field: Retained<native::PremiseField> =
        unsafe { msg_send![super(this), initWithFrame: frame] };
    field.setBackgroundColor(Some(&UIColor::whiteColor()));
    window.addSubview(&field);
    window.bringSubviewToFront(&field);
    Retained::into_super(Retained::into_super(field))
}

/// A visible indicator: its frame in its window's points and its address.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) struct Accessory {
    pub frame: [f64; 4],
    pub id: usize,
}

fn rect(r: CGRect) -> [f64; 4] {
    [r.origin.x, r.origin.y, r.size.width, r.size.height]
}

fn visible_accessories(view: &UIView, window: &UIWindow, found: &mut Vec<Accessory>) {
    if view.isHidden() || view.alpha() <= 0.0 {
        return;
    }
    if native::class_name(view) == ACCESSORY_CLASS {
        let frame = view.convertRect_toView(view.bounds(), Some(window));
        if frame.size.width > 0.0 && frame.size.height > 0.0 {
            found.push(Accessory {
                frame: rect(frame),
                id: std::ptr::from_ref(view) as usize,
            });
        }
    }
    for child in view.subviews().iter() {
        visible_accessories(&child, window, found);
    }
}

/// Every visible indicator in every window of every connected scene.
pub(super) fn accessories(mtm: MainThreadMarker) -> Vec<Accessory> {
    let mut found = Vec::new();
    for scene in UIApplication::sharedApplication(mtm)
        .connectedScenes()
        .iter()
    {
        let Ok(scene) = scene.downcast::<UIWindowScene>() else {
            continue;
        };
        for window in scene.windows().iter() {
            visible_accessories(&window, &window, &mut found);
        }
    }
    found
}

/// The view hierarchy under `view`, one line per view (class, frame in
/// `root`'s points, alpha, hidden), for a switch that showed no indicator.
pub(super) fn dump(view: &UIView, root: &UIView, depth: usize, lines: &mut Vec<String>) {
    if depth > 14 || lines.len() > 600 {
        return;
    }
    let frame = rect(view.convertRect_toView(view.bounds(), Some(root)));
    lines.push(format!(
        "{:indent$}{} frame={frame:?} alpha={:.2} hidden={}",
        "",
        native::class_name(view),
        view.alpha(),
        view.isHidden(),
        indent = depth * 2
    ));
    for child in view.subviews().iter() {
        dump(&child, root, depth + 1, lines);
    }
}

/// Every window of the app, dumped.
pub(super) fn dump_all(mtm: MainThreadMarker) -> Vec<String> {
    let mut lines = Vec::new();
    for scene in UIApplication::sharedApplication(mtm)
        .connectedScenes()
        .iter()
    {
        let Ok(scene) = scene.downcast::<UIWindowScene>() else {
            continue;
        };
        for window in scene.windows().iter() {
            dump(&window, &window, 0, &mut lines);
        }
    }
    lines
}

/// The caret a standard field draws, in its window's points. UIKit gives it
/// in the field's `textInputView` coordinates.
pub(super) fn field_caret(field: &UITextField, window: &UIWindow) -> [f64; 4] {
    let end = field.endOfDocument();
    let caret = field.caretRectForPosition(&end);
    let space = field.textInputView();
    rect(space.convertRect_toView(caret, Some(window)))
}

/// Whether `responder` is UIKit's first responder (no GPUI involvement).
pub(super) fn is_first(view: &UIView) -> bool {
    view.isFirstResponder()
}

/// The text view's `markedTextRange` as UIKit reads it.
pub(super) fn has_marked(view: &UIView) -> bool {
    // SAFETY: UITextInput's getter; GPUI's text view implements it.
    let range: Option<Retained<AnyObject>> = unsafe { msg_send![view, markedTextRange] };
    range.is_some()
}

/// UIKit's own `caretRectForPosition:` at the text view's selection start.
pub(super) fn uikit_caret(view: &UIView) -> Option<[f64; 4]> {
    // SAFETY: UITextInput's getters; GPUI's text view implements them.
    let range: Option<Retained<AnyObject>> = unsafe { msg_send![view, selectedTextRange] };
    let range = range?;
    // SAFETY: UITextRange's getter.
    let start: Retained<AnyObject> = unsafe { msg_send![&*range, start] };
    // SAFETY: UITextInput's selector with a position from the same object.
    let caret: CGRect = unsafe { msg_send![view, caretRectForPosition: &*start] };
    Some(rect(caret))
}

/// RGBA pixels of `window` as drawn now, and the pixels per point.
pub(super) fn screenshot(window: &UIWindow) -> Option<(image::RgbaImage, f64)> {
    let bounds = window.bounds();
    // SAFETY: UIGraphicsImageRenderer's initializer with a CGRect.
    let renderer: Retained<AnyObject> = unsafe {
        let allocated: Allocated<AnyObject> = msg_send![class!(UIGraphicsImageRenderer), alloc];
        let renderer: Option<Retained<AnyObject>> = msg_send![allocated, initWithBounds: bounds];
        renderer?
    };
    let target: Retained<UIView> = Retained::into_super(objc2::Message::retain(window));
    let drawn = std::rc::Rc::new(std::cell::Cell::new(false));
    let seen = drawn.clone();
    let actions = RcBlock::new(move |_context: *mut AnyObject| {
        seen.set(target.drawViewHierarchyInRect_afterScreenUpdates(bounds, true));
    });
    // SAFETY: `imageWithActions:` takes a block with the renderer context.
    let image: Option<Retained<UIImage>> =
        unsafe { msg_send![&*renderer, imageWithActions: &*actions] };
    let image = image?;
    if !drawn.get() {
        super::record("screenshot: drawViewHierarchyInRect returned NO".into());
    }
    // SAFETY: UIImage's `scale` getter, on the main thread.
    let scale = unsafe { image.scale() };
    let png = image.png_representation()?;
    let decoded = image::load_from_memory(&png.to_vec()).ok()?;
    Some((decoded.to_rgba8(), scale))
}

/// The widest run of adjacent pixel columns in `caret` (window points,
/// padded by 4 pt) where at least 40% of the rows are the system tint, in
/// points. A system caret is a 2 pt run; GPUI's background is not tinted.
pub(super) fn tinted_run(shot: &(image::RgbaImage, f64), caret: [f64; 4]) -> f64 {
    let (image, scale) = shot;
    let pad = 4.0;
    let clamp = |v: f64, max: u32| (v * scale).round().clamp(0.0, f64::from(max)) as u32;
    let (x0, x1) = (
        clamp(caret[0] - pad, image.width()),
        clamp(caret[0] + caret[2] + pad, image.width()),
    );
    let (y0, y1) = (
        clamp(caret[1] - pad, image.height()),
        clamp(caret[1] + caret[3] + pad, image.height()),
    );
    let rows = y1.saturating_sub(y0).max(1);
    let (mut best, mut run) = (0u32, 0u32);
    for x in x0..x1 {
        let tinted = (y0..y1)
            .filter(|&y| {
                let [r, g, b, _] = image.get_pixel(x, y).0;
                let (r, g, b) = (i32::from(r), i32::from(g), i32::from(b));
                b >= 150 && b > r + 60 && b > g + 20
            })
            .count() as u32;
        if tinted * 10 >= rows * 4 {
            run += 1;
            best = best.max(run);
        } else {
            run = 0;
        }
    }
    f64::from(best) / scale
}

/// `UITextSelectionDisplayInteraction`s on `view`.
pub(super) fn selection_interactions(view: &UIView) -> usize {
    view.interactions()
        .iter()
        .filter(|i| i.isKindOfClass(UITextSelectionDisplayInteraction::class()))
        .count()
}

/// The whole document and the selection's offsets, as UIKit reads them from
/// GPUI's text view.
pub(super) fn document(view: &UIView) -> (String, (isize, isize)) {
    // SAFETY: UITextInput's getters and selectors; GPUI's text view
    // implements them, and every position comes from the same object.
    unsafe {
        let begin: Retained<AnyObject> = msg_send![view, beginningOfDocument];
        let end: Retained<AnyObject> = msg_send![view, endOfDocument];
        let all: Option<Retained<AnyObject>> =
            msg_send![view, textRangeFromPosition: &*begin, toPosition: &*end];
        let text: Option<Retained<NSString>> = match &all {
            Some(range) => msg_send![view, textInRange: &**range],
            None => None,
        };
        let selection: Option<Retained<AnyObject>> = msg_send![view, selectedTextRange];
        let offsets = selection.map_or((-1, -1), |range| {
            let start: Retained<AnyObject> = msg_send![&*range, start];
            let stop: Retained<AnyObject> = msg_send![&*range, end];
            let a: isize = msg_send![view, offsetFromPosition: &*begin, toPosition: &*start];
            let b: isize = msg_send![view, offsetFromPosition: &*begin, toPosition: &*stop];
            (a, b)
        });
        (text.map(|t| t.to_string()).unwrap_or_default(), offsets)
    }
}

/// The colours along the middle row of `caret` (window points), padded by 4 pt,
/// for a record when the pixel rule finds nothing.
pub(super) fn caret_row(shot: &(image::RgbaImage, f64), caret: [f64; 4]) -> String {
    let (image, scale) = shot;
    let y = ((caret[1] + caret[3] / 2.0) * scale)
        .round()
        .clamp(0.0, f64::from(image.height() - 1)) as u32;
    let x0 = ((caret[0] - 4.0) * scale).round().max(0.0) as u32;
    let x1 = (((caret[0] + caret[2] + 4.0) * scale).round() as u32).min(image.width());
    let row: Vec<String> = (x0..x1)
        .map(|x| {
            let [r, g, b, _] = image.get_pixel(x, y).0;
            format!("{r:02x}{g:02x}{b:02x}")
        })
        .collect();
    format!(
        "{}x{} scale {scale} row y={y}: {}",
        image.width(),
        image.height(),
        row.join(" ")
    )
}
