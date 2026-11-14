//! The production guard also runs against deterministic responder endpoints in tests.

pub(crate) trait SwitchOwner {
    fn begin(&self);
    fn acquired(&self, result: bool);
    fn finish(&self);
}

pub(crate) trait Responder {
    fn acquire(&self) -> bool;
    fn resign(&self) -> bool;
}

pub(crate) struct ResponderSwitch<O: SwitchOwner>(O);

impl<O: SwitchOwner> ResponderSwitch<O> {
    pub(crate) fn new(owner: O) -> Self {
        owner.begin();
        Self(owner)
    }
    pub(crate) fn acquire(&self, view: &impl Responder) -> bool {
        let result = view.acquire();
        self.0.acquired(result);
        result
    }
    pub(crate) fn resign(&self, view: &impl Responder) {
        view.resign();
    }
}

impl<O: SwitchOwner> Drop for ResponderSwitch<O> {
    fn drop(&mut self) {
        self.0.finish();
    }
}

#[cfg(target_os = "ios")]
impl Responder for objc2_ui_kit::UIView {
    fn acquire(&self) -> bool {
        self.becomeFirstResponder()
    }
    fn resign(&self) -> bool {
        self.resignFirstResponder()
    }
}
