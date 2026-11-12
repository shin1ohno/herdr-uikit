//! Q05 mutant-only lookup of the fixture's actual current native object.
//! Production helpers never access this registry or any expected model value.
use std::{cell::RefCell, rc::Rc};

use super::{NativeClient, TextInputView};

thread_local! {
    static INPUT: RefCell<Option<Rc<TextInputView>>> = const { RefCell::new(None) };
}

pub(super) struct Registration(Option<Rc<TextInputView>>);
impl Registration {
    pub(super) fn new(input: Rc<TextInputView>) -> Self {
        Self(INPUT.with_borrow_mut(|slot| slot.replace(input)))
    }
}
impl Drop for Registration {
    fn drop(&mut self) {
        INPUT.with_borrow_mut(|slot| *slot = self.0.take());
    }
}

pub(super) fn capture(fallback: &NativeClient) -> NativeClient {
    let input = INPUT.with_borrow(Clone::clone);
    input.map_or_else(|| fallback.clone(), |input| NativeClient::new(&input))
}
