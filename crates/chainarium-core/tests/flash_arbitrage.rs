use chainarium_core::{
    policy::{DecisionAction, flash_arbitrage as f},
    scheduler::{Channel, SubmissionOutcome, SubmissionRejection},
    *,
};
use serde_json::{Value, json};

fn fixture(name: &str) -> Scenario {
    Scenario::parse(
        &std::fs::read(format!(
            "{}/../../scenarios/demos/sc-flash-arb-{name}.json",
            env!("CARGO_MANIFEST_DIR")
        ))
        .unwrap(),
    )
    .unwrap()
}
fn cfg(s: &mut Scenario) -> &mut f::Config {
    s.scheduler
        .as_mut()
        .unwrap()
        .flash_arbitrage
        .as_mut()
        .unwrap()
}
fn policy(s: &mut Scenario) -> &mut f::Policy {
    cfg(s).policies.get_mut("trader").unwrap()
}
fn first(s: Scenario) -> Run {
    let mut run = Run::new(s).unwrap();
    run.step().unwrap();
    run
}
fn report(r: &Run, i: usize) -> &f::Report {
    r.blocks[i]
        .scheduler
        .as_ref()
        .unwrap()
        .flash_arbitrage
        .as_ref()
        .unwrap()
}
fn decision(r: &Run, i: usize) -> &f::Decision {
    &report(r, i).decisions[0]
}
fn amount(v: &Value) -> Amount {
    v.as_str().unwrap().parse().unwrap()
}
fn reference(name: &str) -> Value {
    serde_json::from_str::<Value>(include_str!(
        "../../../docs/reviews/flash-arbitrage-reference.json"
    ))
    .unwrap()["cases"][name]
        .clone()
}

#[test]
fn exact_reference_separates_principal_flash_fee_and_native_fee() {
    let r = first(fixture("success"));
    let d = decision(&r, 0);
    let row = reference("scaled_paid");
    assert_eq!(d.action, DecisionAction::Submit);
    assert_eq!(d.submission, Some(SubmissionOutcome::Queued));
    assert_eq!(d.candidates.len(), 2); // Both directions of the two-venue cycle.
    let best = &d.candidates[d.selected.unwrap()];
    assert_eq!(
        best.hops.iter().map(|h| h.venue()).collect::<Vec<_>>(),
        vec!["cpmm", "second"]
    );
    assert_eq!(best.net, Some(amount(&row["projected_net"])));
    let o = d.observation.as_ref().unwrap();
    assert_eq!(
        (o.flash_fee, o.repayment, o.fee_reserve),
        (Some(50.into()), Some(100050.into()), 300.into())
    );
    assert_eq!(
        (o.parent_block, o.market.block, o.market.timestamp),
        (0.into(), 1.into(), 1.into())
    );
    assert_eq!(r.state.balance("alice", "Q"), amount(&row["own_after"]));
    assert_eq!(r.state.balance("alice", "X"), 0.into());
    assert_eq!(r.state.balance("loan", "Q"), 10000050.into());
    assert_eq!(r.state.balance("builder", "Q"), 190.into());
    assert_eq!(r.blocks[0].receipts[0].used, amount(&row["used"]));
    for (account, asset, key) in [
        ("pool", "X", "first_pool_x_after"),
        ("pool", "Q", "first_pool_q_after"),
        ("pool-b", "X", "second_pool_x_after"),
        ("pool-b", "Q", "second_pool_q_after"),
    ] {
        assert_eq!(r.state.balance(account, asset), amount(&row[key]));
    }
}

