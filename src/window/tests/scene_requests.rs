use crate::window::requests::{Answer, Requests};

#[test]
fn scenes_answer_requests_in_the_order_they_were_made() {
    let mut requests = Requests::default();
    requests.push('a');
    requests.push('b');
    assert_eq!(requests.answer(), Answer::Window('a'));
    assert_eq!(requests.answer(), Answer::Window('b'));
}

#[test]
fn a_scene_nothing_asked_for_waits_for_a_window() {
    let mut requests = Requests::<char>::default();
    assert_eq!(requests.answer(), Answer::Unrequested);
}

#[test]
fn a_refused_request_returns_its_window_and_leaves_the_line() {
    let mut requests = Requests::default();
    let refused = requests.push('a');
    requests.push('b');
    assert_eq!(requests.refuse(refused), Some('a'));
    // The next scene goes to the window still waiting, not the refused one.
    assert_eq!(requests.answer(), Answer::Window('b'));
    assert_eq!(requests.answer(), Answer::Unrequested);
}

#[test]
fn a_request_is_refused_at_most_once() {
    let mut requests = Requests::default();
    let id = requests.push('a');
    assert_eq!(requests.refuse(id), Some('a'));
    assert_eq!(requests.refuse(id), None);
}

#[test]
fn the_scene_for_a_closed_window_is_unwanted_and_not_given_to_the_next() {
    let mut requests = Requests::default();
    requests.push('a');
    requests.push('b');
    requests.forget(|window| *window == 'a');
    assert_eq!(requests.answer(), Answer::Unwanted);
    assert_eq!(requests.answer(), Answer::Window('b'));
}

#[test]
fn refusing_a_closed_windows_request_returns_nothing_and_drops_it() {
    let mut requests = Requests::default();
    let id = requests.push('a');
    requests.forget(|window| *window == 'a');
    assert_eq!(requests.refuse(id), None);
    assert_eq!(requests.answer(), Answer::Unrequested);
}

#[test]
fn request_ids_are_not_reused() {
    let mut requests = Requests::default();
    let first = requests.push('a');
    assert_eq!(requests.answer(), Answer::Window('a'));
    let second = requests.push('b');
    assert_ne!(first, second);
    // A late refusal of the answered request touches the new one not at all.
    assert_eq!(requests.refuse(first), None);
    assert_eq!(requests.answer(), Answer::Window('b'));
}
