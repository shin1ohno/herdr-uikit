//! The per-window native modal gate and the session it guards: the native
//! search panel's or the inline field's (`super::native_inline_field`). Both
//! kinds share the one gate, so they are never up together.
//!
//! While the gate is shut, every reader listed in `docs/native-search.md`
//! keeps GPUI out: first responder acquisition (`update_first_responder`,
//! the text view's flush and `didMoveToWindow`), the keyboard's show/hide
//! requests and notifications, GPUI's view presses (fenced per key), the
//! input handler and edit dispatch, Edit menus, and the app's menu commands
//! (performed, deferred and validated). It opens again only once a GPUI view
//! has actually become first responder after the consumer released its
//! handle (`InputRestored`).

mod fence;
pub(super) mod gate;
mod intent;
mod panel;
mod queue;

use std::rc::Rc;

use gpui::AnyWindowHandle;
use objc2::{MainThreadMarker, Message, rc::Retained};
use objc2_ui_kit::{UIView, UIViewController};

pub(crate) use self::fence::Fence;
pub use self::gate::{DismissReason, PresentFailure, Responder};
pub(crate) use self::gate::{Effect, Gate, Note, Phase, SessionId, command_enabled, command_runs};
pub(crate) use self::intent::{Intent, Request as IntentRequest};
use self::panel::{Binding, Panel};
pub(crate) use self::panel::{
    color, committed_text, marked, set_accessibility_label, set_identifier, set_return_key_type,
};
pub(crate) use self::queue::{Coalesce, Queue};
use super::{keyboard::ResignReason, native_inline_field::Inline, scene, state::Window};
use crate::native_search::{
    NativeSearchError, NativeSearchEvent, NativeSearchRequest, NativeSearchResults, SharedQueue,
};

/// How long a restore attempt may take before it is retried.
const RESTORE_CHECK: std::time::Duration = std::time::Duration::from_millis(500);

/// The live session's consumer side, beside the window's gate: the search
/// panel's or the inline field's.
pub(crate) enum Session {
    Search(Search),
    Inline(Inline),
}

/// The search panel's consumer side.
pub(crate) struct Search {
    id: SessionId,
    queue: SharedQueue,
    panel: Option<Rc<Panel>>,
    last_query: String,
    revision: u64,
    window: std::rc::Weak<Window>,
}

impl Session {
    pub(super) fn id(&self) -> SessionId {
        match self {
            Self::Search(s) => s.id,
            Self::Inline(s) => s.id,
        }
    }
    fn window(&self) -> &std::rc::Weak<Window> {
        match self {
            Self::Search(s) => &s.window,
            Self::Inline(s) => &s.window,
        }
    }
    fn search(&self) -> Option<&Search> {
        match self {
            Self::Search(s) => Some(s),
            Self::Inline(_) => None,
        }
    }
    fn search_mut(&mut self) -> Option<&mut Search> {
        match self {
            Self::Search(s) => Some(s),
            Self::Inline(_) => None,
        }
    }
    pub(super) fn inline(&self) -> Option<&Inline> {
        match self {
            Self::Inline(s) => Some(s),
            Self::Search(_) => None,
        }
    }
    pub(super) fn inline_mut(&mut self) -> Option<&mut Inline> {
        match self {
            Self::Inline(s) => Some(s),
            Self::Search(_) => None,
        }
    }
    /// The presented controller and its field, once UIKit was asked.
    fn presented(&self) -> Option<(Retained<UIViewController>, Retained<UIView>)> {
        match self {
            Self::Search(s) => s.panel.as_ref().map(|panel| {
                let field: &UIView = &panel.field;
                (panel.controller.clone(), field.retain())
            }),
            Self::Inline(s) => s
                .controller
                .as_ref()
                .map(|c| (c.controller.clone(), c.field_view().retain())),
        }
    }
}

/// Why a native modal session could not open.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpenError {
    AlreadyPresented,
    RetirementInProgress,
    WindowUnavailable,
}

