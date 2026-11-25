//! The search panel's UIKit objects: a `UISearchTextField` subclass (Escape
//! key command), one target object that is the field's, table's and
//! presentation controller's delegate, and the presented view controller.
//!
//! Every callback here comes from UIKit outside GPUI's update and only
//! updates the window's gate or pushes to the session queue; none calls GPUI.

#![allow(
    unsafe_code,
    reason = "defines UIKit subclasses and delegates with objc2's define_class!"
)]

use std::{cell::RefCell, ptr::NonNull, rc::Weak};

use block2::RcBlock;

use objc2::{
    DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send,
    rc::Retained,
    runtime::{AnyObject, NSObject, ProtocolObject, Sel},
    sel,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_foundation::{NSArray, NSIndexPath, NSInteger, NSObjectProtocol, NSSet, NSString};
use objc2_ui_kit::{
    NSIndexPathUIKitAdditions, UIAdaptivePresentationControllerDelegate, UIButton, UIButtonType,
    UIColor, UIControl, UIControlEvents, UIControlState, UIFont, UIKeyboardHIDUsage, UILabel,
    UIModalPresentationStyle, UIPopoverArrowDirection, UIPopoverPresentationControllerDelegate,
    UIPresentationController, UIPress, UIPressesEvent, UIResponder, UIReturnKeyType,
    UIScrollViewDelegate, UISearchTextField, UISheetPresentationControllerDetent, UITableView,
    UITableViewCell, UITableViewCellStyle, UITableViewDataSource, UITableViewDelegate,
    UITableViewStyle, UITextField, UITextFieldDelegate, UITraitCollection, UIView,
    UIViewAutoresizing, UIViewController, UIViewControllerTransitionCoordinator,
    UIViewControllerTransitionCoordinatorContext,
};

use super::{DismissReason, SessionId};
use crate::native_search::{
    NativeSearchPresentation, NativeSearchRequest, NativeSearchResults, NativeSearchStyle,
};
use crate::window::state::Window;

/// The accessibility identifiers Simulator QA presses by.
pub(crate) const FIELD_ID: &str = "native-search-field";
pub(crate) const CANCEL_ID: &str = "native-search-cancel";
pub(crate) const CLEAR_ID: &str = "native-search-clear";
pub(crate) const TABLE_ID: &str = "native-search-results";
const DOT_TAG: NSInteger = 0x5e_a7c4;

// The host-tested Escape decision names UIKit's usage.
const _: () = assert!(super::gate::ESCAPE_USAGE == UIKeyboardHIDUsage::KeyboardEscape.0);

#[cfg(all(target_abi = "sim", feature = "test-support"))]
thread_local! {
    /// Presses the field began and the last one's HID usage, for QA.
    pub(super) static FIELD_PRESSES: std::cell::Cell<(u32, isize)> =
        const { std::cell::Cell::new((0, 0)) };
}

/// Which session and window a UIKit object belongs to.
pub(super) struct Binding {
    pub(super) window: Weak<Window>,
    pub(super) session: SessionId,
}

impl Binding {
    fn live(&self) -> Option<std::rc::Rc<Window>> {
        self.window.upgrade()
    }
}

define_class!(
    // SAFETY: UISearchTextField may be subclassed; instances come from
    // `initWithFrame:` after ivars are set, there is no Drop, and every
    // override keeps the superclass signature.
    #[unsafe(super(UISearchTextField, UITextField, UIControl, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrNativeSearchField"]
    #[ivars = Binding]
    pub(super) struct SearchField;

    impl SearchField {
        // Escape closes only while nothing is composing: then the IME's own
        // Escape cancels the composition. Read from the press, before
        // UITextField handles it: a key command for Escape did not fire while
        // the field was first responder (Simulator, XCUITest hardware key).
        // The press still goes on to UITextField.
        #[unsafe(method(pressesBegan:withEvent:))]
        fn presses_began(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            let mtm = self.mtm();
            let usages = presses
                .iter()
                .filter_map(|press| press.key(mtm).map(|key| key.keyCode().0));
            let dismiss = super::gate::escape_dismisses(usages, marked(self));
            #[cfg(all(target_abi = "sim", feature = "test-support"))]
            for press in presses.iter() {
                let code = press.key(mtm).map_or(-1, |key| key.keyCode().0);
                FIELD_PRESSES.set((FIELD_PRESSES.get().0 + 1, code));
            }
            // SAFETY: forwards UIKit's own arguments to UITextField's method.
            let () = unsafe { msg_send![super(self), pressesBegan: presses, withEvent: event] };
            if dismiss && let Some(window) = self.ivars().live() {
                window.native_modal_dismiss(self.ivars().session, DismissReason::Escape);
            }
        }
    }
);

define_class!(
    // SAFETY: NSObject subclass; its ivars are plain Rust state dropped with
    // it, no Drop, and each protocol method has objc2-ui-kit's signature.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "HerdrNativeSearchTarget"]
    #[ivars = TargetState]
    pub(super) struct Target;

    impl Target {
        #[unsafe(method(herdrSearchCancel:))]
        fn cancel(&self, _sender: Option<&AnyObject>) {
            self.with_window(|window, session| {
                window.native_modal_dismiss(session, DismissReason::Cancel);
            });
        }

        #[unsafe(method(herdrSearchClear:))]
        fn clear(&self, _sender: Option<&AnyObject>) {
            self.with_window(|window, session| window.native_modal_clear_requested(session));
        }

        #[unsafe(method(herdrSearchEditingChanged:))]
        fn editing_changed(&self, sender: Option<&AnyObject>) {
            let Some(field) = sender.and_then(|s| s.downcast_ref::<UITextField>()) else {
                return;
            };
            if marked(field) {
                return;
            }
            let text = committed_text(field);
            self.with_window(|window, session| window.native_modal_query(session, text));
        }
    }

    unsafe impl NSObjectProtocol for Target {}

    unsafe impl UITextFieldDelegate for Target {
        #[unsafe(method(textFieldShouldReturn:))]
        fn should_return(&self, field: &UITextField) -> bool {
            // UIKit does not call this while an IME composes (Return commits
            // the composition then). Checked anyway: a marked field never closes.
            if !marked(field) {
                self.with_window(|window, session| {
                    window.native_modal_dismiss(session, DismissReason::Return);
                });
            }
            false
        }
    }

    unsafe impl UITableViewDataSource for Target {
        #[unsafe(method(numberOfSectionsInTableView:))]
        fn sections(&self, _table: &UITableView) -> NSInteger {
            self.ivars().results.borrow().sections.len() as NSInteger
        }

        #[unsafe(method(tableView:numberOfRowsInSection:))]
        fn rows(&self, _table: &UITableView, section: NSInteger) -> NSInteger {
            let results = self.ivars().results.borrow();
            usize::try_from(section)
                .ok()
                .and_then(|index| results.sections.get(index))
                .map_or(0, |section| section.items.len() as NSInteger)
        }

        #[unsafe(method_id(tableView:titleForHeaderInSection:))]
        fn header(&self, _table: &UITableView, section: NSInteger) -> Option<Retained<NSString>> {
            let results = self.ivars().results.borrow();
            usize::try_from(section)
                .ok()
                .and_then(|index| results.sections.get(index))
                .map(|section| NSString::from_str(&section.title))
        }

        #[unsafe(method_id(tableView:cellForRowAtIndexPath:))]
        fn cell(&self, table: &UITableView, path: &NSIndexPath) -> Retained<UITableViewCell> {
            self.make_cell(table, path)
        }
    }

    unsafe impl UITableViewDelegate for Target {
        #[unsafe(method(tableView:didSelectRowAtIndexPath:))]
        fn selected(&self, table: &UITableView, path: &NSIndexPath) {
            table.deselectRowAtIndexPath_animated(path, true);
            let id = self.item_id(path);
            let revision = self.ivars().revision.get();
            if let Some(id) = id {
                self.with_window(|window, session| {
                    window.native_modal_selected(session, id, revision);
                });
            }
        }
    }

    unsafe impl UIScrollViewDelegate for Target {}

    unsafe impl UIAdaptivePresentationControllerDelegate for Target {
        // A popover stays a popover in compact width; the consumer asks for
        // a sheet itself when it wants one.
        #[unsafe(method(adaptivePresentationStyleForPresentationController:traitCollection:))]
        fn adaptive(
            &self,
            _controller: &UIPresentationController,
            _traits: &UITraitCollection,
        ) -> UIModalPresentationStyle {
            UIModalPresentationStyle::None
        }

        // A user dismissal begins: a tap outside the popover or a swipe of
        // the sheet. An abandoned swipe ends here:
        // no didDismiss follows, the sheet and the session stay.
        #[unsafe(method(presentationControllerWillDismiss:))]
        fn will_dismiss(&self, controller: &UIPresentationController) {
            let state = self.ivars();
            state
                .dismissals_begun
                .set(state.dismissals_begun.get().saturating_add(1));
            let Some(coordinator) = controller.presentedViewController().transitionCoordinator()
            else {
                return;
            };
            // UIKit resigned the field when the swipe began and does not give
            // first responder back when the swipe is abandoned.
            let window = state.binding.window.clone();
            let session = state.binding.session;
            let completion = RcBlock::new(
                move |context: NonNull<
                    ProtocolObject<dyn UIViewControllerTransitionCoordinatorContext>,
                >| {
                    // SAFETY: UIKit passes a live context for the completion.
                    let cancelled = unsafe { context.as_ref() }.isCancelled();
                    if let Some(window) = window.upgrade().filter(|_| cancelled) {
                        window.native_modal_dismissal_cancelled(session);
                    }
                },
            );
            coordinator.animateAlongsideTransition_completion(None, Some(&completion));
        }

        // Only user-driven dismissals arrive here (an outside tap, a finished
        // swipe); programmatic ones complete through our own completion.
        #[unsafe(method(presentationControllerDidDismiss:))]
        fn did_dismiss(&self, _controller: &UIPresentationController) {
            let reason = match self.ivars().presentation {
                NativeSearchPresentation::Popover => DismissReason::OutsideTap,
                NativeSearchPresentation::Sheet => DismissReason::SwipedDown,
            };
            self.with_window(|window, session| window.native_modal_dismissed(session, reason));
        }
    }

    unsafe impl UIPopoverPresentationControllerDelegate for Target {}
);

pub(super) struct TargetState {
    binding: Binding,
    presentation: NativeSearchPresentation,
    style: NativeSearchStyle,
    pub(super) results: RefCell<NativeSearchResults>,
    pub(super) revision: std::cell::Cell<u64>,
    /// User dismissals UIKit began (finished or abandoned), for QA.
    pub(super) dismissals_begun: std::cell::Cell<u32>,
}

impl Target {
    fn with_window(&self, f: impl FnOnce(&Window, SessionId)) {
        let binding = &self.ivars().binding;
        if let Some(window) = binding.live() {
            f(&window, binding.session);
        }
    }

    fn item_id(&self, path: &NSIndexPath) -> Option<u64> {
        let results = self.ivars().results.borrow();
        let section = results
            .sections
            .get(usize::try_from(path.section()).ok()?)?;
        Some(section.items.get(usize::try_from(path.row()).ok()?)?.id)
    }

    fn make_cell(&self, table: &UITableView, path: &NSIndexPath) -> Retained<UITableViewCell> {
        let mtm = self.mtm();
        let reuse = NSString::from_str("herdr-native-search-row");
        let cell = table
            .dequeueReusableCellWithIdentifier(&reuse)
            .unwrap_or_else(|| {
                UITableViewCell::initWithStyle_reuseIdentifier(
                    UITableViewCell::alloc(mtm),
                    UITableViewCellStyle::Subtitle,
                    Some(&reuse),
                )
            });
        let style = self.ivars().style;
        let results = self.ivars().results.borrow();
        let item = usize::try_from(path.section())
            .ok()
            .and_then(|s| results.sections.get(s))
            .and_then(|s| {
                usize::try_from(path.row())
                    .ok()
                    .and_then(|r| s.items.get(r))
            });
        cell.setBackgroundColor(Some(&color(style.background)));
        #[allow(deprecated, reason = "the subtitle style's labels suffice here")]
        {
            if let Some(label) = cell.textLabel() {
                label.setText(item.map(|i| NSString::from_str(&i.title)).as_deref());
                // SAFETY: a UIColor and a UIFont for the label.
                unsafe {
                    label.setTextColor(Some(&color(style.text)));
                    label.setFont(Some(&UIFont::systemFontOfSize(f64::from(style.body_size))));
                }
            }
            if let Some(label) = cell.detailTextLabel() {
                label.setText(item.map(|i| NSString::from_str(&i.detail)).as_deref());
                // SAFETY: as above.
                unsafe {
                    label.setTextColor(Some(&color(style.secondary_text)));
                    label.setFont(Some(&UIFont::systemFontOfSize(f64::from(
                        style.caption_size,
                    ))));
                }
            }
        }
        cell.setIndentationLevel(1);
        cell.setIndentationWidth(18.0);
        let content = cell.contentView();
        let dot = content.viewWithTag(DOT_TAG).unwrap_or_else(|| {
            let dot = UIView::initWithFrame(
                UIView::alloc(mtm),
                CGRect::new(CGPoint::new(12.0, 17.0), CGSize::new(10.0, 10.0)),
            );
            dot.setTag(DOT_TAG);
            dot.layer().setCornerRadius(5.0);
            content.addSubview(&dot);
            dot
        });
        match item.and_then(|i| i.dot) {
            Some(rgba) => {
                dot.setHidden(false);
                dot.setBackgroundColor(Some(&color(rgba)));
            }
            None => dot.setHidden(true),
        }
        if let Some(item) = item {
            set_accessibility_label(&cell, &item.accessibility_label);
        }
        cell
    }
}

pub(crate) fn color(rgba: gpui::Rgba) -> Retained<UIColor> {
    UIColor::colorWithRed_green_blue_alpha(
        f64::from(rgba.r),
        f64::from(rgba.g),
        f64::from(rgba.b),
        f64::from(rgba.a),
    )
}

/// Whether the field holds marked (composing) text.
pub(crate) fn marked(field: &UITextField) -> bool {
    // SAFETY: UITextInput's getter on UITextField.
    let range: Option<Retained<AnyObject>> = unsafe { msg_send![field, markedTextRange] };
    range.is_some()
}

/// The field's text without its marked range: the committed query.
pub(crate) fn committed_text(field: &UITextField) -> String {
    let text = field.text().map(|t| t.to_string()).unwrap_or_default();
    // SAFETY: UITextInput's getters on UITextField; offsets are UTF-16.
    let (start, length): (NSInteger, NSInteger) = unsafe {
        let range: Option<Retained<AnyObject>> = msg_send![field, markedTextRange];
        let Some(range) = range else {
            return text;
        };
        let begin: Retained<AnyObject> = msg_send![field, beginningOfDocument];
        let from: Retained<AnyObject> = msg_send![&*range, start];
        let to: Retained<AnyObject> = msg_send![&*range, end];
        let start: NSInteger = msg_send![field, offsetFromPosition: &*begin, toPosition: &*from];
        let end: NSInteger = msg_send![field, offsetFromPosition: &*begin, toPosition: &*to];
        (start, end - start)
    };
    let units: Vec<u16> = text.encode_utf16().collect();
    let (Ok(start), Ok(length)) = (usize::try_from(start), usize::try_from(length)) else {
        return text;
    };
    let end = start.saturating_add(length).min(units.len());
    let start = start.min(end);
    let mut kept = units[..start].to_vec();
    kept.extend_from_slice(&units[end..]);
    String::from_utf16_lossy(&kept)
}

pub(crate) fn set_identifier(view: &UIView, identifier: &str) {
    let identifier = NSString::from_str(identifier);
    // SAFETY: UIAccessibilityIdentification's setter on UIView.
    let () = unsafe { msg_send![view, setAccessibilityIdentifier: &*identifier] };
}

pub(crate) fn set_accessibility_label(view: &UIView, text: &str) {
    let text = NSString::from_str(text);
    // SAFETY: UIAccessibility's informal-protocol setter on UIView.
    let () = unsafe { msg_send![view, setAccessibilityLabel: &*text] };
}

fn label(
    text: &str,
    size: f32,
    rgba: gpui::Rgba,
    frame: CGRect,
    mtm: MainThreadMarker,
) -> Retained<UILabel> {
    let label = UILabel::initWithFrame(UILabel::alloc(mtm), frame);
    label.setText(Some(&NSString::from_str(text)));
    // SAFETY: a UIColor and a UIFont for the label.
    unsafe {
        label.setTextColor(Some(&color(rgba)));
        label.setFont(Some(&UIFont::systemFontOfSize(f64::from(size))));
    }
    label.setAutoresizingMask(UIViewAutoresizing::FlexibleWidth);
    label
}

fn button(
    title: &str,
    id: &str,
    target: &Target,
    action: Sel,
    frame: CGRect,
    mtm: MainThreadMarker,
) -> Retained<UIButton> {
    let button = UIButton::buttonWithType(UIButtonType::System, mtm);
    button.setFrame(frame);
    button.setTitle_forState(Some(&NSString::from_str(title)), UIControlState::Normal);
    set_identifier(&button, id);
    // SAFETY: `target` implements `action`, taking the sender; UIKit keeps
    // a weak reference and the panel keeps the target alive.
    unsafe {
        button.addTarget_action_forControlEvents(
            Some(target),
            action,
            UIControlEvents::TouchUpInside,
        );
    }
    button
}

/// The presented controller and the views the window updates.
pub(crate) struct Panel {
    pub(super) controller: Retained<UIViewController>,
    pub(super) field: Retained<SearchField>,
    table: Retained<UITableView>,
    status: Retained<UILabel>,
    message: Retained<UILabel>,
    empty: Retained<UIView>,
    empty_label: Retained<UILabel>,
    target: Retained<Target>,
}

impl Panel {
    /// User dismissals UIKit began on this panel, finished or abandoned.
    #[cfg(all(target_abi = "sim", feature = "test-support"))]
    pub(super) fn dismissals_begun(&self) -> u32 {
        self.target.ivars().dismissals_begun.get()
    }
}

impl Panel {
    pub(super) fn new(
        binding: impl Fn() -> Binding,
        request: &NativeSearchRequest,
        mtm: MainThreadMarker,
    ) -> Self {
        let style = request.style;
        let width = f64::from(request.width.max(240.0));
        let height = f64::from(request.height.max(200.0));
        let target = Target::alloc(mtm).set_ivars(TargetState {
            binding: binding(),
            presentation: request.presentation,
            style,
            results: RefCell::new(request.results.clone()),
            revision: std::cell::Cell::new(0),
            dismissals_begun: std::cell::Cell::new(0),
        });
        // SAFETY: NSObject's designated initializer after the ivars.
        let target: Retained<Target> = unsafe { msg_send![super(target), init] };

        let controller = UIViewController::new(mtm);
        let content = UIView::initWithFrame(
            UIView::alloc(mtm),
            CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(width, height)),
        );
        content.setBackgroundColor(Some(&color(style.background)));
        controller.setView(Some(&content));
        controller.setPreferredContentSize(CGSize::new(width, height));

        let title = label(
            &request.title,
            style.title_size,
            style.text,
            CGRect::new(CGPoint::new(16.0, 12.0), CGSize::new(width - 120.0, 24.0)),
            mtm,
        );
        content.addSubview(&title);
        let cancel = button(
            &request.cancel_label,
            CANCEL_ID,
            &target,
            sel!(herdrSearchCancel:),
            CGRect::new(CGPoint::new(width - 96.0, 8.0), CGSize::new(80.0, 32.0)),
            mtm,
        );
        set_accessibility_label(&cancel, &request.cancel_accessibility_label);
        cancel.setAutoresizingMask(UIViewAutoresizing::FlexibleLeftMargin);
        content.addSubview(&cancel);

        let field = SearchField::alloc(mtm).set_ivars(binding());
        let frame = CGRect::new(CGPoint::new(16.0, 48.0), CGSize::new(width - 32.0, 36.0));
        // SAFETY: UITextField's designated initializer after the ivars.
        let field: Retained<SearchField> = unsafe { msg_send![super(field), initWithFrame: frame] };
        set_identifier(&field, FIELD_ID);
        field.setAutoresizingMask(UIViewAutoresizing::FlexibleWidth);
        field.setText(Some(&NSString::from_str(&request.query)));
        field.setPlaceholder(Some(&NSString::from_str(&request.placeholder)));
        field.setBackgroundColor(Some(&color(style.field_background)));
        field.setTextColor(Some(&color(style.text)));
        // SAFETY: a UIColor for the caret.
        unsafe { field.setTintColor(Some(&color(style.accent))) };
        set_return_key_type(&field, UIReturnKeyType::Search);
        field.setDelegate(Some(ProtocolObject::from_ref(&*target)));
        // SAFETY: as for the buttons, for the field's editing events.
        unsafe {
            field.addTarget_action_forControlEvents(
                Some(&*target),
                sel!(herdrSearchEditingChanged:),
                UIControlEvents::EditingChanged,
            );
        }
        content.addSubview(&field);

        let status = label(
            "",
            style.caption_size,
            style.secondary_text,
            CGRect::new(CGPoint::new(16.0, 92.0), CGSize::new(width - 32.0, 18.0)),
            mtm,
        );
        content.addSubview(&status);
        let message = label(
            "",
            style.caption_size,
            style.error,
            CGRect::new(CGPoint::new(16.0, 112.0), CGSize::new(width - 32.0, 18.0)),
            mtm,
        );
        content.addSubview(&message);

        let list = CGRect::new(CGPoint::new(0.0, 134.0), CGSize::new(width, height - 134.0));
        let table = UITableView::initWithFrame_style(
            UITableView::alloc(mtm),
            list,
            UITableViewStyle::Plain,
        );
        set_identifier(&table, TABLE_ID);
        table.setAutoresizingMask(
            UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
        );
        table.setBackgroundColor(Some(&color(style.background)));
        table.setRowHeight(44.0);
        // SAFETY: the target outlives the table (the panel holds both).
        unsafe {
            table.setDataSource(Some(ProtocolObject::from_ref(&*target)));
            table.setDelegate(Some(ProtocolObject::from_ref(&*target)));
        }
        content.addSubview(&table);

        let empty = UIView::initWithFrame(UIView::alloc(mtm), list);
        empty.setAutoresizingMask(
            UIViewAutoresizing::FlexibleWidth | UIViewAutoresizing::FlexibleHeight,
        );
        let empty_label = label(
            "",
            style.body_size,
            style.secondary_text,
            CGRect::new(CGPoint::new(16.0, 16.0), CGSize::new(width - 32.0, 20.0)),
            mtm,
        );
        empty.addSubview(&empty_label);
        let clear = button(
            "",
            CLEAR_ID,
            &target,
            sel!(herdrSearchClear:),
            CGRect::new(CGPoint::new(16.0, 44.0), CGSize::new(160.0, 32.0)),
            mtm,
        );
        empty.addSubview(&clear);
        content.addSubview(&empty);

        let panel = Self {
            controller,
            field,
            table,
            status,
            message,
            empty,
            empty_label,
            target,
        };
        panel.show(&request.results, Some(&clear));
        panel
    }

    /// Configures the presentation; `source` is the drawing view.
    pub(super) fn configure(&self, request: &NativeSearchRequest, source: &UIView) {
        let delegate = ProtocolObject::from_ref(&*self.target);
        match request.presentation {
            NativeSearchPresentation::Popover => {
                self.controller
                    .setModalPresentationStyle(UIModalPresentationStyle::Popover);
                if let Some(popover) = self.controller.popoverPresentationController() {
                    popover.setSourceView(Some(source));
                    let anchor = request.anchor;
                    popover.setSourceRect(CGRect::new(
                        CGPoint::new(f64::from(anchor.origin.x), f64::from(anchor.origin.y)),
                        CGSize::new(f64::from(anchor.size.width), f64::from(anchor.size.height)),
                    ));
                    popover.setPermittedArrowDirections(UIPopoverArrowDirection(0));
                    popover.setBackgroundColor(Some(&color(request.style.background)));
                    // SAFETY: the target outlives the presentation.
                    unsafe { popover.setDelegate(Some(delegate)) };
                }
            }
            NativeSearchPresentation::Sheet => {
                self.controller
                    .setModalPresentationStyle(UIModalPresentationStyle::PageSheet);
                if let Some(sheet) = self.controller.sheetPresentationController() {
                    let mtm = self.controller.mtm();
                    sheet.setDetents(&NSArray::from_retained_slice(&[
                        UISheetPresentationControllerDetent::mediumDetent(mtm),
                        UISheetPresentationControllerDetent::largeDetent(mtm),
                    ]));
                    sheet.setPrefersGrabberVisible(true);
                }
                if let Some(presentation) = self.controller.presentationController() {
                    // SAFETY: as above.
                    unsafe {
                        presentation.setDelegate(Some(ProtocolObject::from_ref(&*self.target)));
                    }
                }
            }
        }
    }

    /// Shows `results`; `revision` numbers them for `Selected`.
    pub(super) fn update(&self, results: NativeSearchResults, revision: u64) {
        self.target.ivars().revision.set(revision);
        self.show(&results, None);
        *self.target.ivars().results.borrow_mut() = results;
        self.table.reloadData();
    }

    /// Replaces the field's text. A programmatic `setText:` sends no
    /// editing-changed action, so no `QueryChanged` follows.
    pub(super) fn set_query(&self, query: &str) {
        self.field.setText(Some(&NSString::from_str(query)));
    }

    fn show(&self, results: &NativeSearchResults, clear: Option<&UIButton>) {
        let text = |value: &Option<String>| value.as_deref().map(NSString::from_str);
        self.status.setText(text(&results.status).as_deref());
        self.message.setText(text(&results.message).as_deref());
        self.message.setHidden(results.message.is_none());
        let empty = results.empty.as_ref();
        self.empty.setHidden(empty.is_none());
        self.table.setHidden(empty.is_some());
        if let Some(empty) = empty {
            self.empty_label
                .setText(Some(&NSString::from_str(&empty.message)));
            let clear_button = clear.map(|b| b.retain()).or_else(|| {
                self.empty
                    .subviews()
                    .iter()
                    .find_map(|view| view.downcast::<UIButton>().ok())
            });
            if let Some(button) = clear_button {
                button.setTitle_forState(
                    Some(&NSString::from_str(&empty.clear_label)),
                    UIControlState::Normal,
                );
            }
        }
    }

    pub(super) fn committed_text(&self) -> String {
        committed_text(&self.field)
    }
}

/// `UITextField` answers the `UITextInputTraits` setters by forwarding: the
/// class has no `setReturnKeyType:` method, so objc2's debug signature check
/// panics on `setReturnKeyType`. Send the message as Objective-C does.
pub(crate) fn set_return_key_type(field: &UITextField, value: UIReturnKeyType) {
    // SAFETY: `-[UITextField setReturnKeyType:]` takes one `NSInteger` and
    // returns nothing; `objc_msgSend` is called with exactly that signature on
    // a live text field, on the main thread like every caller of this module.
    unsafe {
        let send: unsafe extern "C-unwind" fn(*const AnyObject, Sel, NSInteger) =
            std::mem::transmute(objc2::ffi::objc_msgSend as unsafe extern "C-unwind" fn());
        send(
            (field as *const UITextField).cast(),
            sel!(setReturnKeyType:),
            value.0,
        );
    }
}
