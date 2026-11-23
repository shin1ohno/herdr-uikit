//! The prototype's UIKit side: a `UISearchTextField` subclass that counts the
//! presses reaching it, the presented controller, and responder queries.
#![allow(
    unsafe_code,
    reason = "defines a UISearchTextField subclass and sends standard UIKit selectors"
)]

use block2::RcBlock;
use objc2::{
    ClassType, MainThreadMarker, MainThreadOnly, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObject, Sel},
    sel,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSArray, NSRange, NSSet, NSString};
use objc2_ui_kit::{
    UIApplication, UIColor, UIControl, UILabel, UIModalPresentationStyle, UIPopoverArrowDirection,
    UIPress, UIPressesEvent, UIResponder, UISearchTextField, UISheetPresentationControllerDetent,
    UITextField, UITextInputMode, UIView, UIViewController, UIWindow, UIWindowScene,
};
use std::cell::{Cell, RefCell};

thread_local! {
    static FIELD_PRESSES: Cell<u32> = const { Cell::new(0) };
    static PREFER_JAPANESE: Cell<bool> = const { Cell::new(false) };
    static LABELS: RefCell<Vec<Retained<UILabel>>> = const { RefCell::new(Vec::new()) };
    static FIELDS: RefCell<Vec<Retained<PremiseField>>> = const { RefCell::new(Vec::new()) };
}

pub(super) const GPUI_CLASSES: [&str; 2] = ["HerdrUIKitGPUIView", "GPUITextInputView"];

define_class!(
    // SAFETY: UISearchTextField may be subclassed; instances are created with
    // `initWithFrame:`, there are no ivars or Drop, and both overrides keep the
    // superclass signatures and call super.
    #[unsafe(super(UISearchTextField, UITextField, UIControl, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrPremiseSearchField"]
    pub(super) struct PremiseField;

    impl PremiseField {
        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            FIELD_PRESSES.set(FIELD_PRESSES.get() + 1);
            // SAFETY: forwards UIKit's own arguments to UITextField's method.
            let () = unsafe { msg_send![super(self), pressesBegan: presses, withEvent: event] };
        }

        #[unsafe(method_id(textInputMode))]
        fn text_input_mode(&self) -> Option<Retained<UITextInputMode>> {
            let preferred = if PREFER_JAPANESE.get() { japanese_mode(self.mtm()) } else { None };
            // SAFETY: UIResponder's getter, returning an optional mode.
            preferred.or_else(|| unsafe { msg_send![super(self), textInputMode] })
        }
    }
);

pub(super) fn field_presses() -> u32 {
    FIELD_PRESSES.get()
}

pub(super) fn prefer_japanese(on: bool) {
    PREFER_JAPANESE.set(on);
}

fn japanese_mode(mtm: MainThreadMarker) -> Option<Retained<UITextInputMode>> {
    UITextInputMode::activeInputModes(mtm).iter().find(|mode| {
        mode.primaryLanguage()
            .is_some_and(|language| language.to_string().starts_with("ja"))
    })
}

pub(super) fn input_mode_languages(mtm: MainThreadMarker) -> Vec<String> {
    UITextInputMode::activeInputModes(mtm)
        .iter()
        .map(|mode| {
            mode.primaryLanguage()
                .map_or_else(|| "nil".into(), |language| language.to_string())
        })
        .collect()
}

/// Whether `name` is one of GPUI's two view classes (objc2 may decorate the
/// text view's default class name with its module path and version).
pub(super) fn is_gpui_class(name: &str) -> bool {
    GPUI_CLASSES.iter().any(|class| name.contains(class))
}

pub(super) fn class_name(object: &AnyObject) -> String {
    object.class().name().to_string_lossy().into_owned()
}

/// The window whose root controller's view is the GPUI drawing view.
pub(super) fn gpui_window(mtm: MainThreadMarker) -> Option<Retained<UIWindow>> {
    let scenes = UIApplication::sharedApplication(mtm).connectedScenes();
    for scene in scenes.iter() {
        let Ok(scene) = scene.downcast::<UIWindowScene>() else {
            continue;
        };
        for window in scene.windows().iter() {
            let root_view = window
                .rootViewController()
                .and_then(|root| root.viewIfLoaded());
            if root_view.is_some_and(|view| class_name(&view) == GPUI_CLASSES[0]) {
                return Some(window);
            }
        }
    }
    None
}

/// The view in `window`'s hierarchy that is first responder, if any.
pub(super) fn first_responder(window: &UIWindow) -> Option<Retained<UIView>> {
    fn search(view: &UIView) -> Option<Retained<UIView>> {
        if view.isFirstResponder() {
            return Some(objc2::Message::retain(view));
        }
        view.subviews().iter().find_map(|child| search(&child))
    }
    search(window)
}