impl From<OpenError> for NativeSearchError {
    fn from(error: OpenError) -> Self {
        match error {
            OpenError::WindowUnavailable => Self::WindowUnavailable,
            OpenError::AlreadyPresented => Self::AlreadyPresented,
            OpenError::RetirementInProgress => Self::RetirementInProgress,
        }
    }
}

impl super::state::State {
    /// No GPUI view may take first responder.
    pub(super) fn modal_holds_responder(&self) -> bool {
        matches!(self.native_modal.phase(), Some((_, phase)) if phase != Phase::Restoring)
    }
    /// Nothing may reach GPUI's input handler, key events, menus or edits.
    pub(super) fn modal_blocks_input(&self) -> bool {
        self.native_modal.blocks()
    }
}

pub(super) fn window(handle: AnyWindowHandle) -> Option<Rc<Window>> {
    scene::menu_window(handle).filter(|window| !window.state.borrow().closed)
}

pub(crate) fn active(handle: AnyWindowHandle, _mtm: MainThreadMarker) -> Option<bool> {
    window(handle).map(|window| window.state.borrow().modal_blocks_input())
}

/// Whether `handle`'s gate is shut (`false` for a window that is not open).
pub(crate) fn blocks(handle: AnyWindowHandle) -> bool {
    window(handle).is_some_and(|window| window.state.borrow().modal_blocks_input())
}

/// The window whose UIWindow is key: where UIKit delivers key events.
fn key_window(mtm: MainThreadMarker) -> Option<Rc<Window>> {
    scene::live_windows(mtm).into_iter().find(|window| {
        let state = window.state.borrow();
        !state.closed && state.ui_window.as_ref().is_some_and(|w| w.isKeyWindow())
    })
}

/// The key window's handle, for menu commands to remember their origin.
pub(crate) fn key_window_handle(mtm: MainThreadMarker) -> Option<AnyWindowHandle> {
    key_window(mtm).map(|window| window.handle)
}

/// One press of a key event: usage, and whether this event ends it.
fn presses(
    event: &objc2_ui_kit::UIPressesEvent,
    mtm: MainThreadMarker,
) -> Vec<(isize, objc2_ui_kit::UIPressPhase)> {
    event
        .allPresses()
        .iter()
        .filter_map(|press| Some((press.key(mtm)?.keyCode().0, press.phase())))
        .filter(|(usage, _)| !fence::is_modifier(*usage))
        .collect()
}

/// Before UIKit delivers a key event (`HerdrApplication.sendEvent:`): a key
/// that begins while the key window's gate is shut is fenced in that window
/// only. Returns whether the event carries a fenced key there, so a menu
/// command (or its repeat) it triggers does not reach GPUI either.
pub(crate) fn presses_will_deliver(
    event: &objc2_ui_kit::UIPressesEvent,
    mtm: MainThreadMarker,
) -> bool {
    use objc2_ui_kit::UIPressPhase;
    let Some(window) = key_window(mtm) else {
        return false;
    };
    let presses = presses(event, mtm);
    let mut state = window.state.borrow_mut();
    if state.modal_blocks_input() {
        for (usage, phase) in &presses {
            if *phase == UIPressPhase::Began {
                state.press_fence.hold(*usage);
            }
        }
    }
    presses.iter().any(|(usage, phase)| {
        matches!(
            *phase,
            UIPressPhase::Began | UIPressPhase::Changed | UIPressPhase::Stationary
        ) && state.press_fence.blocks(*usage)
    }) || state.modal_blocks_input()
}

