//! The `UITextInputTraits` the software keyboard reads, from GPUI's
//! `TextInputConfiguration` for the focused element.
//!
//! Smart quotes, smart dashes, and smart insert/delete are always off: a
//! terminal must receive exactly the characters typed, and GPUI's
//! configuration has no field for them.

use gpui::{Autocapitalize, TextInputAction, TextInputConfiguration};
use objc2_ui_kit::{
    UIReturnKeyType, UITextAutocapitalizationType, UITextAutocorrectionType,
    UITextInlinePredictionType, UITextSpellCheckingType,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct Traits {
    pub(super) autocapitalization: UITextAutocapitalizationType,
    pub(super) autocorrection: UITextAutocorrectionType,
    pub(super) spell_checking: UITextSpellCheckingType,
    pub(super) inline_prediction: UITextInlinePredictionType,
    pub(super) return_key: UIReturnKeyType,
}

impl Default for Traits {
    /// GPUI's default configuration: no text assistance and the system's
    /// return key.
    fn default() -> Self {
        Self::from(&TextInputConfiguration::default())
    }
}

impl From<&TextInputConfiguration> for Traits {
    fn from(configuration: &TextInputConfiguration) -> Self {
        let autocapitalization = match configuration.autocapitalize {
            Autocapitalize::None => UITextAutocapitalizationType::None,
            Autocapitalize::Words => UITextAutocapitalizationType::Words,
            Autocapitalize::Sentences => UITextAutocapitalizationType::Sentences,
            Autocapitalize::Characters => UITextAutocapitalizationType::AllCharacters,
        };
        let (autocorrection, spell_checking, inline_prediction) = if configuration.autocorrect {
            (
                UITextAutocorrectionType::Yes,
                UITextSpellCheckingType::Yes,
                UITextInlinePredictionType::Yes,
            )
        } else {
            (
                UITextAutocorrectionType::No,
                UITextSpellCheckingType::No,
                UITextInlinePredictionType::No,
            )
        };
        // Suggestions without autocorrection still mean the word list and
        // spellcheck underlines, but never silent replacement.
        let spell_checking = if configuration.suggestions {
            UITextSpellCheckingType::Yes
        } else {
            spell_checking
        };
        Self {
            autocapitalization,
            autocorrection,
            spell_checking,
            inline_prediction,
            return_key: return_key(configuration.input_action),
        }
    }
}

/// UIKit has no "previous" or "enter" return key; both keep the system key.
fn return_key(action: TextInputAction) -> UIReturnKeyType {
    match action {
        TextInputAction::Unspecified | TextInputAction::Enter | TextInputAction::Previous => {
            UIReturnKeyType::Default
        }
        TextInputAction::Done => UIReturnKeyType::Done,
        TextInputAction::Go => UIReturnKeyType::Go,
        TextInputAction::Next => UIReturnKeyType::Next,
        TextInputAction::Search => UIReturnKeyType::Search,
        TextInputAction::Send => UIReturnKeyType::Send,
    }
}
