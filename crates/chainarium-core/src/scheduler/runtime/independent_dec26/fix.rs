use super::super::*;
fn scenario() -> Scenario {
    Scenario::parse(include_bytes!(
        "../../../../../../scenarios/demos/sc-agent-random.json"
    ))
    .unwrap()
}
fn round<'a>(s: &'a Scenario, parent: &'a World) -> Round<'a> {
    Round {
        s,
        c: s.scheduler.as_ref().unwrap(),
        parent,
        state: State::default(),
        trace: vec![],
        metrics: Metrics::default(),
        probes: vec![],
        search: backrun::Report::default(),
        liquidators: policy::liquidator::Report::default(),
        flash_arbitrage: policy::flash_arbitrage::Report::default(),
        baselines: vec![],
        system_preview: None,
        probe_check: &|_| Ok(()),
        round: 1,
        cutoff: 1.into(),
        intents: BTreeMap::new(),
    }
}
fn tx(s: &Scenario, id: &str) -> Transaction {
    Transaction {
        id: id.into(),
        agent: "trader".into(),
        sender: "alice".into(),
        chain: s.chain.id.clone(),
        nonce: 0.into(),
        limit: 200.into(),
        price: 1.into(),
        deadline: 20.into(),
        program: vec![Call::Transfer {
            to: "bob".into(),
            asset: "Q".into(),
            amount: 10.into(),
        }],
    }
}
#[test]
fn fix_independent_typed_refusals_preserve_queue_registry_and_first_payload() {
    for queue_full in [false, true] {
        let mut s = scenario();
        if queue_full {
            s.scheduler.as_mut().unwrap().limits.queue = 1;
        } else {
            s.scheduler.as_mut().unwrap().limits.known = 1;
        }
        let parent = World::genesis(&s).unwrap();
        let mut r = round(&s, &parent);
        let delivery = s.scheduler.as_ref().unwrap().agents["trader"]
            .outgoing
            .clone();
        let first = tx(&s, "first");
        assert_eq!(
            r.submit(first.clone(), delivery.clone(), 0, 0.into(), None)
                .unwrap(),
            SubmissionOutcome::Queued
        );
        let retained = r.state.clone();
        assert_eq!(
            r.submit(first.clone(), delivery.clone(), 0, 0.into(), None)
                .unwrap(),
            SubmissionOutcome::Rejected {
                reason: SubmissionRejection::Duplicate
            }
        );
        assert_eq!(r.state, retained);
        let mut conflicting = first.clone();
        conflicting.price = 2.into();
        assert_eq!(
            r.submit(conflicting, delivery.clone(), 0, 0.into(), None)
                .unwrap(),
            SubmissionOutcome::Rejected {
                reason: SubmissionRejection::ConflictingTxId
            }
        );
        assert_eq!(r.state, retained);
        let second = tx(&s, "second");
        assert_eq!(
            r.submit(second.clone(), delivery.clone(), 0, 0.into(), None)
                .unwrap(),
            SubmissionOutcome::Rejected {
                reason: if queue_full {
                    SubmissionRejection::QueueLimit
                } else {
                    SubmissionRejection::KnownTransactionsLimit
                }
            }
        );
        assert_eq!(r.state.queue, retained.queue);
        assert_eq!(r.state.next_event_id, retained.next_event_id);
        assert_eq!(r.state.pending, retained.pending);
        assert_eq!(r.state.known["first"], retained.known["first"]);
        if queue_full {
            assert!(
                matches!(&r.state.known["second"].status,Status::Rejected{reason} if reason=="QueueLimit")
            );
            let rejected = r.state.clone();
            assert_eq!(
                r.submit(second, delivery, 0, 0.into(), None).unwrap(),
                SubmissionOutcome::Rejected {
                    reason: SubmissionRejection::Duplicate
                }
            );
            assert_eq!(r.state, rejected);
        } else {
            assert_eq!(r.state, retained);
        }
        assert_eq!(parent.balance("alice", "Q"), 1000.into());
        assert_eq!(parent.accounts["alice"].nonce, 0.into());
    }
}
#[test]
fn fix_independent_runtime_recheck_never_executes_forbidden_program() {
    for mode in [
        policy::agents::Preflight::SuccessfulOnly,
        policy::agents::Preflight::AnyReceipt,
    ] {
        let mut s = scenario();
        let parent = World::genesis(&s).unwrap();
        if let policy::agents::Kind::RandomUser {
            probability_ppm,
            programs,
            preflight,
            ..
        } = &mut s.baseline_policies.get_mut("trader").unwrap().kind
        {
            *probability_ppm = 1_000_000;
            *preflight = mode;
            *programs = vec![vec![Call::Sequence {
                calls: vec![Call::AssertBalance {
                    account: "bob".into(),
                    asset: "Q".into(),
                    minimum: 1.into(),
                }],
            }]];
        }
        let previous = State::default();
        let mut rng = crate::run::Streams::new(s.seed);
        let (b, _) = advance_with_rng_check(&s, &parent, &previous, &mut rng, |_| Ok(()), &|_| {
            panic!("forbidden program reached executor/probe")
        })
        .unwrap();
        let d = &b.scheduler.as_ref().unwrap().baselines[0];
        assert!(d.reason.contains("BaselinePrivateRead"));
        assert!(d.trial.is_none());
        assert_eq!(rng.position("agent/trader"), 4.into());
    }
}
#[test]
fn fix_independent_modelerror_after_queue_rejection_restores_rng_and_state() {
    let mut s = scenario();
    s.scheduler.as_mut().unwrap().limits.queue = 1;
    if let policy::agents::Kind::RandomUser {
        probability_ppm, ..
    } = &mut s.baseline_policies.get_mut("trader").unwrap().kind
    {
        *probability_ppm = 1_000_000;
    }
    let mut fill = tx(&s, "fill");
    fill.agent = "recipient".into();
    fill.sender = "bob".into();
    fill.price = 0.into();
    fill.program = vec![Call::AssertCondition {
        condition: true,
        message: "fill".into(),
    }];
    s.scheduler.as_mut().unwrap().emissions.push(Emission {
        at: 1.into(),
        producer_id: "fill".into(),
        producer_sequence: 0.into(),
        delivery: Delivery {
            channel: Channel::Public,
            builder_delay: 20.into(),
        },
        transaction: fill,
    });
    let parent = World::genesis(&s).unwrap();
    let previous = State::default();
    let mut rng = crate::run::Streams::new(s.seed);
    let before = (parent.clone(), previous.clone(), rng.clone());
    let error = advance_with_rng_check(
        &s,
        &parent,
        &previous,
        &mut rng,
        |b| {
            assert!(b.receipts.is_empty());
            assert_eq!(b.state.balance("alice", "Q"), 1000.into());
            Err("injected after refused submission".into())
        },
        &|_| Ok(()),
    )
    .unwrap_err();
    assert!(error.contains("injected"));
    assert_eq!((parent.clone(), previous.clone(), rng.clone()), before);
    let (b, _) = advance(&s, &parent, &previous, &mut rng).unwrap();
    let d = &b.scheduler.as_ref().unwrap().baselines[0];
    assert_eq!(
        d.submission,
        Some(SubmissionOutcome::Rejected {
            reason: SubmissionRejection::QueueLimit
        })
    );
    assert_eq!(rng.position("agent/trader"), 4.into());
}