/// After UIKit delivered a key event: a release ends its key's fence in
/// every window (wherever it went), and keys UIKit no longer reports held
/// lost their release.
pub(crate) fn presses_delivered(event: &objc2_ui_kit::UIPressesEvent, mtm: MainThreadMarker) {
    use objc2_ui_kit::UIPressPhase;
    let presses = presses(event, mtm);
    let held: Vec<isize> = presses
        .iter()
        .filter(|(_, phase)| !matches!(*phase, UIPressPhase::Ended | UIPressPhase::Cancelled))
        .map(|(usage, _)| *usage)
        .collect();
    for window in scene::live_windows(mtm) {
        let mut state = window.state.borrow_mut();
        for (usage, phase) in &presses {
            if matches!(*phase, UIPressPhase::Ended | UIPressPhase::Cancelled) {
                state.press_fence.released(*usage);
            }
        }
        state.press_fence.retain_held(&held);
    }
}

pub(crate) fn present(
    handle: AnyWindowHandle,
    request: NativeSearchRequest,
    queue: SharedQueue,
    _mtm: MainThreadMarker,
) -> Result<SessionId, NativeSearchError> {
    let last_query = request.query.clone();
    open_session(
        handle,
        move |id, window| {
            Session::Search(Search {
                id,
                queue,
                panel: None,
                last_query,
                revision: 0,
                window,
            })
        },
        move |w, id, this| w.present_panel(id, request, this),
    )
    .map_err(NativeSearchError::from)
}

/// Accepts a session of either kind: the gate shuts at once, `session` is
/// stored as the window's consumer side, and `deferred` presents it on the
/// next main-queue turn.
pub(super) fn open_session(
    handle: AnyWindowHandle,
    session: impl FnOnce(SessionId, std::rc::Weak<Window>) -> Session,
    deferred: impl FnOnce(&Window, SessionId, std::rc::Weak<Window>) + 'static,
) -> Result<SessionId, OpenError> {
    let window = window(handle).ok_or(OpenError::WindowUnavailable)?;
    {
        let state = window.state.borrow();
        if state.ui_window.is_none() || state.scene.is_none() {
            return Err(OpenError::WindowUnavailable);
        }
        if state.native_modal.blocks() {
            return Err(OpenError::AlreadyPresented);
        }
        if state.native_request_pending.is_some() || window.text_input.replacing.get() {
            return Err(OpenError::RetirementInProgress);
        }
    }
    // GPUI hears the release of every key it saw pressed while it can still
    // be told: once the gate shuts, nothing reaches it.
    let held = window.text_input.release_held_for_modal();
    let id = window
        .state
        .borrow_mut()
        .native_modal
        .request()
        .map_err(|_| OpenError::AlreadyPresented)?;
    *window.native_search.borrow_mut() = Some(session(id, Rc::downgrade(&window)));
    window.shut_gate(held);
    // UIKit answers a presentation and a responder change by querying text
    // clients synchronously; present outside GPUI's update.
    let this = Rc::downgrade(&window);
    window.defer(move |w| deferred(w, id, this));
    Ok(id)
}

pub(crate) fn update(
    handle: AnyWindowHandle,
    id: SessionId,
    results: NativeSearchResults,
) -> Result<u64, NativeSearchError> {
    let window = window(handle).ok_or(NativeSearchError::Ended)?;
    let mut slot = window.native_search.borrow_mut();
    let session = slot
        .as_mut()
        .and_then(Session::search_mut)
        .filter(|s| s.id == id)
        .ok_or(NativeSearchError::Ended)?;
    session.revision += 1;
    let revision = session.revision;
    if let Some(panel) = session.panel.clone() {
        drop(slot);
        panel.update(results, revision);
    }
    Ok(revision)
}

pub(crate) fn set_query(
    handle: AnyWindowHandle,
    id: SessionId,
    query: &str,
) -> Result<(), NativeSearchError> {
    let window = window(handle).ok_or(NativeSearchError::Ended)?;
    let mut slot = window.native_search.borrow_mut();
    let session = slot
        .as_mut()
        .and_then(Session::search_mut)
        .filter(|s| s.id == id)
        .ok_or(NativeSearchError::Ended)?;
    // The last query is what an edit or the dismissal compares against, so
    // the replaced text is never echoed back as `QueryChanged`.
    session.last_query = query.to_owned();
    if let Some(panel) = session.panel.clone() {
        drop(slot);
        panel.set_query(query);
    }
    Ok(())
}

