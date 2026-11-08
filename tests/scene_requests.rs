#[path = "../src/window/requests.rs"]
mod requests;

use requests::{Answer, Requests};

#[test]
fn unrequested_restored_scene_with_existing_window_is_unwanted() {
    let mut requests = Requests::<char>::default();
    assert_eq!(
        requests.answer().with_existing_windows(true),
        Answer::Unwanted
    );
}

#[test]
fn launch_and_reopen_scenes_with_empty_stack_stay_available() {
    let mut requests = Requests::<char>::default();
    for _ in 0..2 {
        assert_eq!(
            requests.answer().with_existing_windows(false),
            Answer::Unrequested
        );
    }
}

#[test]
fn requested_scenes_attach_even_when_windows_exist() {
    let mut requests = Requests::default();
    requests.push('a');
    requests.push('b');
    assert_eq!(
        requests.answer().with_existing_windows(true),
        Answer::Window('a')
    );
    assert_eq!(
        requests.answer().with_existing_windows(false),
        Answer::Window('b')
    );
    assert_eq!(
        requests.answer().with_existing_windows(true),
        Answer::Unwanted
    );
}

#[test]
fn closed_and_refused_requests_preserve_the_queue() {
    let mut requests = Requests::default();
    requests.push('a');
    let refused = requests.push('b');
    requests.push('c');
    requests.forget(|window| *window == 'a');
    assert_eq!(requests.refuse(refused), Some('b'));
    assert_eq!(
        requests.answer().with_existing_windows(false),
        Answer::Unwanted
    );
    assert_eq!(
        requests.answer().with_existing_windows(true),
        Answer::Window('c')
    );
}
