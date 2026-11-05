//! Which hardware keys must reach UIKit before GPUI bindings and terminals.
use gpui::Keystroke;

pub(crate) fn uikit_first(
    key: &Keystroke,
    composing: impl FnOnce() -> bool,
    prefers_composition: impl FnOnce() -> bool,
) -> bool {
    if is_input_source_switch(key) {
        return true;
    }
    if key.modifiers.platform || key.modifiers.control {
        return false;
    }
    composing()
        || (key
            .key_char
            .as_deref()
            .is_some_and(|text| !text.chars().any(char::is_control))
            && prefers_composition())
}

/// Control-Space cycles iPadOS input sources; Shift reverses the cycle.
/// This remains the system's key even when a terminal or custom binding wants it.
pub(crate) fn is_input_source_switch(key: &Keystroke) -> bool {
    key.key == "space"
        && key.modifiers.control
        && !key.modifiers.platform
        && !key.modifiers.alt
        && !key.modifiers.function
}

#[cfg(test)]
mod tests;