pub(crate) fn dismiss(
    handle: AnyWindowHandle,
    id: SessionId,
    reason: DismissReason,
) -> Result<(), NativeSearchError> {
    let window = window(handle).ok_or(NativeSearchError::Ended)?;
    if window.native_modal_session() != Some(id) {
        return Err(NativeSearchError::Ended);
    }
    window.native_modal_dismiss(id, reason);
    Ok(())
}

pub(crate) fn release(handle: AnyWindowHandle, id: SessionId) {
    if let Some(window) = window(handle) {
        let effects = window.state.borrow_mut().native_modal.release(id);
        window.apply(id, effects);
    }
}

impl Window {
    fn weak(&self) -> std::rc::Weak<Window> {
        self.native_search
            .borrow()
            .as_ref()
            .map_or_else(std::rc::Weak::new, |s| s.window().clone())
    }

    fn native_modal_session(&self) -> Option<SessionId> {
        self.state.borrow().native_modal.session()
    }

    /// What happens the moment a presentation is accepted, inside GPUI's
    /// update: no Edit menu, no GPUI-held key or repeat, and the keyboard
    /// intent starts empty.
    fn shut_gate(&self, held: Vec<isize>) {
        self.edit_menu.revoke(crate::RevokeReason::Unavailable);
        {
            let mut state = self.state.borrow_mut();
            for usage in held {
                state.press_fence.hold(usage);
            }
            state.modal_intent = Intent::default();
            state.keyboard.expect_hide(ResignReason::NativeModal);
        }
        objc2_ui_kit::UIMenuSystem::mainSystem(self.mtm).setNeedsRevalidate();
    }

    /// The deferred present's checks, for either kind: whether session `id`
    /// still waits for UIKit and the window can present it. Otherwise the
    /// gate has been told (nothing presented, or a failure). On `true`,
    /// GPUI's views have given up first responder.
    pub(super) fn ready_to_present(&self, id: SessionId) -> bool {
        let (phase, ui_window) = {
            let state = self.state.borrow();
            (state.native_modal.phase(), state.ui_window.clone())
        };
        match phase {
            Some((current, Phase::Presenting { released: false })) if current == id => {}
            // Released or dismissed before UIKit was asked.
            Some((current, Phase::Presenting { released: true } | Phase::Dismissing { .. }))
                if current == id =>
            {
                let effects = self.state.borrow_mut().native_modal.not_presented(id);
                self.apply(id, effects);
                return false;
            }
            _ => return false,
        }
        let failure = if ui_window.is_none() {
            Some(PresentFailure::WindowDetached)
        } else if self.controller.presentedViewController().is_some() {
            Some(PresentFailure::PresenterBusy)
        } else {
            None
        };
        if let Some(failure) = failure {
            let effects = self.state.borrow_mut().native_modal.failed(id, failure);
            self.apply(id, effects);
            return false;
        }
        // GPUI's views give up first responder before the field takes it;
        // their resignation is expected, not a user's keyboard dismissal.
        {
            let switch = super::edit_menu::responder_switch::ResponderSwitch::new(self);
            self.text_input.resign(ResignReason::NativeModal, &switch);
            if self.view.isFirstResponder() {
                switch.resign(&**self.view);
            }
        }
        true
    }