/// Class names along `responder.nextResponder` until nil.
pub(super) fn chain(responder: &UIResponder) -> Vec<String> {
    let mut names = vec![class_name(responder)];
    let mut next = responder.nextResponder();
    while let Some(current) = next {
        names.push(class_name(&current));
        next = current.nextResponder();
        if names.len() > 64 {
            break;
        }
    }
    names
}

/// Whether `target` (by identity) is on `responder`'s `nextResponder` chain.
pub(super) fn chain_contains(responder: &UIResponder, target: &AnyObject) -> bool {
    let target = std::ptr::from_ref(target).cast::<()>();
    let mut next = responder.nextResponder();
    for _ in 0..64 {
        let Some(current) = next else {
            return false;
        };
        if Retained::as_ptr(&current).cast::<()>() == target {
            return true;
        }
        next = current.nextResponder();
    }
    false
}

/// GPUI's text input view, found by class under the root view.
pub(super) fn gpui_text_view(window: &UIWindow) -> Option<Retained<UIView>> {
    let root = window.rootViewController()?.viewIfLoaded()?;
    root.subviews()
        .iter()
        .find(|view| class_name(view).contains(GPUI_CLASSES[1]))
}

/// The first view of class `name` in `view`'s hierarchy.
pub(super) fn find_class(view: &UIView, name: &str) -> Option<Retained<UIView>> {
    if class_name(view) == name {
        return Some(objc2::Message::retain(view));
    }
    view.subviews()
        .iter()
        .find_map(|child| find_class(&child, name))
}

/// GPUI's two views: whether each is first responder.
pub(super) fn gpui_first_responders(window: &UIWindow) -> (bool, bool) {
    let Some(root) = window
        .rootViewController()
        .and_then(|root| root.viewIfLoaded())
    else {
        return (false, false);
    };
    let text = root
        .subviews()
        .iter()
        .find(|view| class_name(view).contains(GPUI_CLASSES[1]))
        .is_some_and(|view| view.isFirstResponder());
    (root.isFirstResponder(), text)
}

/// Sends `action` with `sender` as its argument to the first responder
/// UIKit resolves (`targetForAction:` from nil), as the Edit menu does.
pub(super) fn send_to_first_responder(
    mtm: MainThreadMarker,
    action: Sel,
    sender: Option<&AnyObject>,
) -> bool {
    // SAFETY: a standard action selector; UIKit picks the target.
    unsafe {
        UIApplication::sharedApplication(mtm)
            .sendAction_to_from_forEvent(action, None, sender, None)
    }
}

/// `insertText:` sent to `responder`, the object UIKit reports as first
/// responder (the system keyboard and IME deliver text to it).
pub(super) fn insert_text(responder: &UIResponder, text: &str) {
    let text = NSString::from_str(text);
    // SAFETY: UIKeyInput's selector; both candidate responders implement it.
    let () = unsafe { msg_send![responder, insertText: &*text] };
}

pub(super) fn set_marked(responder: &UIResponder, text: &str) {
    let text = NSString::from_str(text);
    // SAFETY: UITextInput's selectors; the field implements both.
    let () =
        unsafe { msg_send![responder, setMarkedText: &*text, selectedRange: NSRange::new(0, 0)] };
}

pub(super) fn unmark(responder: &UIResponder) {
    // SAFETY: UITextInput's selector.
    let () = unsafe { msg_send![responder, unmarkText] };
}

pub(super) fn responds(object: &AnyObject, selector: Sel) -> bool {
    // SAFETY: NSObject's selector.
    unsafe { msg_send![object, respondsToSelector: selector] }
}

pub(super) fn marked(field: &UITextField) -> bool {
    // SAFETY: UITextInput's getter on UITextField.
    let range: Option<Retained<AnyObject>> = unsafe { msg_send![field, markedTextRange] };
    range.is_some()
}

fn set_identifier(view: &UIView, identifier: &str) {
    let identifier = NSString::from_str(identifier);
    // SAFETY: UIAccessibilityIdentification's setter on UIView.
    let () = unsafe { msg_send![view, setAccessibilityIdentifier: &*identifier] };
}

/// The phase label XCUITest waits on; one in the root view and one in each
/// presented controller, all showing the same text.
pub(super) fn phase_label(parent: &UIView, mtm: MainThreadMarker) {
    let label = UILabel::new(mtm);
    label.setFrame(CGRect::new(
        CGPoint::new(16.0, 8.0),
        CGSize::new(320.0, 24.0),
    ));
    // SAFETY: a UIColor for the label's text.
    unsafe { label.setTextColor(Some(&UIColor::whiteColor())) };
    set_identifier(&label, "premise-phase");
    if let Some(existing) = LABELS.with_borrow(|labels| labels.first().and_then(|l| l.text())) {
        label.setText(Some(&existing));
    }
    parent.addSubview(&label);
    LABELS.with_borrow_mut(|labels| labels.push(label));
}

