//! `GPUITextInputView`, the invisible `UIView` subclass that is UIKit's text
//! input client for one window. The Objective-C methods here only convert
//! their arguments; the behavior lives in the sibling modules: `editing` for
//! text changes, `query` for what UIKit reads, `keys` for hardware presses,
//! and `sync` for telling UIKit about GPUI's own changes.

#![allow(
    unsafe_code,
    reason = "an Objective-C UIView subclass implementing UIKit's text input protocols"
)]

use objc2::rc::{Retained, Weak};
use objc2::runtime::{AnyObject, NSObject, NSObjectProtocol, ProtocolObject, Sel};
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, Message, define_class, msg_send};
use objc2_core_foundation::{CGPoint, CGRect};
use objc2_foundation::{NSArray, NSComparisonResult, NSInteger, NSRange, NSSet, NSString};
use objc2_ui_kit::{
    UIDictationPhrase, UIEvent, UIKeyInput, UIPress, UIPressesEvent, UIResponder, UIReturnKeyType,
    UITextAutocapitalizationType, UITextAutocorrectionType, UITextInlinePredictionType,
    UITextInput, UITextInputDelegate, UITextInputStringTokenizer, UITextInputTokenizer,
    UITextInputTraits, UITextLayoutDirection, UITextPosition, UITextRange, UITextSelectionRect,
    UITextSmartDashesType, UITextSmartInsertDeleteType, UITextSmartQuotesType,
    UITextSpellCheckingType, UITextStorageDirection, UIView, UIWindow,
};

use super::super::{input_target::InputTarget, keyboard::ResignReason};
use super::actions::EditAction;
use super::positions::{self, TextPosition, TextRange};
use super::state::{Ivars, Mode};
use super::utf16;

/// `NSWritingDirectionLeftToRight`. GPUI lays out left to right and reports
/// no direction, and the `NSText` binding is not compiled in.
const WRITING_DIRECTION_LEFT_TO_RIGHT: NSInteger = 0;

