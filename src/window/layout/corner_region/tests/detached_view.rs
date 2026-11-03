#![allow(
    unsafe_code,
    reason = "a Mach-O test initializer exercises UIKit on the main thread before libtest starts workers"
)]

use super::*;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};

use objc2::{MainThreadMarker, MainThreadOnly};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_ui_kit::UIView;

static DETACHED_INSETS: AtomicU8 = AtomicU8::new(0);
/// The sampled edges (top, left, bottom, right) as f64 bits, for the message.
static DETACHED_EDGES: [AtomicU64; 4] = [const { AtomicU64::new(0) }; 4];

// libtest blocks the main thread while its test workers run, so this UIKit
// check runs before the harness and retains only scalar results.
// SAFETY: __mod_init_func contains C-ABI functions taking no arguments and
// returning nothing. This function has that signature and stores no UIKit
// object beyond its invocation.
#[used]
#[unsafe(link_section = "__DATA,__mod_init_func")]
static CHECK_DETACHED_INSETS: extern "C" fn() = check_detached_insets;

extern "C" fn check_detached_insets() {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let outcome = std::panic::catch_unwind(|| {
        objc2::rc::autoreleasepool(|_| {
            let frame = CGRect::new(CGPoint::ZERO, CGSize::new(400.0, 300.0));
            let view = UIView::initWithFrame(UIView::alloc(mtm), frame);
            sample(&view, view.safeAreaInsets())
        })
    });
    let status = match outcome {
        Ok(edges) => {
            let edges = [edges.top, edges.left, edges.bottom, edges.right];
            for (slot, value) in DETACHED_EDGES.iter().zip(edges) {
                slot.store(value.to_bits(), Ordering::Release);
            }
            if edges.iter().all(|edge| edge.is_finite() && *edge >= 0.0) {
                1
            } else {
                2
            }
        }
        Err(_) => 3,
    };
    DETACHED_INSETS.store(status, Ordering::Release);
}

/// Even with no window, iOS 26.5's corner-adapted region reaches 9 pt past
/// the leading safe area (a corner allowance, not controls), so the check is
/// that sampling cannot panic and yields usable insets, not that it is zero.
#[test]
fn sampling_a_detached_view_does_not_panic_and_yields_usable_insets() {
    let edges: Vec<f64> = DETACHED_EDGES
        .iter()
        .map(|slot| f64::from_bits(slot.load(Ordering::Acquire)))
        .collect();
    assert_eq!(
        DETACHED_INSETS.load(Ordering::Acquire),
        1,
        "sampling a detached UIView must not panic and must give finite, non-negative \
         insets; got (top, left, bottom, right) = {edges:?}"
    );
}