#[test]
fn fee_reserve_cannot_be_funded_by_loan_and_small_free_cycle_is_real_profit() {
    let mut s = fixture("unprofitable");
    policy(&mut s).resource_price = 0.into();
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), 0.into());
    let r = first(s);
    assert_eq!(r.state.balance("alice", "Q"), 22.into());
    assert_eq!(r.state.balance("loan", "Q"), 10001.into());
    assert_eq!(r.state.balance("builder", "Q"), 0.into());
    let r = first(fixture("unprofitable"));
    assert_eq!(decision(&r, 0).reason, "NoProfitableCycle");
    assert!(r.blocks[0].receipts.is_empty());
    assert_eq!(r.state.balance("alice", "Q"), 1000.into());
    for (kind, reason) in [("budget", "OwnFeeBudget"), ("success", "ProviderCash")] {
        let mut s = fixture(kind);
        if kind == "success" {
            s.accounts
                .get_mut("loan")
                .unwrap()
                .balances
                .insert("Q".into(), 99999.into());
        }
        let r = first(s);
        assert_eq!(decision(&r, 0).reason, reason);
        assert_eq!(report(&r, 0).metrics.trials, 0);
        assert_eq!(report(&r, 0).metrics.venue_checks, 0);
        assert!(r.blocks[0].receipts.is_empty());
    }
}

#[test]
fn exact_minimum_and_resource_boundaries_do_not_spend_probe_fees() {
    for (minimum, limit, success) in [
        (24104u64, 300u64, true),
        (24105, 300, false),
        (1, 190, true),
        (1, 189, false),
    ] {
        let mut s = fixture("success");
        policy(&mut s).minimum_net_profit = minimum.into();
        policy(&mut s).resource_limit = limit.into();
        let r = first(s);
        assert_eq!(decision(&r, 0).action == DecisionAction::Submit, success);
        assert_eq!(r.blocks[0].receipts.len(), usize::from(success));
        assert_eq!(
            r.state.balance("alice", "Q"),
            if success { 25104.into() } else { 1000.into() }
        );
        assert_eq!(r.state.accounts["alice"].nonce, u64::from(success).into());
    }
}

#[test]
fn actual_prefix_can_break_net_guard_or_settlement_and_preserves_only_prefix_fee_nonce() {
    for (name, ref_name, error) in [
        ("guard", "late_guard_revert", "net balance delta assertion"),
        (
            "shortfall",
            "late_settlement_shortfall",
            "insufficient balance",
        ),
    ] {
        let r = first(fixture(name));
        let row = reference(ref_name);
        let d = decision(&r, 0);
        assert_eq!(d.candidates[d.selected.unwrap()].net, Some(24104.into()));
        let actual = r.blocks[0]
            .receipts
            .iter()
            .find(|r| r.tx_id.starts_with("policy:flash-arbitrage:"))
            .unwrap();
        assert_eq!(actual.status, "ProtocolRevert");
        assert!(
            actual.error.as_ref().unwrap().contains(error),
            "{:?}",
            actual.error
        );
        assert_eq!(actual.used, amount(&row["used"]));
        assert_eq!(r.state.balance("alice", "Q"), amount(&row["own_after"]));
        assert_eq!(r.state.balance("loan", "Q"), 10000000.into());
        assert_eq!(r.state.accounts["alice"].nonce, 1.into());
        for (account, asset, key) in [
            ("pool", "X", "first_pool_x_after"),
            ("pool", "Q", "first_pool_q_after"),
            ("pool-b", "X", "second_pool_x_after"),
            ("pool-b", "Q", "second_pool_q_after"),
        ] {
            assert_eq!(r.state.balance(account, asset), amount(&row[key]));
        }
    }
}

#[test]
fn delayed_intent_uses_one_nonce_and_private_channel_exposes_only_recipients() {
    let mut s = fixture("delayed");
    s.scheduler
        .as_mut()
        .unwrap()
        .agents
        .get_mut("trader")
        .unwrap()
        .outgoing
        .channel = Channel::Private {
        recipients: Default::default(),
    };
    let mut r = Run::new(s).unwrap();
    for i in 0..3 {
        r.step().unwrap();
        assert_eq!(r, Run::restore(&r.checkpoint().unwrap()).unwrap());
        assert_eq!(
            decision(&r, i).reason,
            if i == 0 {
                "ProfitableFlashCycle"
            } else {
                "BusyNonce"
            }
        );
        assert!(
            r.scheduler
                .as_ref()
                .unwrap()
                .visible
                .values()
                .all(|messages| messages.is_empty())
        );
    }
    assert_eq!(r.state.balance("alice", "Q"), 25104.into());
    assert_eq!(r.state.accounts["alice"].nonce, 1.into());
    assert_eq!(
        r.blocks[2].receipts[0].tx_id,
        "policy:flash-arbitrage:trader:1"
    );
}

