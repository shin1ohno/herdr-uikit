use std::{cell::Cell, rc::Rc};

use super::*;

fn counting() -> (Rc<Cell<u32>>, RefCell<FrameCallback>) {
    let frames = Rc::new(Cell::new(0));
    let counter = frames.clone();
    let callback: FrameCallback = Box::new(move || counter.set(counter.get() + 1));
    (frames, RefCell::new(callback))
}

#[test]
fn each_frame_runs_the_callback() {
    let (frames, callback) = counting();
    assert!(fire(&callback));
    assert!(fire(&callback));
    assert_eq!(frames.get(), 2);
}

#[test]
fn a_frame_inside_a_running_frame_is_skipped() {
    let (frames, callback) = counting();
    let running = callback.borrow_mut();
    assert!(!fire(&callback));
    drop(running);
    assert_eq!(frames.get(), 0);
    assert!(fire(&callback));
    assert_eq!(frames.get(), 1);
}
