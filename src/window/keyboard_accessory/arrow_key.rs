//! The flick arrow key (`KeyboardBarItem::arrows`): a `UIControl` that
//! tracks one touch through `crate::keyboard_bar::arrow::Tracker` and queues
//! the arrow it sends, on the accessory row and as the shortcuts bar item's
//! custom view. Its face is a gray `UIButton` like the row's other keys,
//! showing the four-way arrow, or the arrow the finger means now. The repeat
//! timer is a `performSelector:withObject:afterDelay:` request, as the text
//! input view's key repeat is (`text_input/run_loop.rs`).
#![allow(
    unsafe_code,
    reason = "an Objective-C UIControl subclass, and NSObject's delayed-perform methods"
)]

use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use objc2::rc::Retained;
use objc2::runtime::{AnyObject, NSObject};
use objc2::{
    ClassType, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send, sel,
};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_ui_kit::{
    UIButton, UIButtonConfiguration, UIControl, UIEvent, UIResponder, UITouch, UIView, UIWindow,
};

use crate::keyboard_bar::KeyboardArrows;
use crate::keyboard_bar::PressQueue;
use crate::keyboard_bar::arrow::{Arrow, Timer, Tracker};

/// The key's size as a shortcuts bar item, which UIKit reads from
/// `intrinsicContentSize`; on the accessory row the row sizes it.
const BAR_SIZE: CGSize = CGSize {
    width: 52.0,
    height: 36.0,
};

/// The four-way arrow, while no arrow is meant.
const IDLE_SYMBOL: &str = "arrow.up.and.down.and.arrow.left.and.right";

pub(in crate::window) struct Ivars {
    tracker: Cell<Tracker>,
    /// The finger's last point in the key's own coordinates.
    point: Cell<CGPoint>,
    arrows: KeyboardArrows,
    queue: Rc<RefCell<PressQueue>>,
    face: Retained<UIButton>,
    /// The arrow the face shows.
    shown: Cell<Option<Arrow>>,
}

define_class!(
    // SAFETY: UIControl's subclassing rules are kept: instances are created
    // through `initWithFrame:` (`HerdrArrowKey::new`), the class does not
    // implement `Drop`, and every override keeps the superclass signature.
    #[unsafe(super(UIControl, UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    pub(in crate::window) struct HerdrArrowKey;

    impl HerdrArrowKey {
        #[unsafe(method(beginTrackingWithTouch:withEvent:))]
        fn __begin_tracking(&self, touch: &UITouch, _event: Option<&UIEvent>) -> bool {
            let _native_entry = crate::native_operation::raw("keyboard-arrow-touch");
            let mut tracker = Tracker::default();
            let timer = tracker.touch(self.size(), self.locate(touch));
            self.ivars().tracker.set(tracker);
            self.follow(timer);
            true
        }

        #[unsafe(method(continueTrackingWithTouch:withEvent:))]
        fn __continue_tracking(&self, touch: &UITouch, _event: Option<&UIEvent>) -> bool {
            let _native_entry = crate::native_operation::raw("keyboard-arrow-touch");
            let mut tracker = self.ivars().tracker.get();
            let timer = tracker.touch(self.size(), self.locate(touch));
            self.ivars().tracker.set(tracker);
            self.follow(timer);
            true
        }

        #[unsafe(method(endTrackingWithTouch:withEvent:))]
        fn __end_tracking(&self, touch: Option<&UITouch>, _event: Option<&UIEvent>) {
            let _native_entry = crate::native_operation::raw("keyboard-arrow-press");
            let point = touch.map_or_else(
                || {
                    let point = self.ivars().point.get();
                    (point.x, point.y)
                },
                |touch| self.locate(touch),
            );
            let mut tracker = self.ivars().tracker.get();
            let sent = tracker.release(self.size(), point);
            self.ivars().tracker.set(tracker);
            self.follow(Timer::Stop);
            if let Some(arrow) = sent {
                self.send(arrow);
            }
        }

        #[unsafe(method(cancelTrackingWithEvent:))]
        fn __cancel_tracking(&self, _event: Option<&UIEvent>) {
            let _native_entry = crate::native_operation::raw("keyboard-arrow-touch");
            self.stop();
        }

        // The target of the repeat timer's delayed perform.
        #[unsafe(method(herdrArrowRepeat:))]
        fn __repeat(&self, _: Option<&AnyObject>) {
            let _native_entry = crate::native_operation::raw("keyboard-arrow-press");
            let point = self.ivars().point.get();
            let mut tracker = self.ivars().tracker.get();
            let (sent, timer) = tracker.fire(self.size(), (point.x, point.y));
            self.ivars().tracker.set(tracker);
            self.follow(timer);
            if let Some(arrow) = sent {
                self.send(arrow);
            }
        }

        // A key leaving its window (the keyboard hid, the row was replaced)
        // stops repeating.
        #[unsafe(method(willMoveToWindow:))]
        fn __will_move_to_window(&self, window: Option<&UIWindow>) {
            if window.is_none() {
                self.stop();
            }
            // SAFETY: the argument is UIKit's nullable UIWindow for this
            // selector; UIView's implementation returns nothing.
            let () = unsafe { msg_send![super(self), willMoveToWindow: window] };
        }

        #[unsafe(method(layoutSubviews))]
        fn __layout_subviews(&self) {
            // SAFETY: `layoutSubviews` takes and returns nothing.
            let () = unsafe { msg_send![super(self), layoutSubviews] };
            self.ivars().face.setFrame(self.bounds());
        }

        #[unsafe(method(intrinsicContentSize))]
        fn __intrinsic_content_size(&self) -> CGSize {
            BAR_SIZE
        }
    }
);