#[test]
fn every_demo_checkpoint_resumes_exactly_without_repeating_search_or_rng() {
    for name in [
        "success",
        "unprofitable",
        "guard",
        "shortfall",
        "delayed",
        "budget",
    ] {
        let s = fixture(name);
        let mut whole = Run::new(s.clone()).unwrap();
        whole.finish().unwrap();
        for split in 0..=s.rounds() {
            let mut r = Run::new(s.clone()).unwrap();
            for _ in 0..split {
                r.step().unwrap();
            }
            let saved = r.checkpoint().unwrap();
            assert_eq!(r.checkpoint().unwrap(), saved);
            let mut restored = Run::restore(&saved).unwrap();
            restored.finish().unwrap();
            assert_eq!(restored, whole);
        }
        let raw = serde_json::to_value(&whole).unwrap();
        assert_eq!(raw["rng"]["positions"], json!({}));
    }
}

#[test]
fn configuration_rejects_missing_profiles_duplicate_roles_invalid_envelopes_and_caps() {
    let base = serde_json::to_value(fixture("success")).unwrap();
    for (path, value) in [
        ("/scheduler/flash_arbitrage/limits/hops", json!(0)),
        ("/scheduler/flash_arbitrage/limits/hops", json!(4)),
        (
            "/scheduler/flash_arbitrage/limits/trials_per_policy",
            json!(65),
        ),
        (
            "/scheduler/flash_arbitrage/limits/trials_per_round",
            json!(65),
        ),
        (
            "/scheduler/flash_arbitrage/limits/venue_checks",
            json!(4097),
        ),
        (
            "/scheduler/flash_arbitrage/policies/trader/account",
            json!("bob"),
        ),
        (
            "/scheduler/flash_arbitrage/policies/trader/asset",
            json!("X"),
        ),
        (
            "/scheduler/flash_arbitrage/policies/trader/principal",
            json!("0"),
        ),
        (
            "/scheduler/flash_arbitrage/policies/trader/minimum_net_profit",
            json!("0"),
        ),
        (
            "/scheduler/flash_arbitrage/policies/trader/lifetime",
            json!("0"),
        ),
        (
            "/scheduler/flash_arbitrage/policies/trader/resource_limit",
            json!("99"),
        ),
        (
            "/scheduler/flash_arbitrage/policies/trader/resource_limit",
            json!("10001"),
        ),
        (
            "/scheduler/flash_arbitrage/policies/trader/provider",
            json!("missing"),
        ),
        (
            "/scheduler/flash_arbitrage/policies/trader/resource_price",
            json!(Amount::MAX.to_string()),
        ),
    ] {
        let mut raw = base.clone();
        *raw.pointer_mut(path).unwrap() = value;
        assert!(
            Scenario::parse(&serde_json::to_vec(&raw).unwrap()).is_err(),
            "{path}"
        );
    }
    let mut s = fixture("success");
    s.model_versions.retain(|m| m != f::MODEL);
    assert!(s.validate().is_err());
    let mut s = fixture("success");
    s.scheduler.as_mut().unwrap().flash_arbitrage = None;
    assert!(s.validate().is_err());
    let mut s = fixture("success");
    s.scheduler = None;
    assert!(s.validate().is_err());
    let mut s = fixture("success");
    let random = Scenario::parse(include_bytes!(
        "../../../scenarios/demos/sc-agent-random.json"
    ))
    .unwrap();
    s.model_versions.push(policy::agents::MODEL.into());
    s.baseline_policies = random.baseline_policies;
    assert!(s.validate().unwrap_err().contains("duplicate"));
}

