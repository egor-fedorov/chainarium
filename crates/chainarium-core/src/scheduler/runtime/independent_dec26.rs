use super::*;
use std::cell::Cell;

#[test]
fn independent_one_preflight_per_intent_and_none_for_hold_notready_counted_once() {
    for (name, expected) in [
        ("random", 1),
        ("scripted", 1),
        ("borrower", 1),
        ("spot", 1),
        ("twap", 1),
    ] {
        let s = Scenario::parse(
            &std::fs::read(format!(
                "{}/../../scenarios/demos/sc-agent-{name}.json",
                env!("CARGO_MANIFEST_DIR")
            ))
            .unwrap(),
        )
        .unwrap();
        let parent = World::genesis(&s).unwrap();
        let previous = State::default();
        let mut rng = crate::run::Streams::new(s.seed);
        let probes = Cell::new(0);
        advance_with_rng_check(&s, &parent, &previous, &mut rng, |_| Ok(()), &|_| {
            probes.set(probes.get() + 1);
            Ok(())
        })
        .unwrap();
        assert_eq!(probes.get(), expected, "{name}");
    }
    let mut s = Scenario::parse(include_bytes!(
        "../../../../../scenarios/demos/sc-agent-random.json"
    ))
    .unwrap();
    if let policy::agents::Kind::RandomUser {
        probability_ppm, ..
    } = &mut s.baseline_policies.get_mut("trader").unwrap().kind
    {
        *probability_ppm = 0;
    }
    let parent = World::genesis(&s).unwrap();
    let mut rng = crate::run::Streams::new(s.seed);
    let probes = Cell::new(0);
    advance_with_rng_check(
        &s,
        &parent,
        &State::default(),
        &mut rng,
        |_| Ok(()),
        &|_| {
            probes.set(probes.get() + 1);
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(probes.get(), 0);
    assert_eq!(rng.position("agent/trader"), 4.into());
}

#[test]
fn independent_error_after_paid_revert_does_not_commit_any_rng_or_scheduler_state() {
    let mut s = Scenario::parse(include_bytes!(
        "../../../../../scenarios/demos/sc-agent-random.json"
    ))
    .unwrap();
    if let policy::agents::Kind::RandomUser {
        probability_ppm,
        programs,
        preflight,
        ..
    } = &mut s.baseline_policies.get_mut("trader").unwrap().kind
    {
        *probability_ppm = 1_000_000;
        *programs = vec![vec![
            Call::Transfer {
                to: "bob".into(),
                asset: "Q".into(),
                amount: 10.into(),
            },
            Call::AssertCondition {
                condition: false,
                message: "late revert".into(),
            },
        ]];
        *preflight = policy::agents::Preflight::AnyReceipt;
    }
    let parent = World::genesis(&s).unwrap();
    let previous = State::default();
    let mut rng = crate::run::Streams::new(s.seed);
    let old = (parent.clone(), previous.clone(), rng.clone());
    let result = advance_with_rng_check(
        &s,
        &parent,
        &previous,
        &mut rng,
        |b| {
            assert_eq!(b.receipts.len(), 1);
            assert_eq!(b.receipts[0].status, "ProtocolRevert");
            assert_eq!(b.receipts[0].fee, 125.into());
            assert_eq!(b.state.balance("alice", "Q"), 875.into());
            Err("independent injected commit-boundary failure".into())
        },
        &|_| Ok(()),
    );
    assert!(result.unwrap_err().contains("independent injected"));
    assert_eq!((parent.clone(), previous.clone(), rng.clone()), old);
    let (block, _) = advance(&s, &parent, &previous, &mut rng).unwrap();
    assert_eq!(block.state.balance("alice", "Q"), 875.into());
    assert_eq!(rng.position("agent/trader"), 4.into());
}

mod fix;
