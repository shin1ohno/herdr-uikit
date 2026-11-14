use super::*;
use std::rc::Weak;

#[test]
fn checked_scope_exhaustion_never_grants_admission() {
    // Each case is isolated on a new TLS instance; poison is deliberately sticky.
    for case in 0..3 {
        let joined = std::thread::spawn(move || {
            STATE.with_borrow_mut(|s| match case {
                0 => s.serial = u64::MAX,
                1 => s.native = u32::MAX,
                _ => s.loans = u32::MAX,
            });
            let frame = Frame::enter(Kind::Refused);
            let loan = Loan::enter();
            let origin = NativeInputOrigin {
                window: Weak::new(),
                client: 0,
            };
            assert_eq!(
                admission(&origin),
                Err(Error::IdentityExhausted),
                "scope-exhaustion-fails-closed"
            );
            drop(loan);
            drop(frame);
            assert!(!quiescent(), "exhaustion-does-not-create-safe-epilogue");
        })
        .join();
        assert!(joined.is_ok());
    }
}