#[test]
fn overflow_and_nonce_time_failures_skip_before_any_probe() {
    for reason in [
        "FlashRepaymentOverflow",
        "NonceExhausted",
        "DeadlineOverflow",
        "DeliveryTimeOverflow",
    ] {
        let mut s = fixture("success");
        if reason == "FlashRepaymentOverflow" {
            policy(&mut s).principal = Amount::MAX;
        }
        if reason == "NonceExhausted" {
            s.accounts.get_mut("alice").unwrap().nonce = Amount::MAX;
        }
        let mut r = Run::new(s).unwrap();
        if reason == "DeadlineOverflow" {
            r.state.timestamp = 1.into();
            policy(&mut r.scenario).lifetime = Amount::MAX;
        }
        if reason == "DeliveryTimeOverflow" {
            r.state.timestamp = 1.into();
            r.scenario
                .scheduler
                .as_mut()
                .unwrap()
                .agents
                .get_mut("trader")
                .unwrap()
                .outgoing
                .builder_delay = Amount::MAX;
        }
        r.step().unwrap();
        assert_eq!(decision(&r, 0).reason, reason);
        assert_eq!(report(&r, 0).metrics.trials, 0);
        assert!(r.blocks[0].receipts.is_empty());
    }
}

#[test]
fn search_caps_keep_best_completed_candidate_and_share_round_budget_in_agent_order() {
    let mut s = fixture("success");
    cfg(&mut s).limits.trials_per_policy = 1;
    let r = first(s);
    assert!(decision(&r, 0).truncated);
    assert_eq!(report(&r, 0).metrics.trials, 1);
    assert_eq!(r.state.balance("alice", "Q"), 25104.into());
    let mut s = fixture("success");
    cfg(&mut s).limits.venue_checks = 1;
    let r = first(s);
    assert_eq!(decision(&r, 0).reason, "SearchLimit");
    assert_eq!(report(&r, 0).metrics.trials, 0);
    let mut s = fixture("success");
    cfg(&mut s).limits.hops = 1;
    assert_eq!(decision(&first(s), 0).reason, "NoRoute");
    let mut s = fixture("success");
    let mut second = policy(&mut s).clone();
    second.account = "bob".into();
    cfg(&mut s).policies.insert("recipient".into(), second);
    cfg(&mut s).limits.trials_per_round = 1;
    s.accounts
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("Q".into(), 1000.into());
    let r = first(s);
    let ds = &report(&r, 0).decisions;
    assert_eq!(
        ds.iter().map(|d| d.agent.as_str()).collect::<Vec<_>>(),
        vec!["recipient", "trader"]
    );
    assert_eq!(ds[0].action, DecisionAction::Submit);
    assert_eq!(ds[1].reason, "SearchLimit");
    assert_eq!(report(&r, 0).metrics.trials, 1);
}

#[test]
fn unproductive_three_hop_prefixes_stop_at_4096_venue_checks() {
    let mut s = fixture("success");
    s.protocols.clear();
    s.accounts.remove("pool");
    s.accounts.remove("pool-b");
    s.assets.insert("R".into(), Asset { decimals: 0 });
    for i in 0..65 {
        let id = format!("venue-{i:02}");
        let (x, y) = if i == 0 { ("Q", "X") } else { ("X", "R") };
        s.accounts.insert(
            id.clone(),
            Account {
                nonce: 0.into(),
                balances: [(x.into(), 1000.into()), (y.into(), 1000.into())].into(),
            },
        );
        s.protocols.insert(
            id.clone(),
            Pool {
                account: id,
                x: x.into(),
                y: y.into(),
                fee_bps: 30,
                owner: "alice".into(),
            },
        );
    }
    let r = first(s);
    assert_eq!(report(&r, 0).metrics.venue_checks, 4096);
    assert_eq!(report(&r, 0).metrics.trials, 0);
    assert_eq!(decision(&r, 0).reason, "SearchLimit");
    assert_eq!(r.state.balance("alice", "Q"), 1000.into());
}