define_class!(
    // SAFETY: UIView's subclassing rules are kept: instances are created
    // through `initWithFrame:` (`GPUITextInputView::new`), the class does not
    // implement `Drop`, and every override keeps the superclass signature.
    #[unsafe(super(UIView, UIResponder, NSObject))]
    #[thread_kind = MainThreadOnly]
    #[ivars = Ivars]
    pub(super) struct GPUITextInputView;

    impl GPUITextInputView {
        // The view has real layout bounds so accessibility can discover the
        // first-responder text client. GPUI owns its touch hit testing.
        #[unsafe(method(pointInside:withEvent:))]
        fn __point_inside(&self, _: CGPoint, _: Option<&UIEvent>) -> bool {
            false
        }

        #[unsafe(method(isAccessibilityElement))]
        fn __is_accessibility_element(&self) -> bool {
            self.accessibility_text_entry()
        }

        #[unsafe(method_id(accessibilityLabel))]
        fn __accessibility_label(&self) -> Option<Retained<NSString>> {
            self.accessibility_text_entry().then(|| self.accessibility_label())
        }

        #[unsafe(method_id(accessibilityValue))]
        fn __accessibility_value(&self) -> Option<Retained<NSString>> {
            // The input handler does not expose whether a field is secure.
            // Text entry works through UITextInput without publishing its text.
            None
        }

        #[unsafe(method(accessibilityFrame))]
        fn __accessibility_frame(&self) -> CGRect {
            self.accessibility_frame()
        }

        // Inherit UIKit's accessibilityTraits so its live UITextInput
        // responder contributes editing state; cached traits from another
        // text field would hide this responder's current keyboard focus.

        // Since iOS 18.1, accessibility asks custom text elements for their
        // backing UITextInput responder. This view is both. Implementing the
        // public getter is harmless on iOS 17, which does not query it, and
        // avoids calling an unavailable framework selector there.
        #[unsafe(method_id(accessibilityTextInputResponder))]
        fn __accessibility_text_input_responder(
            &self,
        ) -> Option<Retained<ProtocolObject<dyn UITextInput>>> {
            self.accessibility_text_entry()
                .then(|| ProtocolObject::from_retained(self.retain()))
        }

        #[unsafe(method(canPerformAction:withSender:))]
        fn __can_perform_action(&self, action: Sel, _: Option<&AnyObject>) -> bool {
            self.editing_enabled() && EditAction::from_selector(action).is_some()
        }

        #[unsafe(method(paste:))]
        fn __paste(&self, _: Option<&AnyObject>) {
            self.perform_edit_action(EditAction::Paste);
        }

        #[unsafe(method(copy:))]
        fn __copy(&self, _: Option<&AnyObject>) {
            self.perform_edit_action(EditAction::Copy);
        }

        #[unsafe(method(cut:))]
        fn __cut(&self, _: Option<&AnyObject>) {
            self.perform_edit_action(EditAction::Cut);
        }

        #[unsafe(method(selectAll:))]
        fn __select_all(&self, _: Option<&AnyObject>) {
            self.perform_edit_action(EditAction::SelectAll);
        }

        #[unsafe(method(canBecomeFirstResponder))]
        fn __can_become_first_responder(&self) -> bool {
            true
        }

        #[unsafe(method_id(inputView))]
        fn __input_view(&self) -> Option<Retained<UIView>> {
            (self.ivars().mode.get() == Some(Mode::KeyOnly))
                .then(|| self.ivars().key_only_input_view.clone())
        }

        // UIKit takes first responder away for the keyboard's dismiss key,
        // for another responder, and when the view leaves its window; the
        // flushes must not take it straight back.
        #[unsafe(method(resignFirstResponder))]
        fn __resign_first_responder(&self) -> bool {
            // UIKit may post WillHide only after it has already transferred
            // first responder, so capture ownership before the native call.
            let context = self.ivars().target.text_input_resign_context(self.isFirstResponder());
            // SAFETY: `resignFirstResponder` takes nothing and returns a BOOL.
            let resigned: bool = unsafe { msg_send![super(self), resignFirstResponder] };
            if resigned {
                self.did_resign_first_responder(context);
            }
            resigned
        }

        #[unsafe(method(willMoveToWindow:))]
        fn __will_move_to_window(&self, window: Option<&UIWindow>) {
            if window.is_none() {
                self.ivars().target.expect_keyboard_hide(ResignReason::LeavingWindow);
            }
            // SAFETY: the argument is UIKit's nullable UIWindow for this selector;
            // UIView's implementation returns nothing.
            let () = unsafe { msg_send![super(self), willMoveToWindow: window] };
        }

        // Becoming first responder needs a window, so a mode asked for before
        // the view was in one, or before it left one, is applied now.
        #[unsafe(method(didMoveToWindow))]
        fn __did_move_to_window(&self) {
            // SAFETY: `didMoveToWindow` takes and returns nothing.
            let () = unsafe { msg_send![super(self), didMoveToWindow] };
            if self.window().is_some() && self.ivars().target.text_input_can_acquire() {
                self.request_first_responder();
            }
        }

        #[unsafe(method(pressesBegan:withEvent:))]
        fn __presses_began(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            self.presses_began(presses, event);
        }

        #[unsafe(method(pressesChanged:withEvent:))]
        fn __presses_changed(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            // SAFETY: the arguments are the ones UIKit passed for this selector.
            let () = unsafe { msg_send![super(self), pressesChanged: presses, withEvent: event] };
        }

        #[unsafe(method(pressesEnded:withEvent:))]
        fn __presses_ended(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            self.presses_ended(presses, event, false);
        }

        #[unsafe(method(pressesCancelled:withEvent:))]
        fn __presses_cancelled(&self, presses: &NSSet<UIPress>, event: Option<&UIPressesEvent>) {
            self.presses_ended(presses, event, true);
        }

        // Targets of `performSelector:withObject:afterDelay:`, which runs them
        // from the run loop, outside GPUI's updates and the window's state.
        #[unsafe(method(herdrFlushInputState:))]
        fn __flush_input_state(&self, _: Option<&AnyObject>) {
            self.flush();
        }

        #[unsafe(method(herdrRepeatKey:))]
        fn __repeat_key(&self, _: Option<&AnyObject>) {
            self.repeat_key();
        }
    }

    unsafe impl NSObjectProtocol for GPUITextInputView {}

    unsafe impl UITextInputTraits for GPUITextInputView {
        #[unsafe(method(autocapitalizationType))]
        fn __autocapitalization_type(&self) -> UITextAutocapitalizationType {
            self.ivars().traits.get().autocapitalization
        }

        #[unsafe(method(autocorrectionType))]
        fn __autocorrection_type(&self) -> UITextAutocorrectionType {
            self.ivars().traits.get().autocorrection
        }

        #[unsafe(method(spellCheckingType))]
        fn __spell_checking_type(&self) -> UITextSpellCheckingType {
            self.ivars().traits.get().spell_checking
        }

        #[unsafe(method(inlinePredictionType))]
        fn __inline_prediction_type(&self) -> UITextInlinePredictionType {
            self.ivars().traits.get().inline_prediction
        }

        #[unsafe(method(smartQuotesType))]
        fn __smart_quotes_type(&self) -> UITextSmartQuotesType {
            UITextSmartQuotesType::No
        }

        #[unsafe(method(smartDashesType))]
        fn __smart_dashes_type(&self) -> UITextSmartDashesType {
            UITextSmartDashesType::No
        }

        #[unsafe(method(smartInsertDeleteType))]
        fn __smart_insert_delete_type(&self) -> UITextSmartInsertDeleteType {
            UITextSmartInsertDeleteType::No
        }

        #[unsafe(method(returnKeyType))]
        fn __return_key_type(&self) -> UIReturnKeyType {
            self.ivars().traits.get().return_key
        }
    }

    unsafe impl UIKeyInput for GPUITextInputView {
        // A terminal's document is empty between compositions, yet the delete
        // key must keep sending backspace, so any focused text element counts.
        #[unsafe(method(hasText))]
        fn __has_text(&self) -> bool {
            self.with_handler(|_| ()).is_some()
        }

        #[unsafe(method(insertText:))]
        fn __insert_text(&self, text: &NSString) {
            self.insert_text(&text.to_string());
        }

        #[unsafe(method(deleteBackward))]
        fn __delete_backward(&self) {
            self.delete_backward();
        }
    }

    unsafe impl UITextInput for GPUITextInputView {
        #[unsafe(method_id(textInRange:))]
        fn __text_in_range(&self, range: &UITextRange) -> Option<Retained<NSString>> {
            positions::range(range)
                .and_then(|range| self.text_in_range(range))
                .map(|text| NSString::from_str(&text))
        }

        #[unsafe(method(replaceRange:withText:))]
        fn __replace_range(&self, range: &UITextRange, text: &NSString) {
            if let Some(range) = positions::range(range) {
                self.replace_range(range, &text.to_string());
            }
        }

        #[unsafe(method_id(selectedTextRange))]
        fn __selected_text_range(&self) -> Option<Retained<UITextRange>> {
            let selection = self.read_snapshot().selection.map(|(range, _)| range);
            selection.map(|range| TextRange::text_range(self.mtm(), range))
        }

        #[unsafe(method(setSelectedTextRange:))]
        fn __set_selected_text_range(&self, range: Option<&UITextRange>) {
            if let Some(range) = range.and_then(positions::range) {
                self.select_range(range);
            }
        }

        #[unsafe(method_id(markedTextRange))]
        fn __marked_text_range(&self) -> Option<Retained<UITextRange>> {
            let marked = self.read_snapshot().marked;
            marked.map(|range| TextRange::text_range(self.mtm(), range))
        }

        // GPUI draws marked text itself; the style is kept only so UIKit reads
        // back what it set.
        #[unsafe(method_id(markedTextStyle))]
        fn __marked_text_style(&self) -> Option<Retained<AnyObject>> {
            self.ivars().marked_text_style.borrow().clone()
        }

        #[unsafe(method(setMarkedTextStyle:))]
        fn __set_marked_text_style(&self, style: Option<&AnyObject>) {
            *self.ivars().marked_text_style.borrow_mut() = style.map(AnyObject::retain);
        }

        #[unsafe(method(setMarkedText:selectedRange:))]
        fn __set_marked_text(&self, text: Option<&NSString>, selected: NSRange) {
            let text = text.map(NSString::to_string).unwrap_or_default();
            self.set_marked_text(&text, utf16::range_from_ns(selected.location, selected.length));
        }

        #[unsafe(method(unmarkText))]
        fn __unmark_text(&self) {
            self.unmark_text();
        }

        #[unsafe(method_id(beginningOfDocument))]
        fn __beginning_of_document(&self) -> Retained<UITextPosition> {
            TextPosition::position(self.mtm(), 0)
        }

        #[unsafe(method_id(endOfDocument))]
        fn __end_of_document(&self) -> Retained<UITextPosition> {
            TextPosition::position(self.mtm(), self.read_snapshot().document_len())
        }

        #[unsafe(method_id(textRangeFromPosition:toPosition:))]
        fn __text_range(
            &self,
            from: &UITextPosition,
            to: &UITextPosition,
        ) -> Option<Retained<UITextRange>> {
            positions::offset(from)
                .zip(positions::offset(to))
                .map(|(from, to)| TextRange::text_range(self.mtm(), utf16::between(from, to)))
        }

        #[unsafe(method_id(positionFromPosition:offset:))]
        fn __position_from_position(
            &self,
            position: &UITextPosition,
            offset: NSInteger,
        ) -> Option<Retained<UITextPosition>> {
            positions::offset(position)
                .and_then(|from| self.offset_from(from, offset))
                .map(|offset| TextPosition::position(self.mtm(), offset))
        }

        #[unsafe(method_id(positionFromPosition:inDirection:offset:))]
        fn __position_in_direction(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
            offset: NSInteger,
        ) -> Option<Retained<UITextPosition>> {
            positions::offset(position)
                .and_then(|from| self.position_in_direction(from, direction, offset))
                .map(|offset| TextPosition::position(self.mtm(), offset))
        }

        #[unsafe(method(comparePosition:toPosition:))]
        fn __compare_position(
            &self,
            position: &UITextPosition,
            other: &UITextPosition,
        ) -> NSComparisonResult {
            let (position, other) = (positions::offset(position), positions::offset(other));
            NSComparisonResult::from(position.cmp(&other))
        }

        #[unsafe(method(offsetFromPosition:toPosition:))]
        fn __offset_between(&self, from: &UITextPosition, to: &UITextPosition) -> NSInteger {
            positions::offset(from)
                .zip(positions::offset(to))
                .map_or(0, |(from, to)| utf16::distance(from, to))
        }

        #[unsafe(method_id(inputDelegate))]
        fn __input_delegate(&self) -> Option<Retained<ProtocolObject<dyn UITextInputDelegate>>> {
            self.ivars().input_delegate.borrow().load()
        }

        // A weak property: UIKit owns its delegate.
        #[unsafe(method(setInputDelegate:))]
        fn __set_input_delegate(
            &self,
            delegate: Option<&ProtocolObject<dyn UITextInputDelegate>>,
        ) {
            *self.ivars().input_delegate.borrow_mut() = delegate.map(Weak::new).unwrap_or_default();
        }

        // Made per call rather than kept: the string tokenizer refers back to
        // its text input, and keeping it would risk a retain cycle.
        #[unsafe(method_id(tokenizer))]
        fn __tokenizer(&self) -> Retained<ProtocolObject<dyn UITextInputTokenizer>> {
            let responder: &UIResponder = self;
            // SAFETY: `responder` is this view, which implements UITextInput.
            let tokenizer = unsafe {
                UITextInputStringTokenizer::initWithTextInput(
                    UITextInputStringTokenizer::alloc(self.mtm()),
                    responder,
                )
            };
            ProtocolObject::from_retained(tokenizer)
        }

        #[unsafe(method_id(positionWithinRange:farthestInDirection:))]
        fn __farthest_position(
            &self,
            range: &UITextRange,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextPosition>> {
            positions::range(range)
                .map(|range| positions::farthest(range, direction))
                .map(|offset| TextPosition::position(self.mtm(), offset))
        }

        #[unsafe(method_id(characterRangeByExtendingPosition:inDirection:))]
        fn __extend_position(
            &self,
            position: &UITextPosition,
            direction: UITextLayoutDirection,
        ) -> Option<Retained<UITextRange>> {
            positions::offset(position)
                .map(|offset| self.extended_range(offset, direction))
                .map(|range| TextRange::text_range(self.mtm(), range))
        }

        #[unsafe(method(baseWritingDirectionForPosition:inDirection:))]
        fn __base_writing_direction(
            &self,
            _position: &UITextPosition,
            _direction: UITextStorageDirection,
        ) -> NSInteger {
            WRITING_DIRECTION_LEFT_TO_RIGHT
        }

        // GPUI's input handler cannot change a paragraph's direction.
        #[unsafe(method(setBaseWritingDirection:forRange:))]
        fn __set_base_writing_direction(&self, _direction: NSInteger, _range: &UITextRange) {}

        #[unsafe(method(firstRectForRange:))]
        fn __first_rect(&self, range: &UITextRange) -> CGRect {
            positions::range(range).map_or(CGRect::ZERO, |range| self.first_rect(range))
        }

        #[unsafe(method(caretRectForPosition:))]
        fn __caret_rect(&self, position: &UITextPosition) -> CGRect {
            positions::offset(position).map_or(CGRect::ZERO, |offset| self.caret_rect(offset))
        }

        // GPUI draws its own selection, so UIKit is given no rectangles to
        // highlight.
        #[unsafe(method_id(selectionRectsForRange:))]
        fn __selection_rects(&self, _range: &UITextRange) -> Retained<NSArray<UITextSelectionRect>> {
            NSArray::from_retained_slice(&[])
        }

        #[unsafe(method_id(closestPositionToPoint:))]
        fn __closest_position(&self, point: CGPoint) -> Option<Retained<UITextPosition>> {
            self.closest_offset(point)
                .map(|offset| TextPosition::position(self.mtm(), offset))
        }

        #[unsafe(method_id(closestPositionToPoint:withinRange:))]
        fn __closest_position_within(
            &self,
            point: CGPoint,
            range: &UITextRange,
        ) -> Option<Retained<UITextPosition>> {
            positions::range(range)
                .zip(self.closest_offset(point))
                .map(|(range, offset)| offset.clamp(range.start, range.end))
                .map(|offset| TextPosition::position(self.mtm(), offset))
        }

        #[unsafe(method_id(characterRangeAtPoint:))]
        fn __character_range_at_point(&self, point: CGPoint) -> Option<Retained<UITextRange>> {
            self.closest_offset(point)
                .map(|offset| self.extended_range(offset, UITextLayoutDirection::Right))
                .map(|range| TextRange::text_range(self.mtm(), range))
        }

        // Geometry is in GPUI window coordinates, which are the GPUIView's.
        #[unsafe(method_id(textInputView))]
        fn __text_input_view(&self) -> Retained<UIView> {
            self.superview()
                .unwrap_or_else(|| Retained::into_super(self.retain()))
        }

        #[unsafe(method(insertDictationResult:))]
        fn __insert_dictation_result(&self, phrases: &NSArray<UIDictationPhrase>) {
            let text: String = phrases.iter().map(|phrase| phrase.text().to_string()).collect();
            self.insert_dictation(&text);
        }
    }
);

impl GPUITextInputView {
    pub(super) fn new(mtm: MainThreadMarker, target: InputTarget) -> Retained<Self> {
        let key_only_input_view = UIView::initWithFrame(UIView::alloc(mtm), CGRect::ZERO);
        let this = Self::alloc(mtm).set_ivars(Ivars::new(target, key_only_input_view));
        // The window sizes this to its drawing view before attachment. The
        // origins agree and pointInside:withEvent: leaves touches to GPUI.
        // User interaction stays enabled: UIKit drops presses without it.
        // SAFETY: `initWithFrame:` is UIView's designated initializer, and the
        // ivars were set before it runs.
        unsafe { msg_send![super(this), initWithFrame: CGRect::ZERO] }
    }

    /// Runs `f` with the focused element's input handler.
    pub(super) fn with_handler<R>(
        &self,
        f: impl FnOnce(&mut gpui::PlatformInputHandler) -> R,
    ) -> Option<R> {
        self.ivars().target.with_input_handler(f)
    }

    /// This view as UIKit's text input protocol object, for the delegate.
    pub(super) fn as_text_input(&self) -> &ProtocolObject<dyn UITextInput> {
        ProtocolObject::from_ref(self)
    }
}