    fn present_panel(
        &self,
        id: SessionId,
        mut request: NativeSearchRequest,
        weak: std::rc::Weak<Window>,
    ) {
        if !self.ready_to_present(id) {
            return;
        }
        let binding = || Binding {
            window: weak.clone(),
            session: id,
        };
        // A `set_query` between `present` and now replaced the last query.
        if let Some(session) = self
            .native_search
            .borrow()
            .as_ref()
            .and_then(Session::search)
            .filter(|s| s.id == id)
        {
            request.query.clone_from(&session.last_query);
        }
        let panel = Rc::new(Panel::new(binding, &request, self.mtm));
        panel.configure(&request, &self.view);
        if let Some(session) = self
            .native_search
            .borrow_mut()
            .as_mut()
            .and_then(Session::search_mut)
        {
            session.panel = Some(panel.clone());
        }
        let done = weak.clone();
        let field = panel.field.clone();
        let completion = block2::RcBlock::new(move || {
            let Some(window) = done.upgrade() else {
                return;
            };
            let focused = window.acquire_field(&field);
            let effects = window
                .state
                .borrow_mut()
                .native_modal
                .presented(id, focused);
            window.apply(id, effects);
            if !focused && window.state.borrow().is_active {
                // An active scene's field may take first responder a turn
                // later; if it still refuses, the panel is unusable.
                let field = field.clone();
                window.defer(move |w| {
                    w.retry_field_focus(id, &field);
                });
            }
        });
        self.controller.presentViewController_animated_completion(
            &panel.controller,
            true,
            Some(&completion),
        );
    }

    /// A later attempt for a field that refused first responder; returns
    /// whether it took it (otherwise the session failed).
    pub(super) fn retry_field_focus(&self, id: SessionId, field: &UIView) -> bool {
        let unfocused = matches!(
            self.state.borrow().native_modal.phase(),
            Some((current, Phase::Presented { focused: false })) if current == id
        );
        if !unfocused {
            return false;
        }
        let focused = self.acquire_field(field);
        let effects = if focused {
            self.state.borrow_mut().native_modal.focused(id)
        } else {
            self.state
                .borrow_mut()
                .native_modal
                .failed(id, PresentFailure::FieldRefused)
        };
        self.apply(id, effects);
        focused
    }

    /// The field takes first responder through the window's responder guard.
    pub(super) fn acquire_field(&self, field: &UIView) -> bool {
        let switch = super::edit_menu::responder_switch::ResponderSwitch::new(self);
        switch.acquire(field)
    }

    pub(super) fn native_modal_dismiss(&self, id: SessionId, reason: DismissReason) {
        self.flush_query(id);
        let effects = self.state.borrow_mut().native_modal.dismiss(id, reason);
        self.apply(id, effects);
    }

    pub(super) fn native_modal_dismissed(&self, id: SessionId, reason: DismissReason) {
        self.flush_query(id);
        let effects = self.state.borrow_mut().native_modal.dismissed(id, reason);
        self.apply(id, effects);
    }

    pub(super) fn native_modal_query(&self, id: SessionId, text: String) {
        let mut slot = self.native_search.borrow_mut();
        let Some(session) = slot
            .as_mut()
            .and_then(Session::search_mut)
            .filter(|s| s.id == id)
        else {
            return;
        };
        if session.last_query != text {
            session.last_query.clone_from(&text);
            session
                .queue
                .borrow_mut()
                .push(NativeSearchEvent::QueryChanged(text));
        }
    }

    /// The final committed text goes out before the dismissal does.
    fn flush_query(&self, id: SessionId) {
        let panel = self
            .native_search
            .borrow()
            .as_ref()
            .and_then(Session::search)
            .filter(|s| s.id == id)
            .and_then(|s| s.panel.clone());
        if let Some(panel) = panel {
            self.native_modal_query(id, panel.committed_text());
        }
    }

    pub(super) fn native_modal_clear_requested(&self, id: SessionId) {
        self.push(id, NativeSearchEvent::ClearRequested);
    }

    pub(super) fn native_modal_selected(&self, id: SessionId, item: u64, revision: u64) {
        let up = matches!(
            self.state.borrow().native_modal.phase(),
            Some((current, Phase::Presented { focused: true })) if current == id
        );
        if up {
            self.push(id, NativeSearchEvent::Selected { id: item, revision });
        }
    }