#[test]
fn immediate_submission_refusal_is_distinct_from_later_expiry() {
    let mut s = fixture("delayed");
    policy(&mut s).lifetime = 1.into();
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(decision(&r, 0).submission, Some(SubmissionOutcome::Queued));
    assert!(r.blocks.iter().all(|b| b.receipts.is_empty()));
    let mut s = fixture("guard");
    s.scheduler.as_mut().unwrap().limits.queue = 1;
    let r = first(s);
    let d = decision(&r, 0);
    assert_eq!(
        d.submission,
        Some(SubmissionOutcome::Rejected {
            reason: SubmissionRejection::QueueLimit
        })
    );
    assert_eq!(d.action, DecisionAction::Skip);
    assert!(d.transaction.is_some());
    assert!(d.selected.is_some());
    assert_eq!(r.state.accounts["alice"].nonce, 0.into());
    assert_eq!(r.state.balance("alice", "Q"), 1000.into());
}

fn add_pool(s: &mut Scenario, id: &str, x: &str, y: &str, rx: u64, ry: u64) {
    s.accounts.insert(
        id.into(),
        Account {
            nonce: 0.into(),
            balances: [(x.into(), rx.into()), (y.into(), ry.into())].into(),
        },
    );
    s.protocols.insert(
        id.into(),
        Pool {
            account: id.into(),
            x: x.into(),
            y: y.into(),
            fee_bps: 30,
            owner: "alice".into(),
        },
    );
}
#[test]
fn three_hop_cycle_uses_actual_outputs_and_210_units() {
    let mut s = fixture("success");
    s.protocols.remove("second");
    s.accounts.remove("pool-b");
    s.assets.insert("R".into(), Asset { decimals: 0 });
    add_pool(&mut s, "middle", "X", "R", 1000000, 1000000);
    add_pool(&mut s, "last", "R", "Q", 1000000, 1500000);
    let r = first(s);
    let d = decision(&r, 0);
    let c = &d.candidates[d.selected.unwrap()];
    assert_eq!(c.hops.len(), 3);
    assert_eq!(c.net, Some(14247.into()));
    assert_eq!(r.state.balance("alice", "Q"), 15247.into());
    assert_eq!(r.state.balance("alice", "X"), 0.into());
    assert_eq!(r.state.balance("alice", "R"), 0.into());
    assert_eq!(r.blocks[0].receipts[0].used, 210.into());
}
fn counter_hook(s: &mut Scenario, payout: bool) {
    s.model_versions.push(hooks::MODEL.into());
    s.accounts.insert(
        "hook".into(),
        Account {
            nonce: 0.into(),
            balances: [("Q".into(), 100.into())].into(),
        },
    );
    let mut config = json!({"pool":"cpmm","account":"hook","capabilities":{"swap_fee":false,"reject":false,"write_local":true,"spend_own":{}},"programs":{"BeforeSwap":[{"type":"IncrementCounter"}]}});
    if payout {
        config["capabilities"]["swap_fee"] = json!(true);
        config["capabilities"]["spend_own"] = json!({"Q":"3"});
        config["programs"]["BeforeSwap"].as_array_mut().unwrap().push(json!({"type":"FeeByInput","asset_in":"Q","threshold":"1","below_bps":300,"at_or_above_bps":300}));
        config["programs"]["AfterSwap"] = json!([{"type":"PaySender","asset":"Q","amount":"3"}]);
    }
    s.hooks
        .insert("guard".into(), serde_json::from_value(config).unwrap());
}
#[test]
fn equal_profit_prefers_lower_resource_then_lexicographic_path() {
    let mut s = fixture("success");
    policy(&mut s).resource_price = 0.into();
    add_pool(&mut s, "z-source", "X", "Q", 1000000, 1000000);
    let r = first(s.clone());
    let d = decision(&r, 0);
    assert_eq!(d.candidates[d.selected.unwrap()].hops[0].venue(), "cpmm");
    counter_hook(&mut s, false);
    let r = first(s);
    let d = decision(&r, 0);
    let c = &d.candidates[d.selected.unwrap()];
    assert_eq!(c.hops[0].venue(), "z-source");
    assert_eq!(c.net, Some(24294.into()));
    assert_eq!(c.receipt.as_ref().unwrap().used, 190.into());
    assert_eq!(r.state.hooks["guard"].counter, 0.into()); // Hooked losing trials never commit.
}
#[test]
fn dynamic_hook_fee_payout_and_counter_are_isolated_per_probe_and_rolled_back_on_actual_loss() {
    let mut s = fixture("success");
    counter_hook(&mut s, true);
    let r = first(s);
    let d = decision(&r, 0);
    let c = &d.candidates[d.selected.unwrap()];
    assert_eq!(r.state.hooks["guard"].counter, 1.into());
    assert_eq!(r.state.balance("hook", "Q"), 97.into());
    assert_eq!(c.receipt.as_ref().unwrap().used, 245.into());
    // Q payout3 stays in the wallet: second hop consumes only the swap's X88422.
    assert_eq!(r.state.balance("pool-b", "X"), 1088422.into());
    assert_eq!(
        c.net,
        Some(
            r.state
                .balance("alice", "Q")
                .checked_sub(1000.into())
                .unwrap()
        )
    );
    let mut s = fixture("guard");
    counter_hook(&mut s, true);
    let r = first(s);
    assert_eq!(
        r.blocks[0].receipts.last().unwrap().status,
        "ProtocolRevert"
    );
    assert_eq!(r.state.hooks["guard"].counter, 0.into());
    assert_eq!(r.state.balance("hook", "Q"), 100.into());
    assert_eq!(r.state.balance("loan", "Q"), 10000000.into());
}
#[test]
fn sixty_four_trial_cap_bounds_dense_routes_and_retains_best_completed() {
    let mut s = fixture("success");
    for i in 0..10 {
        add_pool(&mut s, &format!("extra-{i:02}"), "X", "Q", 1000000, 1000000);
    }
    let r = first(s);
    assert_eq!(report(&r, 0).metrics.trials, 64);
    assert!(decision(&r, 0).truncated);
    assert_eq!(decision(&r, 0).candidates.len(), 64);
    assert!(report(&r, 0).metrics.venue_checks <= 4096);
    assert_eq!(r.state.balance("alice", "Q"), 25104.into());
}
fn emission(s: &mut Scenario, call: Call) {
    let cfg = s.scheduler.as_mut().unwrap();
    cfg.rounds = 2;
    cfg.emissions.push(scheduler::Emission {
        at: 0.into(),
        producer_id: "maker".into(),
        producer_sequence: 0.into(),
        delivery: scheduler::Delivery {
            channel: Channel::Public,
            builder_delay: 0.into(),
        },
        transaction: Transaction {
            id: "maker".into(),
            agent: "recipient".into(),
            sender: "bob".into(),
            chain: s.chain.id.clone(),
            nonce: 0.into(),
            limit: 2000.into(),
            price: 0.into(),
            deadline: 20.into(),
            program: vec![call],
        },
    });
}
#[test]
fn book_direction_limits_and_require_full_exclude_partial_inventory() {
    for lot in [1u64, 1000] {
        let mut s = fixture("success");
        s.protocols.remove("second");
        s.accounts.remove("pool-b");
        s.model_versions.push(book::MODEL.into());
        s.accounts.insert(
            "escrow".into(),
            Account {
                nonce: 0.into(),
                balances: Default::default(),
            },
        );
        s.accounts
            .get_mut("bob")
            .unwrap()
            .balances
            .insert("Q".into(), 3000000.into());
        s.books.insert(
            "book".into(),
            book::Config {
                account: "escrow".into(),
                base_asset: "X".into(),
                quote_asset: "Q".into(),
                lot_atoms: lot.into(),
                quote_tick_atoms: 1.into(),
                taker_fee_bps: 0,
                fee_account: "builder".into(),
            },
        );
        policy(&mut s).resource_limit = 1000.into();
        policy(&mut s).route_limits = vec![routing::heterogeneous::DirectionalLimit {
            asset_in: "X".into(),
            hop: routing::heterogeneous::VenueHop::Book {
                venue: "book".into(),
                limit_price_ticks: (2 * lot).into(),
            },
        }];
        emission(
            &mut s,
            Call::PlaceOrder {
                book: "book".into(),
                side: book::Side::Buy,
                price_ticks: (2 * lot).into(),
                lots: (1000000 / lot).into(),
                tif: book::TimeInForce::GTC,
            },
        );
        let mut r = Run::new(s.clone()).unwrap();
        r.finish().unwrap();
        let d = decision(&r, 1);
        if lot == 1 {
            assert_eq!(d.action, DecisionAction::Submit);
            assert!(r.state.balance("alice", "Q") > 80000.into());
        } else {
            assert_eq!(d.reason, "NoProfitableCycle");
            assert_eq!(r.state.balance("alice", "Q"), 1000.into());
            assert_eq!(r.state.balance("alice", "X"), 0.into());
        }
        policy(&mut s).route_limits.clear();
        let mut r = Run::new(s).unwrap();
        r.finish().unwrap();
        assert_eq!(decision(&r, 1).reason, "NoRoute");
    }
}
#[test]
fn clmm_directional_price_limits_and_stable_solver_use_same_flash_executor() {
    let mut s = fixture("success");
    s.protocols.remove("second");
    s.accounts.remove("pool-b");
    s.accounts
        .get_mut("pool")
        .unwrap()
        .balances
        .insert("X".into(), 2000000.into());
    s.model_versions.push(clmm::MODEL.into());
    s.accounts.insert(
        "clmm-cash".into(),
        Account {
            nonce: 0.into(),
            balances: Default::default(),
        },
    );
    s.accounts.get_mut("bob").unwrap().balances =
        [("Q".into(), 100000.into()), ("X".into(), 100000.into())].into();
    s.clmm_pools.insert(
        "clmm".into(),
        clmm::Config {
            account: "clmm-cash".into(),
            x: "X".into(),
            q: "Q".into(),
            fee_bps: 30,
            initial_sqrt: "79228162514264337593543950336".parse().unwrap(),
        },
    );
    policy(&mut s).principal = 1000.into();
    policy(&mut s).resource_limit = 1000.into();
    policy(&mut s).route_limits = vec![routing::heterogeneous::DirectionalLimit {
        asset_in: "X".into(),
        hop: routing::heterogeneous::VenueHop::Clmm {
            venue: "clmm".into(),
            limit_tick: -100,
        },
    }];
    emission(
        &mut s,
        Call::MintPosition {
            pool: "clmm".into(),
            lower: -100,
            upper: 100,
            liquidity: 1000000.into(),
            max_x: 100000.into(),
            max_q: 100000.into(),
        },
    );
    let mut r = Run::new(s.clone()).unwrap();
    r.finish().unwrap();
    assert_eq!(decision(&r, 1).action, DecisionAction::Submit);
    assert_eq!(r.state.balance("alice", "X"), 0.into());
    policy(&mut s).route_limits[0].hop = routing::heterogeneous::VenueHop::Clmm {
        venue: "clmm".into(),
        limit_tick: -1,
    };
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(decision(&r, 1).reason, "NoProfitableCycle");
    assert_eq!(r.state.balance("alice", "Q"), 1000.into());
    let mut s = fixture("success");
    let pool = s.protocols.remove("cpmm").unwrap();
    s.model_versions.push(stable::MODEL.into());
    s.stable_pools.insert(
        "cpmm".into(),
        stable::StablePool {
            pool,
            amplification: 100,
            iteration_limit: 255,
        },
    );
    policy(&mut s).resource_limit = 1000.into();
    let r = first(s);
    let d = decision(&r, 0);
    assert_eq!(d.action, DecisionAction::Submit);
    assert!(
        d.candidates[d.selected.unwrap()]
            .receipt
            .as_ref()
            .unwrap()
            .used
            > 190.into()
    );
    assert_eq!(r.state.balance("alice", "X"), 0.into());
}

