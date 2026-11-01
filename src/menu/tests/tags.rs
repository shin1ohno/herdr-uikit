use super::*;

#[test]
fn tags_round_trip_through_a_property_list_number() {
    for (generation, index) in [(0, 0), (1, 7), (42, 65_535), (u32::MAX, u32::MAX)] {
        let tag = MenuTag { generation, index };
        assert_eq!(MenuTag::from(u64::from(tag)), tag);
    }
}

#[test]
fn generation_and_index_occupy_separate_halves() {
    let tag = MenuTag {
        generation: 3,
        index: 9,
    };
    assert_eq!(u64::from(tag), (3 << 32) | 9);
}

#[test]
fn an_index_beyond_u32_has_no_tag() {
    assert_eq!(MenuTag::new(1, 5).map(MenuTag::index), Some(5));
    let too_large = usize::try_from(u64::from(u32::MAX) + 1).unwrap_or(usize::MAX);
    assert_eq!(MenuTag::new(1, too_large), None);
}