pub(super) fn set_phase(phase: &str) {
    let text = NSString::from_str(phase);
    LABELS.with_borrow(|labels| {
        for label in labels {
            label.setText(Some(&text));
        }
    });
    // XCUITest reads the field's placeholder: the field is the one element
    // certain to be in the presented controller's accessibility tree.
    FIELDS.with_borrow(|fields| {
        for field in fields {
            field.setPlaceholder(Some(&text));
        }
    });
    eprintln!("PREMISE_PHASE: {phase}");
}

pub(super) struct Presented {
    pub controller: Retained<UIViewController>,
    pub field: Retained<PremiseField>,
}

/// Presents the prototype over `root`: a popover anchored in `source`, or a
/// page sheet with medium and large detents. The field asks to become first
/// responder in the completion, once its view is in the window.
pub(super) fn present(
    root: &UIViewController,
    source: &UIView,
    sheet: bool,
    completed: impl Fn(bool) + 'static,
    mtm: MainThreadMarker,
) -> Presented {
    let controller = UIViewController::new(mtm);
    let content = UIView::new(mtm);
    content.setBackgroundColor(Some(&UIColor::colorWithRed_green_blue_alpha(
        0.17, 0.16, 0.2, 1.0,
    )));
    controller.setView(Some(&content));
    phase_label(&content, mtm);
    let this = PremiseField::alloc(mtm).set_ivars(());
    let frame = CGRect::new(CGPoint::new(16.0, 40.0), CGSize::new(320.0, 36.0));
    // SAFETY: UITextField's designated initializer.
    let field: Retained<PremiseField> = unsafe { msg_send![super(this), initWithFrame: frame] };
    set_identifier(&field, "premise-search-field");
    if let Some(existing) = LABELS.with_borrow(|labels| labels.first().and_then(|l| l.text())) {
        field.setPlaceholder(Some(&existing));
    }
    FIELDS.with_borrow_mut(|fields| fields.push(field.clone()));
    content.addSubview(&field);
    controller.setPreferredContentSize(CGSize::new(360.0, 240.0));
    if sheet {
        controller.setModalPresentationStyle(UIModalPresentationStyle::PageSheet);
        if let Some(sheet) = controller.sheetPresentationController() {
            let detents = NSArray::from_retained_slice(&[
                UISheetPresentationControllerDetent::mediumDetent(mtm),
                UISheetPresentationControllerDetent::largeDetent(mtm),
            ]);
            sheet.setDetents(&detents);
        }
    } else {
        controller.setModalPresentationStyle(UIModalPresentationStyle::Popover);
        if let Some(popover) = controller.popoverPresentationController() {
            popover.setSourceView(Some(source));
            popover.setSourceRect(CGRect::new(
                CGPoint::new(24.0, 24.0),
                CGSize::new(24.0, 24.0),
            ));
            popover.setPermittedArrowDirections(UIPopoverArrowDirection(0));
        }
    }
    let target = field.clone();
    let completion = RcBlock::new(move || completed(target.becomeFirstResponder()));
    root.presentViewController_animated_completion(&controller, true, Some(&completion));
    Presented { controller, field }
}

pub(super) fn dismiss(presented: &Presented, done: impl Fn() + 'static) {
    let completion = RcBlock::new(done);
    presented
        .controller
        .dismissViewControllerAnimated_completion(true, Some(&completion));
}

pub(super) fn field_text(field: &UITextField) -> String {
    field
        .text()
        .map(|text| text.to_string())
        .unwrap_or_default()
}

pub(super) fn clear(field: &UITextField) {
    field.setText(Some(&NSString::from_str("")));
}

pub(super) fn pasteboard(text: &str, mtm: MainThreadMarker) {
    let _ = mtm;
    // SAFETY: the general pasteboard's string setter, on the main thread.
    unsafe {
        objc2_ui_kit::UIPasteboard::generalPasteboard().setString(Some(&NSString::from_str(text)))
    };
}

/// Asks the scene for a window narrower than 352 points.
pub(super) fn request_narrow(window: &UIWindow) {
    if let Some(restrictions) = window
        .windowScene()
        .and_then(|scene| scene.sizeRestrictions())
    {
        restrictions.setMinimumSize(CGSize::new(300.0, 400.0));
        restrictions.setMaximumSize(CGSize::new(320.0, 4000.0));
    }
}

pub(super) fn width(window: &UIWindow) -> f64 {
    window.bounds().size.width
}

pub(super) fn paste_sel() -> Sel {
    sel!(paste:)
}
pub(super) fn dictation_sel() -> Sel {
    sel!(insertDictationResult:)
}
pub(super) fn dictation_placeholder_sel() -> Sel {
    sel!(insertDictationResultPlaceholder)
}
pub(super) fn dictation_end_sel() -> Sel {
    sel!(dictationRecordingDidEnd)
}

pub(super) fn ensure_class() {
    let _ = PremiseField::class();
}