#[test]
fn known_limit_refusal_keeps_successful_trial_without_retry_or_canonical_fee() {
    let mut s = fixture("budget");
    s.scheduler.as_mut().unwrap().limits.known = 1;
    s.accounts
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("Q".into(), 1.into());
    emission(
        &mut s,
        Call::Transfer {
            to: "alice".into(),
            asset: "Q".into(),
            amount: 1.into(),
        },
    );
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(decision(&r, 0).reason, "OwnFeeBudget");
    let d = decision(&r, 1);
    assert_eq!(d.reason, "Submission: KnownTransactionsLimit");
    assert_eq!(d.action, DecisionAction::Skip);
    assert_eq!(d.candidates[d.selected.unwrap()].outcome, "Success");
    assert!(d.transaction.is_some());
    assert!(r.blocks[1].receipts.is_empty());
    assert_eq!(r.state.balance("alice", "Q"), 300.into());
    assert_eq!(r.state.accounts["alice"].nonce, 0.into());
    assert_eq!(r, Run::restore(&r.checkpoint().unwrap()).unwrap());
}

#[test]
fn foreign_private_future_data_does_not_change_search_and_rehashed_report_is_rejected() {
    let mut decisions = vec![];
    for amount in [1u64, 2] {
        let mut s = fixture("success");
        s.scheduler.as_mut().unwrap().rounds = 2;
        s.accounts
            .get_mut("bob")
            .unwrap()
            .balances
            .insert("Q".into(), 100.into());
        s.accounts.insert(
            "hidden".into(),
            Account {
                nonce: 0.into(),
                balances: Default::default(),
            },
        );
        for (id, at, nonce) in [("private", 0u64, 0u64), ("future", 2, 1)] {
            s.scheduler
                .as_mut()
                .unwrap()
                .emissions
                .push(scheduler::Emission {
                    at: at.into(),
                    producer_id: id.into(),
                    producer_sequence: 0.into(),
                    delivery: scheduler::Delivery {
                        channel: Channel::Private {
                            recipients: Default::default(),
                        },
                        builder_delay: 0.into(),
                    },
                    transaction: Transaction {
                        id: id.into(),
                        agent: "recipient".into(),
                        sender: "bob".into(),
                        chain: s.chain.id.clone(),
                        nonce: nonce.into(),
                        limit: 200.into(),
                        price: 0.into(),
                        deadline: 10.into(),
                        program: vec![Call::Transfer {
                            to: "hidden".into(),
                            asset: "Q".into(),
                            amount: amount.into(),
                        }],
                    },
                });
        }
        let r = first(s);
        decisions.push(decision(&r, 0).clone());
        let mut corrupt: Value = serde_json::from_slice(&r.checkpoint().unwrap()).unwrap();
        corrupt["run"]["blocks"][0]["scheduler"]["flash_arbitrage"]["metrics"]["trials"] = json!(0);
        let changed: Run = serde_json::from_value(corrupt["run"].clone()).unwrap();
        corrupt["hash"] = json!(run::digest(&serde_json::to_vec(&changed).unwrap()));
        assert!(
            Run::restore(&serde_json::to_vec(&corrupt).unwrap())
                .unwrap_err()
                .contains("replay mismatch")
        );
    }
    assert_eq!(decisions[0], decisions[1]);
}

#[test]
fn observation_delay_overflow_only_applies_to_visible_recipients() {
    for public in [true, false] {
        let mut s = fixture("success");
        let network = s.scheduler.as_mut().unwrap();
        network
            .agents
            .get_mut("recipient")
            .unwrap()
            .observation_delay = Amount::MAX;
        if !public {
            network.agents.get_mut("trader").unwrap().outgoing.channel = Channel::Private {
                recipients: Default::default(),
            };
        }
        let mut r = Run::new(s).unwrap();
        r.state.timestamp = 1.into();
        r.step().unwrap();
        assert_eq!(
            decision(&r, 0).reason,
            if public {
                "DeliveryTimeOverflow"
            } else {
                "ProfitableFlashCycle"
            }
        );
        assert_eq!(r.blocks[0].receipts.len(), usize::from(!public));
    }
}
