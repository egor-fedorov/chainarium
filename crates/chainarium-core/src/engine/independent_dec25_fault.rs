//! Retained independent DEC-25 fault injection from the review of d4a4709.
use super::*;

#[test]
fn independent_model_error_after_second_hook_tx_aborts_all_working_block() {
    let s = Scenario::parse(include_bytes!(
        "../../../../scenarios/demos/sc-hook-fee.json"
    ))
    .unwrap();
    let parent = World::genesis(&s).unwrap();
    let old = parent.clone();
    let first = s.blocks[0][0].clone();
    let mut second = first.clone();
    second.id = "review-second".into();
    second.nonce = 1.into();
    let mut checks = 0;
    let result = build_with_check(&s, &parent, &[first, second], |attempt| {
        checks += 1;
        if checks == 1 {
            assert_eq!(attempt.hooks["guard"].counter, 2.into());
            assert_eq!(attempt.balance("alice", "Q"), 890.into());
            assert_eq!(attempt.balance("builder", "Q"), 195.into());
            Ok(())
        } else {
            // First output = floor(1000*900000/10900000) = 82.
            // Second = floor(918*900000/11900000) = 69.
            assert_eq!(attempt.hooks["guard"].counter, 4.into());
            assert_eq!(attempt.balance("hook", "Q"), 14.into());
            assert_eq!(attempt.balance("alice", "Q"), 767.into());
            assert_eq!(attempt.balance("builder", "Q"), 390.into());
            Err("review ModelError after two successful hook transactions".into())
        }
    });
    assert_eq!(checks, 2);
    assert_eq!(
        result.unwrap_err(),
        "review ModelError after two successful hook transactions"
    );
    assert_eq!(parent, old);
}
