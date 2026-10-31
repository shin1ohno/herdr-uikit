use super::*;

use gpui::{Autocapitalize, TextInputAction};
use objc2_ui_kit::{
    UIReturnKeyType, UITextAutocapitalizationType, UITextAutocorrectionType,
    UITextInlinePredictionType, UITextSpellCheckingType,
};

#[test]
fn default_configuration_turns_assistance_off() {
    let traits = Traits::default();
    assert_eq!(
        traits.autocapitalization,
        UITextAutocapitalizationType::None
    );
    assert_eq!(traits.autocorrection, UITextAutocorrectionType::No);
    assert_eq!(traits.spell_checking, UITextSpellCheckingType::No);
    assert_eq!(traits.inline_prediction, UITextInlinePredictionType::No);
    assert_eq!(traits.return_key, UIReturnKeyType::Default);
}

#[test]
fn autocorrect_turns_on_corrections_and_predictions() {
    let traits = Traits::from(&TextInputConfiguration {
        autocorrect: true,
        autocapitalize: Autocapitalize::Sentences,
        suggestions: false,
        input_action: TextInputAction::Send,
    });
    assert_eq!(
        traits.autocapitalization,
        UITextAutocapitalizationType::Sentences
    );
    assert_eq!(traits.autocorrection, UITextAutocorrectionType::Yes);
    assert_eq!(traits.spell_checking, UITextSpellCheckingType::Yes);
    assert_eq!(traits.inline_prediction, UITextInlinePredictionType::Yes);
    assert_eq!(traits.return_key, UIReturnKeyType::Send);
}

#[test]
fn suggestions_alone_spellcheck_without_correcting() {
    let traits = Traits::from(&TextInputConfiguration {
        suggestions: true,
        ..TextInputConfiguration::default()
    });
    assert_eq!(traits.autocorrection, UITextAutocorrectionType::No);
    assert_eq!(traits.spell_checking, UITextSpellCheckingType::Yes);
    assert_eq!(traits.inline_prediction, UITextInlinePredictionType::No);
}

#[test]
fn capitalization_and_return_keys_map_one_to_one() {
    let capitalization = [
        (Autocapitalize::None, UITextAutocapitalizationType::None),
        (Autocapitalize::Words, UITextAutocapitalizationType::Words),
        (
            Autocapitalize::Sentences,
            UITextAutocapitalizationType::Sentences,
        ),
        (
            Autocapitalize::Characters,
            UITextAutocapitalizationType::AllCharacters,
        ),
    ];
    for (autocapitalize, expected) in capitalization {
        let traits = Traits::from(&TextInputConfiguration {
            autocapitalize,
            ..TextInputConfiguration::default()
        });
        assert_eq!(traits.autocapitalization, expected, "{autocapitalize:?}");
    }

    let return_keys = [
        (TextInputAction::Unspecified, UIReturnKeyType::Default),
        (TextInputAction::Enter, UIReturnKeyType::Default),
        (TextInputAction::Previous, UIReturnKeyType::Default),
        (TextInputAction::Done, UIReturnKeyType::Done),
        (TextInputAction::Go, UIReturnKeyType::Go),
        (TextInputAction::Next, UIReturnKeyType::Next),
        (TextInputAction::Search, UIReturnKeyType::Search),
        (TextInputAction::Send, UIReturnKeyType::Send),
    ];
    for (input_action, expected) in return_keys {
        let traits = Traits::from(&TextInputConfiguration {
            input_action,
            ..TextInputConfiguration::default()
        });
        assert_eq!(traits.return_key, expected, "{input_action:?}");
    }
}
