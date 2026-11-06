use chainarium_core::{
    policy::{DecisionAction, agents::Kind},
    *,
};
use serde_json::{Value, json};
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
fn run(s: Scenario) -> Run {
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    r
}
fn decision(r: &Run, block: usize) -> &policy::agents::Decision {
    &r.blocks[block].scheduler.as_ref().unwrap().baselines[0]
}
fn reference() -> Value {
    serde_json::from_str(include_str!(
        "../../../docs/reviews/baseline-agents-reference.json"
    ))
    .unwrap()
}
fn a(s: &str) -> Amount {
    s.parse().unwrap()
}
fn amount(v: &Value) -> Amount {
    a(v.as_str().unwrap())
}
fn random_mut(s: &mut Scenario) -> &mut policy::agents::Policy {
    s.baseline_policies.get_mut("trader").unwrap()
}

#[test]
fn random_reference_exact_draws_choices_fees_and_positions() {
    let r = run(fixture("random"));
    let expected = reference();
    let mut raw = vec![];
    for (i, row) in expected["random_user"]["rounds"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let d = decision(&r, i);
        let s = d.selection.as_ref().unwrap();
        assert_eq!(s.gate.value, Some(amount(&row["gate"])));
        assert_eq!(
            s.template.as_ref().unwrap().value,
            Some(amount(&row["choice"]))
        );
        assert_eq!(s.position_before, amount(&row["position_before"]));
        assert_eq!(s.position_after, amount(&row["position_after"]));
        raw.extend(
            s.gate
                .draws
                .iter()
                .chain(&s.template.as_ref().unwrap().draws)
                .map(ToString::to_string),
        );
        assert_eq!(
            d.action == DecisionAction::Submit,
            row["submit"].as_bool().unwrap()
        );
        let w = &r.blocks[i].state;
        for (who, key) in [
            ("alice", "sender_Q"),
            ("bob", "recipient_Q"),
            ("builder", "builder_Q"),
        ] {
            assert_eq!(w.balance(who, "Q"), amount(&row[key]));
        }
        assert_eq!(w.accounts["alice"].nonce, amount(&row["nonce"]));
    }
    assert_eq!(json!(raw), expected["rng"]["first_12_u64"]);
}
#[test]
fn scripted_preflight_and_paid_failure_have_explicit_different_effects() {
    for (name, q, fee, nonce) in [("scripted", 740, 240, 2), ("preflight", 865, 115, 1)] {
        let r = run(fixture(name));
        let first = decision(&r, 0);
        assert_eq!(first.trial.as_ref().unwrap().used, 125.into());
        assert_eq!(first.trial.as_ref().unwrap().status, "ProtocolRevert");
        assert_eq!(r.blocks[0].receipts.len(), usize::from(name == "scripted"));
        assert_eq!(r.state.balance("alice", "Q"), q.into());
        assert_eq!(r.state.balance("bob", "Q"), 20.into());
        assert_eq!(r.state.balance("builder", "Q"), fee.into());
        assert_eq!(r.state.accounts["alice"].nonce, nonce.into());
        assert_eq!(decision(&r, 1).reason, "NotScheduled");
    }
    let mut s = fixture("scripted");
    random_mut(&mut s).resource_limit = 114.into();
    let r = run(s);
    assert_eq!(r.blocks[0].receipts[0].status, "ResourceExhausted");
    assert_eq!(r.blocks[0].receipts[0].used, 114.into());
    assert_eq!(r.blocks[0].state.balance("alice", "Q"), 886.into());
    assert_eq!(r.blocks[0].state.balance("bob", "Q"), 0.into());
}
#[test]
fn borrower_matches_independent_targets_and_cash_accounting() {
    let r = run(fixture("borrower"));
    let expected = reference();
    for (i, row) in expected["borrower"]["rounds"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        let d = decision(&r, i);
        let w = &r.blocks[i].state;
        assert_eq!(
            d.program.first().map(Call::label).unwrap_or("Hold"),
            row["action"].as_str().unwrap()
        );
        assert_eq!(w.balance("bob", "Q"), amount(&row["sender_Q"]));
        assert_eq!(w.balance("bob", "X"), amount(&row["sender_X"]));
        assert_eq!(w.balance("escrow-bob", "X"), amount(&row["collateral"]));
        assert_eq!(w.balance("reserve", "Q"), amount(&row["cash"]));
        let l = &w.lending["credit"];
        assert_eq!(l.debt, amount(&row["debt"]));
        assert_eq!(l.claims, 100.into());
        assert_eq!(l.equity, 0.into());
        assert_eq!(w.accounts["bob"].nonce, amount(&row["nonce"]));
        assert_eq!(
            d.observation.as_ref().unwrap().parent_block,
            (i as u64).into()
        );
        assert_eq!(
            d.observation.as_ref().unwrap().market.block,
            ((i + 1) as u64).into()
        );
    }
}
#[test]
fn borrower_sees_one_accrual_and_never_shrinks_an_unaffordable_step() {
    let mut s = fixture("borrower");
    s.lending.get_mut("credit").unwrap().rate = credit::Rate::Constant {
        rate: a("125000000000000000"),
    };
    s.baseline_policies.get_mut("bob").unwrap().resource_price = 0.into();
    let r = run(s);
    for (i, debt, repay) in [(2, 90, 10), (3, 90, 80), (4, 11, 11)] {
        let d = decision(&r, i);
        assert_eq!(
            d.observation
                .as_ref()
                .unwrap()
                .credit
                .as_ref()
                .unwrap()
                .debt,
            debt.into()
        );
        assert!(matches!(&d.program[0],Call::Repay{amount,..} if *amount==repay.into()));
    }
    assert_eq!(r.state.balance("reserve", "Q"), 121.into());
    assert_eq!(r.state.balance("bob", "Q"), 979.into());
    assert_eq!(r.state.lending["credit"].debt, 0.into());
    assert_eq!(r.state.balance("bob", "X"), 6.into());
    let mut s = fixture("borrower");
    s.baseline_policies.get_mut("bob").unwrap().resource_limit = 114.into();
    let r = run(s);
    assert!(
        r.blocks
            .iter()
            .all(|b| b.receipts.iter().all(|r| r.tx_id == "lender-supply"))
    );
    assert_eq!(r.state.balance("bob", "Q"), 1000.into());
    assert_eq!(r.state.balance("escrow-bob", "X"), 0.into());
    assert_eq!(
        decision(&r, 0).trial.as_ref().unwrap().status,
        "ResourceExhausted"
    );
}
#[test]
fn native_publishers_use_actual_price_and_fixed_twap_end() {
    let r = run(fixture("spot"));
    assert_eq!(
        r.blocks[0].state.feeds["spot"]
            .record
            .as_ref()
            .unwrap()
            .price
            .numerator(),
        20.into()
    );
    assert_eq!(
        r.blocks[1].state.feeds["spot"]
            .record
            .as_ref()
            .unwrap()
            .price
            .numerator(),
        5.into()
    );
    assert_eq!(r.state.balance("oracle", "Q"), 780.into());
    let mut s = fixture("spot");
    s.scheduler
        .as_mut()
        .unwrap()
        .agents
        .get_mut("oracle")
        .unwrap()
        .outgoing
        .builder_delay = 2.into();
    let r = run(s);
    assert!(r.blocks[0].state.feeds["spot"].record.is_none());
    let rec = r.state.feeds["spot"].record.as_ref().unwrap();
    assert_eq!(rec.price.numerator(), 5.into());
    assert_eq!(rec.observed_at, 2.into());
    assert_eq!(decision(&r, 1).reason, "BusyNonce");
    let r = run(fixture("twap"));
    assert_eq!(decision(&r, 0).action, DecisionAction::Skip);
    assert!(r.blocks[0].receipts.is_empty());
    let d = decision(&r, 1);
    assert_eq!(d.trial.as_ref().unwrap().used, 111.into());
    let rec = r.state.feeds["twap"].record.as_ref().unwrap();
    assert_eq!(rec.observed_at, 2.into());
    assert_eq!(rec.sequence, 1.into());
    assert_eq!(rec.price.numerator(), 20.into());
    assert_eq!(r.state.balance("oracle", "Q"), 889.into());
    let mut s = fixture("twap");
    s.scheduler.as_mut().unwrap().rounds = 4;
    s.scheduler
        .as_mut()
        .unwrap()
        .agents
        .get_mut("oracle")
        .unwrap()
        .outgoing
        .builder_delay = 2.into();
    let r = run(s);
    let rec = r.state.feeds["twap"].record.as_ref().unwrap();
    assert_eq!(rec.observed_at, 2.into());
    assert_eq!(rec.published_at, 3.into());
}
#[test]
fn native_multi_feed_is_atomic_and_sequence_exhaustion_skips() {
    let mut s = fixture("twap");
    if let Kind::NativeOraclePublisher { feeds, .. } =
        &mut s.baseline_policies.get_mut("oracle").unwrap().kind
    {
        *feeds = vec!["spot".into(), "twap".into()];
    }
    let r = run(s.clone());
    assert!(r.blocks[0].state.feeds["spot"].record.is_none());
    assert!(r.blocks[0].receipts.is_empty());
    assert_eq!(decision(&r, 1).trial.as_ref().unwrap().used, 121.into());
    assert_eq!(r.state.balance("oracle", "Q"), 879.into());
    s.feeds
        .get_mut("spot")
        .unwrap()
        .publishers
        .insert("alice".into());
    s.scheduler
        .as_mut()
        .unwrap()
        .emissions
        .push(scheduler::Emission {
            at: 0.into(),
            producer_id: "other-publisher".into(),
            producer_sequence: 0.into(),
            delivery: scheduler::Delivery {
                channel: scheduler::Channel::Public,
                builder_delay: 0.into(),
            },
            transaction: Transaction {
                id: "max-sequence".into(),
                agent: "alice".into(),
                sender: "alice".into(),
                chain: s.chain.id.clone(),
                nonce: 0.into(),
                limit: 200.into(),
                price: 0.into(),
                deadline: 10.into(),
                program: vec![Call::PublishSpot {
                    feed: "spot".into(),
                    sequence: Amount::MAX,
                    update_id: "max-sequence".into(),
                }],
            },
        });
    let r = run(s);
    assert_eq!(decision(&r, 1).reason, "OracleSequenceExhausted");
    assert!(r.blocks[1].receipts.is_empty());
    assert_eq!(r.state.balance("oracle", "Q"), 1000.into());
}
#[test]
fn rng_draws_do_not_depend_on_builder_busy_nonce_or_viewing() {
    let s = fixture("random");
    let base = run(s.clone());
    for builder in [
        scheduler::Builder::Fifo,
        scheduler::Builder::FeePriority,
        scheduler::Builder::Greedy,
    ] {
        let mut s = s.clone();
        if builder == scheduler::Builder::Greedy {
            s.model_versions
                .extend(scheduler::bundles::MODELS.map(str::to_owned));
            s.scheduler.as_mut().unwrap().bundles = Some(scheduler::bundles::Config {
                limits: Default::default(),
                emissions: vec![],
            });
        }
        s.scheduler.as_mut().unwrap().builder = builder;
        s.scheduler
            .as_mut()
            .unwrap()
            .agents
            .get_mut("trader")
            .unwrap()
            .outgoing
            .builder_delay = 10.into();
        let mut r = Run::new(s).unwrap();
        for i in 0..6 {
            let before = r.checkpoint().unwrap();
            for _ in 0..10 {
                routing::Observation::capture(&r.scenario, &r.state, "alice").unwrap();
            }
            assert_eq!(r.checkpoint().unwrap(), before);
            r.step().unwrap();
            assert_eq!(decision(&r, i).selection, decision(&base, i).selection);
            if i > 0 {
                assert_eq!(decision(&r, i).reason, "BusyNonce");
            }
        }
        assert_eq!(r.state.accounts["alice"].nonce, 0.into());
        assert_eq!(r.state.balance("alice", "Q"), 1000.into());
    }
}
#[test]
fn cadence_and_probability_extremes_have_fixed_draws() {
    for ppm in [0, 1000000] {
        let mut s = fixture("random");
        if let Kind::RandomUser {
            cadence,
            probability_ppm,
            ..
        } = &mut random_mut(&mut s).kind
        {
            *probability_ppm = ppm;
            cadence.first_block = 2.into();
            cadence.every = 2.into();
        }
        let r = run(s);
        for i in 0..6 {
            let d = decision(&r, i);
            if i % 2 == 0 {
                assert_eq!(d.reason, "NotScheduled");
                assert!(d.selection.is_none());
            } else {
                assert_eq!(
                    d.selection.as_ref().unwrap().position_after,
                    ((i.div_ceil(2) * 4) as u64).into()
                );
                assert_eq!(d.action == DecisionAction::Submit, ppm != 0);
            }
        }
        let raw = serde_json::to_value(&r).unwrap();
        assert_eq!(raw["rng"]["positions"]["agent/trader"], "12");
    }
}
#[test]
fn all_demo_checkpoints_resume_exactly_and_rehashed_rng_is_rejected() {
    for name in [
        "random",
        "scripted",
        "preflight",
        "borrower",
        "spot",
        "twap",
    ] {
        let s = fixture(name);
        let finished = run(s.clone());
        let mut r = Run::new(s).unwrap();
        loop {
            let bytes = r.checkpoint().unwrap();
            let mut restored = Run::restore(&bytes).unwrap();
            restored.finish().unwrap();
            assert_eq!(restored, finished, "{name}");
            if r.step().unwrap().is_none() {
                break;
            }
        }
    }
    let mut r = Run::new(fixture("random")).unwrap();
    r.step().unwrap();
    let mut cp: Value = serde_json::from_slice(&r.checkpoint().unwrap()).unwrap();
    cp["run"]["rng"]["positions"]["agent/trader"] = json!("8");
    let changed: Run = serde_json::from_value(cp["run"].clone()).unwrap();
    cp["hash"] = json!(run::digest(&serde_json::to_vec(&changed).unwrap()));
    assert!(
        Run::restore(&serde_json::to_vec(&cp).unwrap())
            .unwrap_err()
            .contains("replay mismatch")
    );
}
#[test]
fn config_caps_roles_and_unordered_script_events() {
    let source = serde_json::to_value(fixture("random")).unwrap();
    let mut v = source.clone();
    v["baseline_policies"]["trader"]["kind"]["probability_ppm"] = json!(1000001);
    assert!(Scenario::parse(&serde_json::to_vec(&v).unwrap()).is_err());
    for n in [0, 1, 16, 17] {
        let mut v = source.clone();
        let p = v["baseline_policies"]["trader"]["kind"]["programs"][0].clone();
        v["baseline_policies"]["trader"]["kind"]["programs"] = json!(vec![p; n]);
        assert_eq!(
            Scenario::parse(&serde_json::to_vec(&v).unwrap()).is_ok(),
            n == 1 || n == 16
        );
    }
    let mut s = fixture("scripted");
    if let Kind::ScriptedUser { schedule, .. } = &mut random_mut(&mut s).kind {
        schedule.reverse();
    }
    s.validate().unwrap();
    let mut v = serde_json::to_value(s).unwrap();
    let event = v["baseline_policies"]["trader"]["kind"]["schedule"][0].clone();
    v["baseline_policies"]["trader"]["kind"]["schedule"] = json!([event.clone(), event]);
    assert!(Scenario::parse(&serde_json::to_vec(&v).unwrap()).is_err());
    for field in ["every", "first_block", "last_block"] {
        let mut v = source.clone();
        v["baseline_policies"]["trader"]["kind"]["cadence"][field] = json!("0");
        assert!(Scenario::parse(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    for account in ["pool", "bob", "builder"] {
        let mut v = source.clone();
        v["baseline_policies"]["trader"]["account"] = json!(account);
        assert!(Scenario::parse(&serde_json::to_vec(&v).unwrap()).is_err());
    }
    let mut s = fixture("random");
    s.model_versions.push("on-visible-program-v1".into());
    s.scheduler.as_mut().unwrap().reactive.insert(
        "trader".into(),
        scheduler::ReactivePolicy {
            account: "alice".into(),
            resource_limit: 200.into(),
            resource_price: 0.into(),
            reaction_delay: 0.into(),
            lifetime: 1.into(),
            rules: [(
                "trigger".into(),
                vec![Call::Transfer {
                    to: "bob".into(),
                    asset: "Q".into(),
                    amount: 1.into(),
                }],
            )]
            .into(),
        },
    );
    assert!(s.validate().unwrap_err().contains("duplicate baseline"));
    let mut s = fixture("random");
    s.scheduler = None;
    s.blocks = vec![vec![]];
    s.model_versions
        .retain(|m| !scheduler::MODELS.contains(&m.as_str()));
    s.model_versions.push("scheduler-m1-v1".into());
    assert!(s.validate().is_err());
}

#[test]
fn exact_schedule_and_native_feed_caps_are_enforced() {
    for n in [1024usize, 1025] {
        let mut s = fixture("scripted");
        s.scheduler.as_mut().unwrap().rounds = 1025;
        if let Kind::ScriptedUser { schedule, .. } = &mut random_mut(&mut s).kind {
            let original = schedule[0].program.clone();
            *schedule = (1..=n)
                .map(|b| policy::agents::ScriptEvent {
                    block: (b as u64).into(),
                    program: original.clone(),
                })
                .collect();
        }
        assert_eq!(s.validate().is_ok(), n == 1024);
    }
    for n in [1usize, 16, 17] {
        let mut s = fixture("spot");
        s.scheduler.as_mut().unwrap().emissions.clear();
        s.scheduler.as_mut().unwrap().rounds = 1;
        let cfg = s.feeds["spot"].clone();
        let names: Vec<_> = (0..n).map(|i| format!("price-{i}")).collect();
        for name in &names {
            s.feeds.insert(name.clone(), cfg.clone());
        }
        if let Kind::NativeOraclePublisher { feeds, cadence } =
            &mut s.baseline_policies.get_mut("oracle").unwrap().kind
        {
            *feeds = names;
            cadence.last_block = 1.into();
        }
        if n == 17 {
            assert!(s.validate().is_err());
            continue;
        }
        let r = run(s);
        assert_eq!(r.blocks[0].receipts[0].used, (100 + 10 * n as u64).into());
        assert_eq!(r.state.accounts["oracle"].nonce, 1.into());
    }
}
#[test]
fn borrower_ltv_freshness_and_cash_failures_do_not_rewrite_targets() {
    for (case, target) in [
        ("boundary", 90u64),
        ("ltv", 91),
        ("stale", 80),
        ("cash", 80),
    ] {
        let mut s = fixture("borrower");
        if let Kind::BorrowerTarget {
            targets,
            max_debt_step,
            ..
        } = &mut s.baseline_policies.get_mut("bob").unwrap().kind
        {
            targets.truncate(1);
            targets[0].debt = target.into();
            *max_debt_step = 100.into();
        }
        if case == "stale" {
            s.lending.get_mut("credit").unwrap().max_age = 1.into();
        }
        if case == "cash" {
            let tx = &mut s.scheduler.as_mut().unwrap().emissions[0].transaction;
            tx.program = vec![Call::Supply {
                market: "credit".into(),
                amount: 50.into(),
                min_shares: 50.into(),
            }];
        }
        let r = run(s);
        let d = decision(&r, 1);
        assert_eq!(d.target.as_ref().unwrap().debt, target.into());
        assert!(matches!(&d.program[0],Call::Borrow{amount,..} if *amount==target.into()));
        assert_eq!(d.action == DecisionAction::Submit, case == "boundary");
        assert_eq!(
            r.state.lending["credit"].debt,
            if case == "boundary" {
                90.into()
            } else {
                0.into()
            }
        );
        if case != "boundary" {
            assert_eq!(r.state.accounts["bob"].nonce, 1.into());
            assert_eq!(r.state.balance("bob", "Q"), 885.into());
        }
    }
}
#[test]
fn nonce_deadline_and_delivery_overflow_skip_after_drawing() {
    let mut s = fixture("random");
    s.accounts.get_mut("alice").unwrap().nonce = Amount::MAX;
    let r = run(s);
    assert!(r.blocks.iter().all(|b| b.receipts.is_empty()));
    for i in 0..6 {
        assert_eq!(decision(&r, i).reason, "NonceExhausted");
        assert_eq!(
            decision(&r, i).selection.as_ref().unwrap().position_after,
            ((i as u64 + 1) * 4).into()
        );
    }
    for which in ["deadline", "delivery"] {
        let mut s = fixture("random");
        if which == "deadline" {
            random_mut(&mut s).lifetime = Amount::MAX;
        } else {
            s.scheduler
                .as_mut()
                .unwrap()
                .agents
                .get_mut("trader")
                .unwrap()
                .outgoing
                .builder_delay = Amount::MAX;
        }
        let mut r = Run::new(s).unwrap();
        r.state.timestamp = 1.into();
        r.step().unwrap();
        assert_eq!(
            decision(&r, 0).reason,
            if which == "deadline" {
                "DeadlineOverflow"
            } else {
                "DeliveryTimeOverflow"
            }
        );
        assert_eq!(
            decision(&r, 0).selection.as_ref().unwrap().position_after,
            4.into()
        );
        assert!(r.blocks[0].receipts.is_empty());
    }
}
#[test]
fn delayed_expired_intents_and_scripted_busy_events_have_no_hidden_retry() {
    let mut s = fixture("random");
    random_mut(&mut s).lifetime = 1.into();
    s.scheduler
        .as_mut()
        .unwrap()
        .agents
        .get_mut("trader")
        .unwrap()
        .outgoing
        .builder_delay = 2.into();
    let r = run(s);
    assert!(r.blocks.iter().all(|b| b.receipts.is_empty()));
    assert_eq!(r.state.balance("alice", "Q"), 1000.into());
    assert_eq!(decision(&r, 1).reason, "BusyNonce");
    assert_eq!(decision(&r, 3).action, DecisionAction::Submit);
    let mut s = fixture("scripted");
    s.scheduler.as_mut().unwrap().rounds = 6;
    s.scheduler
        .as_mut()
        .unwrap()
        .agents
        .get_mut("trader")
        .unwrap()
        .outgoing
        .builder_delay = 4.into();
    let r = run(s);
    assert_eq!(decision(&r, 2).reason, "BusyNonce");
    assert_eq!(r.blocks.iter().map(|b| b.receipts.len()).sum::<usize>(), 1);
    assert_eq!(r.state.balance("bob", "Q"), 0.into());
    assert_eq!(r.state.balance("alice", "Q"), 875.into());
}
#[test]
fn preflight_success_can_become_an_included_paid_revert() {
    let mut s = fixture("scripted");
    let (pool, cfg) = s.protocols.iter_mut().next().unwrap();
    let pool = pool.clone();
    cfg.fee_bps = 0;
    let account = cfg.account.clone();
    s.accounts
        .get_mut(&account)
        .unwrap()
        .balances
        .insert("X".into(), 1000.into());
    s.accounts
        .get_mut(&account)
        .unwrap()
        .balances
        .insert("Q".into(), 1000.into());
    let own_x = s.accounts["alice"].balances["X"];
    if let Kind::ScriptedUser {
        schedule,
        preflight,
    } = &mut random_mut(&mut s).kind
    {
        *preflight = policy::agents::Preflight::SuccessfulOnly;
        schedule.truncate(1);
        schedule[0].program = vec![Call::SwapExactIn {
            pool: pool.clone(),
            asset_in: "X".into(),
            amount: 100.into(),
            min_out: 90.into(),
        }];
    }
    s.accounts
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("X".into(), 1000.into());
    s.scheduler
        .as_mut()
        .unwrap()
        .agents
        .get_mut("trader")
        .unwrap()
        .outgoing
        .builder_delay = 1.into();
    s.scheduler
        .as_mut()
        .unwrap()
        .emissions
        .push(scheduler::Emission {
            at: 0.into(),
            producer_id: "market-move".into(),
            producer_sequence: 0.into(),
            delivery: scheduler::Delivery {
                channel: scheduler::Channel::Public,
                builder_delay: 0.into(),
            },
            transaction: Transaction {
                id: "market-move".into(),
                agent: "recipient".into(),
                sender: "bob".into(),
                chain: s.chain.id.clone(),
                nonce: 0.into(),
                limit: 500.into(),
                price: 0.into(),
                deadline: 10.into(),
                program: vec![Call::SwapExactIn {
                    pool,
                    asset_in: "X".into(),
                    amount: 1000.into(),
                    min_out: 1.into(),
                }],
            },
        });
    let r = run(s);
    let d = decision(&r, 0);
    assert_eq!(d.trial.as_ref().unwrap().status, "Success");
    assert_eq!(d.trial.as_ref().unwrap().used, 120.into());
    let receipt = r.blocks[0]
        .receipts
        .iter()
        .find(|r| r.tx_id.starts_with("policy:baseline:"))
        .unwrap();
    assert_eq!(receipt.status, "ProtocolRevert");
    assert_eq!(receipt.used, 110.into());
    assert_eq!(r.state.balance("alice", "Q"), 890.into());
    assert_eq!(r.state.balance("alice", "X"), own_x);
}
#[test]
fn future_private_orderflow_and_other_wallets_do_not_enter_baseline_observation() {
    let mut base = fixture("random");
    base.accounts
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("Q".into(), 100.into());
    base.accounts.insert(
        "hidden".into(),
        Account {
            nonce: 0.into(),
            balances: [("Q".into(), 0.into())].into(),
        },
    );
    let mut changed = base.clone();
    changed
        .accounts
        .get_mut("hidden")
        .unwrap()
        .balances
        .insert("Q".into(), 999.into());
    for (scenario, amount) in [(&mut base, 1u64), (&mut changed, 2)] {
        let network = scenario.scheduler.as_mut().unwrap();
        let tx = Transaction {
            id: "private-other-wallet".into(),
            agent: "recipient".into(),
            sender: "bob".into(),
            chain: scenario.chain.id.clone(),
            nonce: 0.into(),
            limit: 200.into(),
            price: 0.into(),
            deadline: 20.into(),
            program: vec![Call::Transfer {
                to: "hidden".into(),
                asset: "Q".into(),
                amount: amount.into(),
            }],
        };
        network.emissions.push(scheduler::Emission {
            at: 0.into(),
            producer_id: "private".into(),
            producer_sequence: 0.into(),
            transaction: tx.clone(),
            delivery: scheduler::Delivery {
                channel: scheduler::Channel::Private {
                    recipients: Default::default(),
                },
                builder_delay: 0.into(),
            },
        });
        let mut future = tx;
        future.id = "future".into();
        future.nonce = 1.into();
        future.program = vec![Call::Transfer {
            to: "hidden".into(),
            asset: "Q".into(),
            amount: (amount * 10).into(),
        }];
        network.emissions.push(scheduler::Emission {
            at: 5.into(),
            producer_id: "future".into(),
            producer_sequence: 0.into(),
            transaction: future,
            delivery: scheduler::Delivery {
                channel: scheduler::Channel::Public,
                builder_delay: 0.into(),
            },
        });
    }
    let mut left = Run::new(base).unwrap();
    let mut right = Run::new(changed).unwrap();
    for i in 0..4 {
        left.step().unwrap();
        right.step().unwrap();
        assert_eq!(decision(&left, i), decision(&right, i));
    }
}

#[test]
fn foreign_wallet_access_is_rejected_recursively_for_both_preflight_modes() {
    use policy::agents::{Preflight, ScriptEvent};
    let calls = [
        Call::AssertBalance {
            account: "bob".into(),
            asset: "Q".into(),
            minimum: 1.into(),
        },
        Call::AssertBalanceDelta {
            account: "bob".into(),
            asset: "Q".into(),
            baseline: 0.into(),
            minimum: 1.into(),
        },
        Call::TransferFrom {
            from: "bob".into(),
            to: "alice".into(),
            asset: "Q".into(),
            amount: 1.into(),
        },
    ];
    for call in calls {
        for mode in [Preflight::SuccessfulOnly, Preflight::AnyReceipt] {
            for random in [false, true] {
                for nesting in 0..3 {
                    let mut s = fixture("random");
                    s.flash_providers.insert(
                        "loan".into(),
                        FlashProvider {
                            account: "pool-b".into(),
                            asset: "Q".into(),
                            fee_bps: 0,
                        },
                    );
                    s.protocols.remove("second");
                    let program = vec![match nesting {
                        0 => call.clone(),
                        1 => Call::Sequence {
                            calls: vec![call.clone()],
                        },
                        _ => Call::Sequence {
                            calls: vec![Call::FlashLoan {
                                provider: "loan".into(),
                                receiver: "alice".into(),
                                amount: 1.into(),
                                callback: vec![Call::Sequence {
                                    calls: vec![call.clone()],
                                }],
                            }],
                        },
                    }];
                    let p = random_mut(&mut s);
                    if random {
                        if let Kind::RandomUser {
                            programs,
                            preflight,
                            ..
                        } = &mut p.kind
                        {
                            *programs = vec![program];
                            *preflight = mode;
                        }
                    } else {
                        p.kind = Kind::ScriptedUser {
                            schedule: vec![ScriptEvent {
                                block: 1.into(),
                                program,
                            }],
                            preflight: mode,
                        };
                    }
                    let error = s.validate().unwrap_err();
                    assert!(error.contains("BaselinePrivateRead"), "{error}");
                }
            }
        }
    }
}

#[test]
fn foreign_credit_probes_are_rejected_but_own_repayment_is_allowed() {
    use policy::agents::{Preflight, ScriptEvent};
    let mut base = fixture("borrower");
    let market_fixture = fixture("random");
    base.accounts
        .insert("pool".into(), market_fixture.accounts["pool"].clone());
    base.protocols
        .insert("cpmm".into(), market_fixture.protocols["cpmm"].clone());
    base.model_versions.extend([
        routing::heterogeneous::MODEL.into(),
        liquidation::MODEL.into(),
    ]);
    let mut p = base.baseline_policies.remove("bob").unwrap();
    p.account = "alice".into();
    base.baseline_policies.insert("alice".into(), p);
    for call in [
        Call::Repay {
            market: "credit".into(),
            borrower: "bob".into(),
            amount: 1.into(),
        },
        Call::Liquidate {
            market: "credit".into(),
            borrower: "bob".into(),
            amount: 1.into(),
            min_collateral: 0.into(),
        },
        Call::LiquidateAndSell {
            market: "credit".into(),
            borrower: "bob".into(),
            max_repay: 1.into(),
            min_collateral: 0.into(),
            hops: vec![routing::heterogeneous::VenueHop::Amm {
                venue: "cpmm".into(),
            }],
            min_out: 0.into(),
        },
    ] {
        let mut s = base.clone();
        s.baseline_policies.get_mut("alice").unwrap().kind = Kind::ScriptedUser {
            schedule: vec![ScriptEvent {
                block: 1.into(),
                program: vec![Call::Sequence { calls: vec![call] }],
            }],
            preflight: Preflight::AnyReceipt,
        };
        let error = s.validate().unwrap_err();
        assert!(error.contains("BaselinePrivateRead"), "{error}");
    }
    // Existing borrower demo repays only its own accrued debt and must still execute.
    let r = run(fixture("borrower"));
    assert!(
        matches!(decision(&r, 3).program[0], Call::Repay { ref borrower, .. } if borrower=="bob")
    );
    assert_eq!(decision(&r, 3).action, DecisionAction::Submit);
}

#[test]
fn changed_template_cannot_bypass_runtime_capability_check() {
    let mut results = vec![];
    for balance in [0u64, 1] {
        let mut s = fixture("random");
        s.accounts
            .get_mut("bob")
            .unwrap()
            .balances
            .insert("Q".into(), balance.into());
        let mut r = Run::new(s).unwrap();
        // Defensive API test: mutate a previously validated config after genesis.
        if let Kind::RandomUser {
            programs,
            probability_ppm,
            ..
        } = &mut random_mut(&mut r.scenario).kind
        {
            *probability_ppm = 1_000_000;
            *programs = vec![vec![Call::Sequence {
                calls: vec![Call::AssertBalance {
                    account: "bob".into(),
                    asset: "Q".into(),
                    minimum: 1.into(),
                }],
            }]];
        }
        r.step().unwrap();
        let d = decision(&r, 0);
        assert_eq!(d.action, DecisionAction::Skip);
        assert!(d.reason.contains("BaselinePrivateRead"));
        assert!(d.trial.is_none() && d.transaction.is_none() && d.submission.is_none());
        assert!(r.blocks[0].receipts.is_empty());
        assert_eq!(r.state.balance("alice", "Q"), 1000.into());
        assert_eq!(r.state.accounts["alice"].nonce, 0.into());
        assert_eq!(d.selection.as_ref().unwrap().position_after, 4.into());
        results.push(d.clone());
    }
    assert_eq!(results[0], results[1]);
}

#[test]
fn submission_limits_preserve_attempt_and_trial_with_explicit_skip() {
    use scheduler::{SubmissionOutcome, SubmissionRejection};
    for queue_full in [false, true] {
        let mut s = fixture("random");
        if let Kind::RandomUser {
            programs,
            probability_ppm,
            ..
        } = &mut random_mut(&mut s).kind
        {
            *probability_ppm = 1_000_000;
            *programs = vec![vec![Call::Transfer {
                to: "bob".into(),
                asset: "Q".into(),
                amount: 10.into(),
            }]];
        }
        if queue_full {
            let cfg = s.scheduler.as_mut().unwrap();
            cfg.limits.queue = 1;
            cfg.emissions.push(scheduler::Emission {
                at: 1.into(),
                producer_id: "fills-queue".into(),
                producer_sequence: 0.into(),
                delivery: scheduler::Delivery {
                    channel: scheduler::Channel::Public,
                    builder_delay: 20.into(),
                },
                transaction: Transaction {
                    id: "fills-queue".into(),
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
        r.step().unwrap();
        if !queue_full {
            assert_eq!(decision(&r, 0).submission, Some(SubmissionOutcome::Queued));
            r.step().unwrap();
        }
        let i = usize::from(!queue_full);
        let d = decision(&r, i);
        let reason = if queue_full {
            SubmissionRejection::QueueLimit
        } else {
            SubmissionRejection::KnownTransactionsLimit
        };
        assert_eq!(d.action, DecisionAction::Skip);
        assert_eq!(d.reason, format!("Submission: {}", reason.label()));
        assert_eq!(d.submission, Some(SubmissionOutcome::Rejected { reason }));
        assert_eq!(d.trial.as_ref().unwrap().status, "Success");
        assert_eq!(d.trial.as_ref().unwrap().fee, 115.into());
        assert!(d.transaction.is_some());
        assert!(r.blocks[i].receipts.is_empty());
        assert_eq!(r.state.accounts["alice"].nonce, (i as u64).into());
        assert_eq!(
            r.state.balance("alice", "Q"),
            (1000 - 125 * i as u64).into()
        );
        assert_eq!(r.state.balance("builder", "Q"), (115 * i as u64).into());
        assert_eq!(Run::restore(&r.checkpoint().unwrap()).unwrap(), r);
    }
}