impl HerdrArrowKey {
    /// A key that queues `arrows`' ids on `queue`.
    pub(in crate::window) fn new(
        arrows: KeyboardArrows,
        queue: &Rc<RefCell<PressQueue>>,
        mtm: MainThreadMarker,
    ) -> Retained<Self> {
        let face = UIButton::buttonWithConfiguration_primaryAction(&face(None, mtm), None);
        // The control takes the touches; the face only draws.
        face.setUserInteractionEnabled(false);
        let this = Self::alloc(mtm).set_ivars(Ivars {
            tracker: Cell::new(Tracker::default()),
            point: Cell::new(CGPoint::ZERO),
            arrows,
            queue: Rc::clone(queue),
            face: face.clone(),
            shown: Cell::new(None),
        });
        let frame = CGRect {
            origin: CGPoint::ZERO,
            size: BAR_SIZE,
        };
        // SAFETY: `initWithFrame:` is UIView's designated initializer, and the
        // ivars were set before it runs.
        let this: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        this.addSubview(&face);
        // SAFETY: NSObject's UIAccessibility category; the setter takes a BOOL.
        let () = unsafe { msg_send![&*this, setIsAccessibilityElement: true] };
        this
    }

    /// Keeps the key as wide as `BAR_SIZE` as a shortcuts bar item's custom view:
    /// UIKit narrows custom views to fit a crowded bar (12.5 pt in one run,
    /// yebisu#276 r12), too narrow to flick.
    pub(in crate::window) fn hold_bar_width(&self) {
        self.widthAnchor()
            .constraintEqualToConstant(BAR_SIZE.width)
            .setActive(true);
    }

    /// The ids this key sends.
    pub(in crate::window) fn arrows(&self) -> KeyboardArrows {
        self.ivars().arrows
    }

    fn size(&self) -> (f64, f64) {
        let size = self.bounds().size;
        (size.width, size.height)
    }

    /// `touch`'s point in the key's coordinates, recorded for the timer.
    fn locate(&self, touch: &UITouch) -> (f64, f64) {
        let point = touch.locationInView(Some(self));
        self.ivars().point.set(point);
        (point.x, point.y)
    }

    /// Applies `timer` to the delayed repeat, then draws the meant arrow.
    fn follow(&self, timer: Timer) {
        match timer {
            Timer::Keep => {}
            Timer::Stop => self.cancel_repeat(),
            Timer::After(delay) => {
                self.cancel_repeat();
                // SAFETY: `herdrArrowRepeat:` is a method of this class that
                // takes one optional object and returns nothing, which is what
                // `performSelector:withObject:afterDelay:` calls.
                let () = unsafe {
                    msg_send![
                        self,
                        performSelector: sel!(herdrArrowRepeat:),
                        withObject: None::<&AnyObject>,
                        afterDelay: delay,
                    ]
                };
            }
        }
        self.show(self.ivars().tracker.get().arrow());
    }

    fn cancel_repeat(&self) {
        let target: &AnyObject = self;
        // SAFETY: this class method takes the target, the selector and the
        // argument the request was queued with (none), and returns nothing.
        let () = unsafe {
            msg_send![
                NSObject::class(),
                cancelPreviousPerformRequestsWithTarget: target,
                selector: sel!(herdrArrowRepeat:),
                object: None::<&AnyObject>,
            ]
        };
    }

    /// Ends the touch without sending anything.
    fn stop(&self) {
        let mut tracker = self.ivars().tracker.get();
        tracker.cancel();
        self.ivars().tracker.set(tracker);
        self.follow(Timer::Stop);
    }

    fn send(&self, arrow: Arrow) {
        let id = self.ivars().arrows.id(arrow);
        let Ok(mut queue) = self.ivars().queue.try_borrow_mut() else {
            tracing::warn!(id, "arrow key press while its queue is borrowed; dropped");
            return;
        };
        let queued = queue.push(id);
        tracing::debug!(id, ?arrow, queued, "arrow key press");
    }

    fn show(&self, arrow: Option<Arrow>) {
        let ivars = self.ivars();
        if ivars.shown.replace(arrow) == arrow {
            return;
        }
        ivars.face.setConfiguration(Some(&face(arrow, self.mtm())));
        ivars.face.setHighlighted(arrow.is_some());
    }
}

/// The face's look: the four-way arrow while idle, the meant arrow filled.
fn face(arrow: Option<Arrow>, mtm: MainThreadMarker) -> Retained<UIButtonConfiguration> {
    let name = match arrow {
        None => IDLE_SYMBOL,
        Some(Arrow::Up) => "arrowtriangle.up.fill",
        Some(Arrow::Down) => "arrowtriangle.down.fill",
        Some(Arrow::Left) => "arrowtriangle.left.fill",
        Some(Arrow::Right) => "arrowtriangle.right.fill",
    };
    let config = if arrow.is_some() {
        UIButtonConfiguration::tintedButtonConfiguration(mtm)
    } else {
        UIButtonConfiguration::grayButtonConfiguration(mtm)
    };
    config.setImage(super::super::keyboard_bar::symbol(Some(name)).as_deref());
    super::key_style(&config);
    config
}
