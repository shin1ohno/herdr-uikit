use super::*;
use core_foundation::{base::TCFType, string::CFString};

#[test]
fn boolean_zero_and_one_are_valid_core_text_trait_numbers() {
    assert_eq!(number(&CFBoolean::false_value().as_CFType()), Some(0.0));
    assert_eq!(number(&CFBoolean::true_value().as_CFType()), Some(1.0));
    assert_eq!(number(&CFNumber::from(0.4).as_CFType()), Some(0.4));
    assert_eq!(number(&CFString::new("0").as_CFType()), None);
}

#[test]
fn core_text_weights_keep_font_kits_css_mapping() {
    for &(normalized, css) in WEIGHTS {
        assert_eq!(css_weight(normalized), css);
    }
    assert_eq!(css_weight(0.1), Weight(450.0));
}

#[test]
fn core_text_widths_keep_font_kits_css_mapping() {
    for (normalized, css) in [
        (-1.0, Stretch::ULTRA_CONDENSED),
        (-0.75, Stretch::EXTRA_CONDENSED),
        (-0.5, Stretch::CONDENSED),
        (-0.25, Stretch::SEMI_CONDENSED),
        (0.0, Stretch::NORMAL),
        (0.25, Stretch::SEMI_EXPANDED),
        (0.5, Stretch::EXPANDED),
        (0.75, Stretch::EXTRA_EXPANDED),
        (1.0, Stretch::ULTRA_EXPANDED),
    ] {
        assert_eq!(css_stretch(normalized), css);
    }
}
