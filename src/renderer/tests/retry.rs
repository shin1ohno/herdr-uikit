use super::*;

#[test]
fn the_first_attempt_is_allowed_without_waiting() {
    let retry = Retry::default();
    assert!(retry.allows(Instant::now()));
}

#[test]
fn a_failure_blocks_attempts_until_the_full_retry_interval_has_elapsed() {
    let start = Instant::now();
    let mut retry = Retry::default();
    retry.failed(start);
    assert!(!retry.allows(start));
    assert!(!retry.allows(start + Duration::from_millis(999)));
    assert!(retry.allows(start + Duration::from_secs(1)));
}

#[test]
fn another_failure_starts_a_fresh_interval() {
    let start = Instant::now();
    let mut retry = Retry::default();
    retry.failed(start);
    let second_attempt = start + Duration::from_secs(1);
    assert!(retry.allows(second_attempt));
    retry.failed(second_attempt);
    assert!(!retry.allows(second_attempt));
    assert!(!retry.allows(second_attempt + Duration::from_millis(999)));
    assert!(retry.allows(second_attempt + Duration::from_secs(1)));
}

#[test]
fn success_clears_the_previous_failure_interval() {
    let start = Instant::now();
    let mut retry = Retry::default();
    retry.failed(start);
    assert!(!retry.allows(start + Duration::from_millis(10)));
    retry.succeeded();
    assert!(retry.allows(start + Duration::from_millis(10)));

    let later_failure = start + Duration::from_millis(20);
    retry.failed(later_failure);
    assert!(!retry.allows(later_failure));
    assert!(retry.allows(later_failure + Duration::from_secs(1)));
}

#[test]
fn an_earlier_sample_does_not_accelerate_the_next_attempt() {
    let earlier = Instant::now();
    let failed = earlier + Duration::from_secs(10);
    let mut retry = Retry::default();
    retry.failed(failed);
    assert!(!retry.allows(earlier));
    assert!(!retry.allows(failed));
    assert!(!retry.allows(failed + Duration::from_millis(999)));
    assert!(retry.allows(failed + Duration::from_secs(1)));
}