    fn push(&self, id: SessionId, event: NativeSearchEvent) {
        let queue = self
            .native_search
            .borrow()
            .as_ref()
            .and_then(Session::search)
            .filter(|s| s.id == id)
            .map(|s| s.queue.clone());
        if let Some(queue) = queue {
            queue.borrow_mut().push(event);
        }
    }

    /// Carries out a transition's effects, in order.
    pub(super) fn apply(&self, id: SessionId, effects: Vec<Effect>) {
        for effect in effects {
            match effect {
                Effect::Emit(note) => self.emit(id, note),
                Effect::Dismiss { animated } => self.dismiss_panel(id, animated),
                Effect::Restore => {
                    self.restore_after_modal();
                    self.check_restore(id, 0);
                }
                Effect::RetryRestore(attempt) => {
                    self.restore_after_modal();
                    self.check_restore(id, attempt);
                }
            }
        }
    }

    fn emit(&self, id: SessionId, note: Note) {
        let restored = matches!(note, Note::Restored(_));
        let sink = {
            let mut slot = self.native_search.borrow_mut();
            let sink = slot.as_ref().filter(|s| s.id() == id).map(|s| match s {
                Session::Search(s) => Sink::Search(s.queue.clone()),
                Session::Inline(s) => Sink::Inline(s.queue.clone(), s.decision.clone()),
            });
            if restored {
                *slot = None;
            }
            sink
        };
        match sink {
            Some(Sink::Search(queue)) => {
                let event = match note {
                    Note::Opened => NativeSearchEvent::Opened,
                    Note::Dismissed(reason) => NativeSearchEvent::Dismissed(reason),
                    Note::Failed(failure) => NativeSearchEvent::Failed(failure),
                    Note::Restored(responder) => NativeSearchEvent::InputRestored(responder),
                };
                let mut queue = queue.borrow_mut();
                queue.push(event);
                if restored {
                    queue.close();
                }
            }
            Some(Sink::Inline(queue, decision)) => {
                super::native_inline_field::emit(&queue, note, decision.as_ref());
            }
            None => {}
        }
        if restored {
            objc2_ui_kit::UIMenuSystem::mainSystem(self.mtm).setNeedsRevalidate();
        }
    }

    fn dismiss_panel(&self, id: SessionId, animated: bool) {
        let presented = self
            .native_search
            .borrow()
            .as_ref()
            .filter(|s| s.id() == id)
            // The inline field comes and goes without animation.
            .and_then(|s| Some((s.presented()?.0, animated && s.search().is_some())));
        let Some((controller, animated)) = presented else {
            return;
        };
        let weak = self.weak();
        let completion = block2::RcBlock::new(move || {
            if let Some(window) = weak.upgrade() {
                let effects = window
                    .state
                    .borrow_mut()
                    .native_modal
                    .dismissed(id, DismissReason::Requested);
                window.apply(id, effects);
            }
        });
        controller.dismissViewControllerAnimated_completion(animated, Some(&completion));
    }

    /// GPUI's keyboard intent is replayed and its responder taken again; the
    /// gate opens once one of GPUI's views is actually first responder.
    fn restore_after_modal(&self) {
        let request = {
            let mut state = self.state.borrow_mut();
            // The field's keyboard may still post WillHide after this.
            state.keyboard.expect_hide(ResignReason::NativeModal);
            state.keyboard.minimized = false;
            state.modal_intent.take()
        };
        match request {
            IntentRequest::Show => self.state.borrow_mut().keyboard.show(),
            IntentRequest::Hide => self.state.borrow_mut().keyboard.hide(),
            IntentRequest::None => {}
        }
        if let Some(window) = self.weak().upgrade() {
            window.defer(Window::update_first_responder);
        }
    }

