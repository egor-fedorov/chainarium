use chainarium_core::{
    policy::{
        DecisionAction,
        agents::{Cadence, Kind, Preflight, ScriptEvent},
    },
    *,
};
fn fixture(name: &str) -> Scenario {
    Scenario::parse(
        &std::fs::read(format!(
            "{}/../../scenarios/demos/sc-agent-{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}
fn ds(r: &Run, i: usize) -> &policy::agents::Decision {
    &r.blocks[i].scheduler.as_ref().unwrap().baselines[0]
}
fn random_all(mut s: Scenario) -> Scenario {
    if let Kind::RandomUser {
        probability_ppm,
        programs,
        ..
    } = &mut s.baseline_policies.get_mut("trader").unwrap().kind
    {
        *probability_ppm = 1_000_000;
        *programs = vec![vec![Call::Transfer {
            to: "bob".into(),
            asset: "Q".into(),
            amount: 10.into(),
        }]];
    }
    s
}
fn access_base() -> Scenario {
    let mut s = fixture("borrower");
    s.scheduler.as_mut().unwrap().emissions.clear();
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), 1000.into());
    s.accounts.insert(
        "escrow-alice".into(),
        Account {
            nonce: 0.into(),
            balances: Default::default(),
        },
    );
    s.lending
        .get_mut("credit")
        .unwrap()
        .positions
        .insert("alice".into(), "escrow-alice".into());
    s.accounts.insert(
        "flash".into(),
        Account {
            nonce: 0.into(),
            balances: [("Q".into(), 1000.into())].into(),
        },
    );
    s.flash_providers.insert(
        "loan".into(),
        FlashProvider {
            account: "flash".into(),
            asset: "Q".into(),
            fee_bps: 0,
        },
    );
    let m = fixture("random");
    s.accounts.insert("pool".into(), m.accounts["pool"].clone());
    s.protocols
        .insert("cpmm".into(), m.protocols["cpmm"].clone());
    s.model_versions.extend([
        "strict-flash-provider-v1".into(),
        routing::heterogeneous::MODEL.into(),
        liquidation::MODEL.into(),
    ]);
    let mut p = s.baseline_policies.remove("bob").unwrap();
    p.account = "alice".into();
    p.resource_limit = 1000.into();
    p.resource_price = 0.into();
    p.kind = Kind::RandomUser {
        cadence: Cadence {
            first_block: 1.into(),
            last_block: 6.into(),
            every: 1.into(),
        },
        probability_ppm: 1_000_000,
        programs: vec![vec![Call::AssertCondition {
            condition: true,
            message: "neutral".into(),
        }]],
        preflight: Preflight::SuccessfulOnly,
    };
    s.baseline_policies.insert("alice".into(), p);
    s.validate().unwrap();
    s
}
fn forbidden(which: usize, who: &str) -> Call {
    match which {
        0 => Call::AssertBalance {
            account: who.into(),
            asset: "Q".into(),
            minimum: 1.into(),
        },
        1 => Call::AssertBalanceDelta {
            account: who.into(),
            asset: "Q".into(),
            baseline: 0.into(),
            minimum: 1.into(),
        },
        2 => Call::TransferFrom {
            from: who.into(),
            to: "alice".into(),
            asset: "Q".into(),
            amount: 1.into(),
        },
        3 => Call::Repay {
            market: "credit".into(),
            borrower: who.into(),
            amount: 1.into(),
        },
        4 => Call::Liquidate {
            market: "credit".into(),
            borrower: who.into(),
            amount: 1.into(),
            min_collateral: 0.into(),
        },
        _ => Call::LiquidateAndSell {
            market: "credit".into(),
            borrower: who.into(),
            max_repay: 1.into(),
            min_collateral: 0.into(),
            hops: vec![routing::heterogeneous::VenueHop::Amm {
                venue: "cpmm".into(),
            }],
            min_out: 0.into(),
        },
    }
}
fn wrap(mut call: Call, form: usize) -> Vec<Call> {
    match form {
        0 => (),
        1 => call = Call::Sequence { calls: vec![call] },
        2 => {
            call = Call::FlashLoan {
                provider: "loan".into(),
                receiver: "alice".into(),
                amount: 1.into(),
                callback: vec![call],
            }
        }
        3 => {
            call = Call::Sequence {
                calls: vec![Call::FlashLoan {
                    provider: "loan".into(),
                    receiver: "alice".into(),
                    amount: 1.into(),
                    callback: vec![Call::Sequence { calls: vec![call] }],
                }],
            }
        }
        _ => {
            let count = if matches!(call, Call::LiquidateAndSell { .. }) {
                28
            } else {
                31
            };
            for _ in 0..count {
                call = Call::Sequence { calls: vec![call] };
            }
        }
    };
    vec![call]
}
fn set_program(s: &mut Scenario, program: Vec<Call>, random: bool, mode: Preflight) {
    let p = s.baseline_policies.get_mut("alice").unwrap();
    p.kind = if random {
        Kind::RandomUser {
            cadence: Cadence {
                first_block: 1.into(),
                last_block: 6.into(),
                every: 1.into(),
            },
            probability_ppm: 1_000_000,
            programs: vec![program],
            preflight: mode,
        }
    } else {
        Kind::ScriptedUser {
            schedule: vec![ScriptEvent {
                block: 1.into(),
                program,
            }],
            preflight: mode,
        }
    };
}
#[test]
fn full_foreign_capability_matrix_static_runtime_and_own_positive() {
    let base = access_base();
    let mut n = 0;
    for which in 0..6 {
        for form in 0..5 {
            for random in [false, true] {
                for mode in [Preflight::SuccessfulOnly, Preflight::AnyReceipt] {
                    let mut compare = vec![];
                    for balance in [0u64, 1] {
                        let mut s = base.clone();
                        s.accounts
                            .get_mut("bob")
                            .unwrap()
                            .balances
                            .insert("Q".into(), balance.into());
                        let mut invalid = s.clone();
                        set_program(
                            &mut invalid,
                            wrap(forbidden(which, "bob"), form),
                            random,
                            mode,
                        );
                        let error = invalid.validate().unwrap_err();
                        assert!(
                            error.contains("BaselinePrivateRead"),
                            "{which}/{form}: {error}"
                        );
                        let mut own = s.clone();
                        set_program(
                            &mut own,
                            wrap(forbidden(which, "alice"), form),
                            random,
                            mode,
                        );
                        own.validate().unwrap();
                        let mut r = Run::new(s).unwrap();
                        set_program(
                            &mut r.scenario,
                            wrap(forbidden(which, "bob"), form),
                            random,
                            mode,
                        );
                        r.step().unwrap();
                        let d = ds(&r, 0);
                        assert_eq!(d.action, DecisionAction::Skip);
                        assert!(d.reason.contains("BaselinePrivateRead"));
                        assert!(
                            d.trial.is_none() && d.transaction.is_none() && d.submission.is_none()
                        );
                        assert!(r.blocks[0].receipts.is_empty());
                        assert_eq!(r.state.balance("alice", "Q"), 1000.into());
                        assert_eq!(r.state.accounts["alice"].nonce, 0.into());
                        if random {
                            assert_eq!(d.selection.as_ref().unwrap().position_after, 4.into());
                        } else {
                            assert!(d.selection.is_none());
                        }
                        compare.push(d.clone());
                        n += 1;
                    }
                    assert_eq!(compare[0], compare[1]);
                }
            }
        }
    }
    assert_eq!(n, 240);
}
#[test]
fn known_and_queue_refusals_across_rounds_keep_rng_and_replay() {
    for queue in [false, true] {
        let mut s = random_all(fixture("random"));
        if queue {
            let cfg = s.scheduler.as_mut().unwrap();
            cfg.limits.queue = 1;
            cfg.emissions.push(scheduler::Emission {
                at: 1.into(),
                producer_id: "fill".into(),
                producer_sequence: 0.into(),
                delivery: scheduler::Delivery {
                    channel: scheduler::Channel::Public,
                    builder_delay: 20.into(),
                },
                transaction: Transaction {
                    id: "fill".into(),
                    agent: "recipient".into(),
                    sender: "bob".into(),
                    chain: s.chain.id.clone(),
                    nonce: 0.into(),
                    limit: 200.into(),
                    price: 0.into(),
                    deadline: 100.into(),
                    program: vec![Call::AssertCondition {
                        condition: true,
                        message: "hold".into(),
                    }],
                },
            });
        } else {
            s.scheduler.as_mut().unwrap().limits.known = 1;
        }
        let mut r = Run::new(s).unwrap();
        for i in 0..6 {
            r.step().unwrap();
            let d = ds(&r, i);
            let rep = r.blocks[i].scheduler.as_ref().unwrap();
            assert_eq!(
                d.selection.as_ref().unwrap().position_after,
                ((i as u64 + 1) * 4).into()
            );
            if queue || i > 0 {
                let reason = if queue {
                    scheduler::SubmissionRejection::QueueLimit
                } else {
                    scheduler::SubmissionRejection::KnownTransactionsLimit
                };
                assert_eq!(d.action, DecisionAction::Skip);
                assert_eq!(d.reason, format!("Submission: {}", reason.label()));
                assert_eq!(
                    d.submission,
                    Some(scheduler::SubmissionOutcome::Rejected { reason })
                );
                assert_eq!(d.trial.as_ref().unwrap().fee, 115.into());
                let tx = d.transaction.as_ref().unwrap();
                assert!(rep.trace.iter().any(|t| t.action == "Baseline"
                    && t.detail == d.reason
                    && t.tx_id.as_ref() == Some(&tx.id)));
                assert!(r.blocks[i].receipts.is_empty());
                if queue {
                    assert!(matches!(
                        rep.state.known[&tx.id].committed_status,
                        scheduler::Status::Rejected { .. }
                    ));
                } else {
                    assert!(!rep.state.known.contains_key(&tx.id));
                }
            }
            assert_eq!(
                r.state.balance("alice", "Q"),
                if queue { 1000.into() } else { 875.into() }
            );
            assert_eq!(
                r.state.balance("builder", "Q"),
                if queue { 0.into() } else { 115.into() }
            );
            assert_eq!(Run::restore(&r.checkpoint().unwrap()).unwrap(), r);
        }
    }
}
#[test]
fn observer_queue_limit_does_not_misclassify_successful_builder_delivery() {
    let mut s = random_all(fixture("random"));
    s.scheduler.as_mut().unwrap().limits.queue = 1;
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    for (i, b) in r.blocks.iter().enumerate() {
        let d = ds(&r, i);
        assert_eq!(d.action, DecisionAction::Submit);
        assert_eq!(d.submission, Some(scheduler::SubmissionOutcome::Queued));
        assert!(
            b.scheduler
                .as_ref()
                .unwrap()
                .trace
                .iter()
                .any(|t| t.action == "QueueLimit")
        );
        assert_eq!(b.receipts[0].status, "Success");
    }
    assert_eq!(r.state.balance("alice", "Q"), 250.into());
    assert_eq!(r.state.balance("bob", "Q"), 60.into());
    assert_eq!(r.state.balance("builder", "Q"), 690.into());
}
#[test]
fn queued_expiry_is_a_later_status_with_no_retroactive_submission_rejection() {
    let mut s = random_all(fixture("random"));
    s.baseline_policies.get_mut("trader").unwrap().lifetime = 1.into();
    s.scheduler
        .as_mut()
        .unwrap()
        .agents
        .get_mut("trader")
        .unwrap()
        .outgoing
        .builder_delay = 2.into();
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    for i in 0..6 {
        let d = ds(&r, i);
        assert!(r.blocks[i].receipts.is_empty());
        if i % 2 == 0 {
            assert_eq!(d.action, DecisionAction::Submit);
            assert_eq!(d.submission, Some(scheduler::SubmissionOutcome::Queued));
        } else {
            assert_eq!(d.reason, "BusyNonce");
            assert!(d.submission.is_none());
            let id = format!("policy:baseline:trader:{}", i);
            assert!(
                matches!(&r.blocks[i].scheduler.as_ref().unwrap().state.known[&id].committed_status,scheduler::Status::Rejected{reason} if reason=="ExpiredDeadline")
            );
        }
    }
    assert_eq!(r.state.balance("alice", "Q"), 1000.into());
    assert_eq!(r.state.accounts["alice"].nonce, 0.into());
    assert_eq!(Run::restore(&r.checkpoint().unwrap()).unwrap(), r);
}
#[test]
fn own_assertion_remains_executable_in_both_modes_including_paid_failure() {
    for mode in [Preflight::SuccessfulOnly, Preflight::AnyReceipt] {
        for pass in [false, true] {
            let mut s = fixture("random");
            let p = s.baseline_policies.get_mut("trader").unwrap();
            p.kind = Kind::ScriptedUser {
                schedule: vec![ScriptEvent {
                    block: 1.into(),
                    program: vec![Call::Sequence {
                        calls: vec![Call::AssertBalance {
                            account: "alice".into(),
                            asset: "Q".into(),
                            minimum: if pass { 1.into() } else { 1001.into() },
                        }],
                    }],
                }],
                preflight: mode,
            };
            let mut r = Run::new(s).unwrap();
            r.step().unwrap();
            let d = ds(&r, 0);
            assert_eq!(d.trial.as_ref().unwrap().fee, 120.into());
            if pass || mode == Preflight::AnyReceipt {
                assert_eq!(d.action, DecisionAction::Submit);
                assert_eq!(r.state.balance("alice", "Q"), 880.into());
                assert_eq!(r.state.accounts["alice"].nonce, 1.into());
                assert_eq!(
                    r.blocks[0].receipts[0].status,
                    if pass { "Success" } else { "ProtocolRevert" }
                );
            } else {
                assert_eq!(d.action, DecisionAction::Skip);
                assert!(r.blocks[0].receipts.is_empty());
                assert_eq!(r.state.balance("alice", "Q"), 1000.into());
                assert_eq!(r.state.accounts["alice"].nonce, 0.into());
            }
        }
    }
}
#[test]
fn missing_and_foreign_order_ids_change_diagnostics_but_not_policy_action_or_fee() {
    for mode in [Preflight::SuccessfulOnly, Preflight::AnyReceipt] {
        let mut results = vec![];
        for foreign_exists in [false, true] {
            let mut s = fixture("random");
            s.model_versions.push(book::MODEL.into());
            s.accounts
                .get_mut("bob")
                .unwrap()
                .balances
                .insert("X".into(), 10.into());
            s.accounts.insert(
                "book-escrow".into(),
                Account {
                    nonce: 0.into(),
                    balances: Default::default(),
                },
            );
            s.books.insert(
                "book".into(),
                book::Config {
                    account: "book-escrow".into(),
                    base_asset: "X".into(),
                    quote_asset: "Q".into(),
                    lot_atoms: 1.into(),
                    quote_tick_atoms: 1.into(),
                    taker_fee_bps: 0,
                    fee_account: "builder".into(),
                },
            );
            s.baseline_policies.get_mut("trader").unwrap().kind = Kind::ScriptedUser {
                schedule: vec![ScriptEvent {
                    block: 2.into(),
                    program: vec![Call::CancelOrder {
                        book: "book".into(),
                        order_id: 1.into(),
                    }],
                }],
                preflight: mode,
            };
            let call = if foreign_exists {
                Call::PlaceOrder {
                    book: "book".into(),
                    side: book::Side::Sell,
                    price_ticks: 1.into(),
                    lots: 1.into(),
                    tif: book::TimeInForce::GTC,
                }
            } else {
                Call::AssertCondition {
                    condition: true,
                    message: "no order".into(),
                }
            };
            s.scheduler
                .as_mut()
                .unwrap()
                .emissions
                .push(scheduler::Emission {
                    at: 0.into(),
                    producer_id: "private".into(),
                    producer_sequence: 0.into(),
                    delivery: scheduler::Delivery {
                        channel: scheduler::Channel::Private {
                            recipients: Default::default(),
                        },
                        builder_delay: 0.into(),
                    },
                    transaction: Transaction {
                        id: "private".into(),
                        agent: "recipient".into(),
                        sender: "bob".into(),
                        chain: s.chain.id.clone(),
                        nonce: 0.into(),
                        limit: 500.into(),
                        price: 0.into(),
                        deadline: 20.into(),
                        program: vec![call],
                    },
                });
            let mut r = Run::new(s).unwrap();
            r.step().unwrap();
            r.step().unwrap();
            let d = ds(&r, 1);
            assert_eq!(d.trial.as_ref().unwrap().status, "ProtocolRevert");
            assert_eq!(d.trial.as_ref().unwrap().used, 110.into());
            assert_eq!(
                r.state.balance("alice", "Q"),
                if mode == Preflight::AnyReceipt {
                    890.into()
                } else {
                    1000.into()
                }
            );
            results.push(d.clone());
        }
        assert_eq!(results[0].observation, results[1].observation);
        assert_eq!(results[0].action, results[1].action);
        assert_eq!(results[0].transaction, results[1].transaction);
        assert_ne!(
            results[0].trial.as_ref().unwrap().error,
            results[1].trial.as_ref().unwrap().error
        );
    }
}