    /// `RESTORE_CHECK` after restore attempt `attempt`, a session still
    /// restoring is asked again or, after the last attempt, ends with
    /// `InputRestored(None)` (`Gate::restore_timed_out`).
    fn check_restore(&self, id: SessionId, attempt: u32) {
        let handle = self.handle;
        let delay = i64::try_from(RESTORE_CHECK.as_nanos()).unwrap_or(i64::MAX);
        let when = dispatch2::DispatchTime::NOW.time(delay);
        let scheduled = dispatch2::DispatchQueue::main().after(when, move || {
            let Some(window) = window(handle) else {
                return;
            };
            // A view may hold first responder without a callback saying so.
            window.native_modal_responder_settled();
            let effects = window
                .state
                .borrow_mut()
                .native_modal
                .restore_timed_out(id, attempt);
            window.apply(id, effects);
        });
        if let Err(error) = scheduled {
            tracing::error!(?error, "GCD refused the restore check");
        }
    }

    /// Called after every responder change GPUI's views make: in the
    /// restoring phase, the first view that holds first responder opens the
    /// gate.
    pub(super) fn native_modal_responder_settled(&self) {
        let phase = self.state.borrow().native_modal.phase();
        let Some((id, Phase::Restoring)) = phase else {
            return;
        };
        let responder = if self.text_input.view().isFirstResponder() {
            Responder::Text
        } else if self.view.isFirstResponder() {
            Responder::Drawing
        } else {
            return;
        };
        let effects = self.state.borrow_mut().native_modal.restored(id, responder);
        self.apply(id, effects);
    }

    /// The scene became active again: the panel may have lost its field's
    /// first responder (or been removed by UIKit) while it was inactive.
    pub(super) fn native_modal_scene_active(&self) {
        let phase = self.state.borrow().native_modal.phase();
        let Some((id, Phase::Presented { focused })) = phase else {
            return;
        };
        let presented = self
            .native_search
            .borrow()
            .as_ref()
            .and_then(Session::presented);
        let Some((controller, field)) = presented else {
            return;
        };
        if controller.presentingViewController().is_none() {
            self.native_modal_dismissed(id, DismissReason::System);
            return;
        }
        if !field.isFirstResponder() && self.acquire_field(&field) && !focused {
            let effects = self.state.borrow_mut().native_modal.focused(id);
            self.apply(id, effects);
        }
    }

    /// UIKit cancelled a user dismissal (an abandoned sheet swipe) after
    /// resigning the field when it began: the field takes first responder
    /// back, as on scene reactivation.
    pub(super) fn native_modal_dismissal_cancelled(&self, id: SessionId) {
        if self.state.borrow().native_modal.session() == Some(id) {
            self.native_modal_scene_active();
        }
    }

    /// The window is closing or its scene disconnected: the session ends
    /// once, before UIKit's window is torn down.
    pub(super) fn native_modal_window_closed(&self) {
        let (id, effects) = {
            let mut state = self.state.borrow_mut();
            let id = state.native_modal.session();
            (id, state.native_modal.window_closed())
        };
        let Some(id) = id else {
            return;
        };
        for effect in effects {
            match effect {
                Effect::Emit(note) => self.emit(id, note),
                Effect::Dismiss { .. } => {
                    let presented = self
                        .native_search
                        .borrow()
                        .as_ref()
                        .and_then(Session::presented);
                    if let Some((controller, _)) = presented {
                        controller.dismissViewControllerAnimated_completion(false, None);
                    }
                }
                Effect::Restore | Effect::RetryRestore(_) => {}
            }
        }
        *self.native_search.borrow_mut() = None;
    }

    /// Input that reached GPUI's side while the gate was shut: dropped, and
    /// counted for QA (`native_search_state`).
    pub(super) fn native_modal_drops_input(&self) -> bool {
        self.state.borrow_mut().native_modal.drops_late_input()
    }
}

/// Where a session's events go.
enum Sink {
    Search(SharedQueue),
    Inline(
        crate::native_inline_field::InlineQueue,
        Option<super::native_inline_field::Decision>,
    ),
}

#[cfg(all(target_abi = "sim", feature = "test-support"))]
mod qa;
#[cfg(all(target_abi = "sim", feature = "test-support"))]
pub use qa::{NativeSearchState, native_search_state};
