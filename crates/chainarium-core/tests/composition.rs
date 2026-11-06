use chainarium_core::{
    engine,
    policy::ArbitragePolicy,
    routing::{Observation, search},
    *,
};
use std::collections::BTreeMap;
fn scenario() -> Scenario {
    let mut s = Scenario::parse(include_bytes!("../../../scenarios/demos/sc-01.json")).unwrap();
    s.model_versions.extend(M2_MODELS.map(String::from));
    s.blocks.clear();
    s
}
fn tx(program: Vec<Call>) -> Transaction {
    Transaction {
        id: "composition".into(),
        agent: "trader".into(),
        sender: "alice".into(),
        chain: "lab-1".into(),
        nonce: Amount::ZERO,
        limit: 1000.into(),
        price: Amount::ZERO,
        deadline: Amount::MAX,
        program,
    }
}
fn run(mut s: Scenario, program: Vec<Call>) -> Run {
    s.blocks = vec![vec![tx(program)]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    r
}
fn fail() -> Call {
    Call::AssertCondition {
        condition: false,
        message: "late failure".into(),
    }
}
fn receipt(r: &Run) -> &Receipt {
    &r.blocks[0].receipts[0]
}
fn second_pool(s: &mut Scenario) {
    s.accounts.insert(
        "pool-b".into(),
        Account {
            nonce: Amount::ZERO,
            balances: BTreeMap::from([("X".into(), 1000.into()), ("Q".into(), 1500.into())]),
        },
    );
    let mut p = s.protocols["cpmm"].clone();
    p.account = "pool-b".into();
    s.protocols.insert("second".into(), p);
}
fn flash_provider(s: &mut Scenario, id: &str, fee: u16) {
    s.accounts.insert(
        id.into(),
        Account {
            nonce: Amount::ZERO,
            balances: BTreeMap::from([("Q".into(), 10000.into())]),
        },
    );
    s.flash_providers.insert(
        id.into(),
        FlashProvider {
            account: id.into(),
            asset: "Q".into(),
            fee_bps: fee,
        },
    );
}
fn loan(provider: &str, callback: Vec<Call>) -> Call {
    Call::FlashLoan {
        provider: provider.into(),
        receiver: "alice".into(),
        amount: 100.into(),
        callback,
    }
}
#[test]
fn vf_lp_mint_caps_and_burn_exact() {
    let r = run(
        scenario(),
        vec![
            Call::AddLiquidity {
                pool: "cpmm".into(),
                max_x: 100.into(),
                max_y: 120.into(),
                min_shares: 100.into(),
            },
            Call::RemoveLiquidity {
                pool: "cpmm".into(),
                shares: 25.into(),
                min_x: 25.into(),
                min_y: 25.into(),
            },
        ],
    );
    assert_eq!(receipt(&r).status, "Success");
    assert_eq!(r.state.shares["cpmm"], 1075.into());
    assert_eq!(r.state.balance("pool", "X"), 1075.into());
    assert_eq!(r.state.balance("pool", "Q"), 1075.into());
    assert_eq!(r.state.balance("alice", "Q"), 9925.into());
    assert_eq!(r.state.lp_balances["cpmm"]["alice"], 1075.into());
    r.state.invariant(&r.scenario).unwrap();
}
#[test]
fn vf_lp_last_exit_empty_quote_and_bootstrap() {
    let mut s = scenario();
    s.blocks = vec![vec![tx(vec![Call::RemoveLiquidity {
        pool: "cpmm".into(),
        shares: 1000.into(),
        min_x: 1000.into(),
        min_y: 1000.into(),
    }])]];
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(r.state.balance("pool", "X"), Amount::ZERO);
    assert_eq!(r.state.shares["cpmm"], Amount::ZERO);
    assert!(engine::quote(&r.scenario, &r.state, "cpmm", "X", 1.into()).is_err());
    let mut reinit = tx(vec![Call::AddLiquidity {
        pool: "cpmm".into(),
        max_x: 25.into(),
        max_y: 100.into(),
        min_shares: 50.into(),
    }]);
    reinit.nonce = 1.into();
    let b = engine::build_block(&r.scenario, &r.state, &[reinit]).unwrap();
    assert_eq!(b.receipts[0].status, "Success");
    assert_eq!(b.state.shares["cpmm"], 50.into());
    assert_eq!(b.state.balance("pool", "X"), 25.into());
}
#[test]
fn vf_lp_zero_mint_zero_burn_output_and_owner() {
    let mut s = scenario();
    s.accounts
        .get_mut("pool")
        .unwrap()
        .balances
        .insert("X".into(), 1000000.into());
    s.accounts
        .get_mut("pool")
        .unwrap()
        .balances
        .insert("Q".into(), 1.into());
    let r = run(
        s.clone(),
        vec![Call::AddLiquidity {
            pool: "cpmm".into(),
            max_x: 1.into(),
            max_y: 1.into(),
            min_shares: Amount::ZERO,
        }],
    );
    assert_eq!(receipt(&r).status, "ProtocolRevert");
    assert_eq!(r.state.shares["cpmm"], 1000.into());
    let r = run(
        s,
        vec![Call::RemoveLiquidity {
            pool: "cpmm".into(),
            shares: 1.into(),
            min_x: Amount::ZERO,
            min_y: Amount::ZERO,
        }],
    );
    assert_eq!(receipt(&r).status, "ProtocolRevert");
    let s = scenario();
    let w = World::genesis(&s).unwrap();
    let mut steal = tx(vec![Call::RemoveLiquidity {
        pool: "cpmm".into(),
        shares: 1.into(),
        min_x: Amount::ZERO,
        min_y: Amount::ZERO,
    }]);
    steal.agent = "recipient".into();
    steal.sender = "bob".into();
    let b = engine::build_block(&s, &w, &[steal]).unwrap();
    assert_eq!(b.receipts[0].status, "ProtocolRevert");
    assert_eq!(b.state.lp_balances, w.lp_balances);
}
#[test]
fn vf_lp_extended_ratio_does_not_overflow_before_min() {
    let mut s = scenario();
    let q = Amount::from_big(Amount::MAX.big() / 4u8).unwrap();
    s.accounts.get_mut("pool").unwrap().balances =
        BTreeMap::from([("X".into(), 1.into()), ("Q".into(), q)]);
    s.accounts.get_mut("alice").unwrap().balances =
        BTreeMap::from([("X".into(), q), ("Q".into(), q)]);
    let r = run(
        s,
        vec![Call::AddLiquidity {
            pool: "cpmm".into(),
            max_x: q,
            max_y: q,
            min_shares: 1.into(),
        }],
    );
    assert_eq!(receipt(&r).status, "Success");
    assert_eq!(r.state.balance("pool", "X"), 2.into());
}
#[test]
fn vf_lp_late_failure_restores_shares_and_all_reserves() {
    let r = run(
        scenario(),
        vec![
            Call::AddLiquidity {
                pool: "cpmm".into(),
                max_x: 100.into(),
                max_y: 120.into(),
                min_shares: 100.into(),
            },
            fail(),
        ],
    );
    assert_eq!(receipt(&r).status, "ProtocolRevert");
    assert_eq!(r.state.shares, r.initial.shares);
    assert_eq!(r.state.lp_balances, r.initial.lp_balances);
    for a in ["alice", "pool"] {
        assert_eq!(r.state.accounts[a].balances, r.initial.accounts[a].balances);
    }
}
#[test]
fn vf_route_actual_outputs_do_not_spend_preexisting_inventory() {
    let mut s = scenario();
    second_pool(&mut s);
    let obs = Observation::capture(&s, &World::genesis(&s).unwrap(), "alice").unwrap();
    let route = search(&obs, "Q", "Q", 100.into()).routes.remove(0);
    assert_eq!(route.output, 123.into());
    assert_eq!(route.hops[0].output, 90.into());
    let r = run(s, route.calls(100.into(), 123.into(), ""));
    assert_eq!(receipt(&r).status, "Success");
    assert_eq!(r.state.balance("alice", "X"), 1000.into());
    assert_eq!(r.state.balance("alice", "Q"), 10023.into());
    assert_eq!(r.state.balance("pool", "X"), 910.into());
    assert_eq!(r.state.balance("pool-b", "X"), 1090.into());
}
#[test]
fn vf_route_references_and_multi_pool_late_rollback() {
    let mut s = scenario();
    second_pool(&mut s);
    let w = World::genesis(&s).unwrap();
    let obs = Observation::capture(&s, &w, "alice").unwrap();
    let route = search(&obs, "Q", "Q", 100.into()).routes.remove(0);
    let mut calls = route.calls(100.into(), 1.into(), "");
    calls.push(fail());
    let r = run(s.clone(), calls);
    assert_eq!(receipt(&r).status, "ProtocolRevert");
    for a in ["pool", "pool-b"] {
        assert_eq!(r.state.accounts[a], r.initial.accounts[a]);
    }
    let mut bad = route.calls(100.into(), 1.into(), "");
    if let Call::SwapOutput { output_of, .. } = &mut bad[1] {
        *output_of = "99".into();
    }
    assert!(
        s.validate_tx(&tx(bad))
            .unwrap_err()
            .contains("output reference")
    );
}
#[test]
fn vf_route_search_budget_determinism_and_observation_scope() {
    let mut s = scenario();
    let p = s.protocols["cpmm"].clone();
    s.protocols.clear();
    for i in 0..65 {
        let id = format!("venue-{i:02}");
        let account = format!("vault-{i:02}");
        s.accounts
            .insert(account.clone(), s.accounts["pool"].clone());
        let mut pool = p.clone();
        pool.account = account;
        s.protocols.insert(id, pool);
    }
    let w = World::genesis(&s).unwrap();
    let mut obs = Observation::capture(&s, &w, "alice").unwrap();
    let a = search(&obs, "X", "Q", 100.into());
    assert_eq!(a.routes.len(), 64);
    assert!(a.truncated);
    assert_eq!(a.routes[0].hops[0].venue, "venue-00");
    obs.venues.reverse();
    assert_eq!(search(&obs, "X", "Q", 100.into()), a);
    let view = serde_json::to_value(obs).unwrap();
    assert!(view.get("accounts").is_none());
    assert!(view.get("future").is_none());
}
#[test]
fn vf_flash_success_rounds_fee_up_and_has_provider_callback() {
    let mut s = scenario();
    flash_provider(&mut s, "flash", 5);
    let r = run(s, vec![loan("flash", vec![])]);
    assert_eq!(receipt(&r).status, "Success");
    assert_eq!(receipt(&r).used, 130.into());
    assert_eq!(r.state.balance("alice", "Q"), 9999.into());
    assert_eq!(r.state.balance("flash", "Q"), 10001.into());
    let callback = &receipt(&r).trace[1];
    assert_eq!(
        (&*callback.operation, &*callback.caller, &*callback.callee),
        ("FlashCallback", "flash", "alice")
    );
}
#[test]
fn vf_flash_shortfall_and_late_failure_restore_all_transfers() {
    let mut s = scenario();
    flash_provider(&mut s, "flash", 5);
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), Amount::ZERO);
    let r = run(s.clone(), vec![loan("flash", vec![])]);
    assert_eq!(receipt(&r).status, "ProtocolRevert");
    assert_eq!(r.state.balance("flash", "Q"), 10000.into());
    assert_eq!(r.state.balance("alice", "Q"), Amount::ZERO);
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), 1.into());
    let r = run(s, vec![loan("flash", vec![]), fail()]);
    assert_eq!(receipt(&r).status, "ProtocolRevert");
    assert_eq!(r.state.balance("flash", "Q"), 10000.into());
    assert_eq!(r.state.balance("alice", "Q"), 1.into());
    assert!(receipt(&r).events.is_empty());
}
#[test]
fn vf_flash_nested_providers_reentry_and_receiver_authority() {
    let mut s = scenario();
    flash_provider(&mut s, "flash", 5);
    flash_provider(&mut s, "other", 5);
    let r = run(s.clone(), vec![loan("flash", vec![loan("other", vec![])])]);
    assert_eq!(receipt(&r).status, "Success");
    assert_eq!(r.state.balance("alice", "Q"), 9998.into());
    assert_eq!(r.state.balance("flash", "Q"), 10001.into());
    assert_eq!(r.state.balance("other", "Q"), 10001.into());
    let r = run(s.clone(), vec![loan("flash", vec![loan("flash", vec![])])]);
    assert_eq!(receipt(&r).error.as_deref(), Some("flash provider reentry"));
    assert_eq!(r.state.balance("flash", "Q"), 10000.into());
    let r = run(
        s,
        vec![Call::FlashLoan {
            provider: "flash".into(),
            receiver: "bob".into(),
            amount: 100.into(),
            callback: vec![],
        }],
    );
    assert_eq!(
        receipt(&r).error.as_deref(),
        Some("receiver must be sender")
    );
}
#[test]
fn vf_flash_funded_cycle_uses_real_liquidity_and_closes_scope() {
    let mut s = scenario();
    second_pool(&mut s);
    flash_provider(&mut s, "flash", 5);
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), Amount::ZERO);
    let obs = Observation::capture(&s, &World::genesis(&s).unwrap(), "alice").unwrap();
    let route = search(&obs, "Q", "Q", 100.into()).routes.remove(0);
    let r = run(
        s.clone(),
        vec![
            loan("flash", route.calls(100.into(), 1.into(), "0.callback")),
            Call::AssertBalanceDelta {
                account: "alice".into(),
                asset: "Q".into(),
                baseline: Amount::ZERO,
                minimum: 22.into(),
            },
        ],
    );
    assert_eq!(receipt(&r).status, "Success");
    assert_eq!(r.state.balance("alice", "Q"), 22.into());
    assert_eq!(r.state.balance("alice", "X"), 1000.into());
    assert_eq!(r.state.balance("flash", "Q"), 10001.into());
    assert_eq!(receipt(&r).used, 180.into());
    let r = run(
        s,
        vec![loan(
            "flash",
            route.calls(100.into(), 124.into(), "0.callback"),
        )],
    );
    assert_eq!(receipt(&r).status, "ProtocolRevert");
    for a in ["flash", "pool", "pool-b"] {
        assert_eq!(r.state.accounts[a], r.initial.accounts[a]);
    }
}
#[test]
fn vf_policy_simulation_is_readonly_and_checkpoint_replays_decisions() {
    let mut s = scenario();
    second_pool(&mut s);
    s.blocks = vec![vec![], vec![]];
    s.policies.insert(
        "trader".into(),
        ArbitragePolicy {
            account: "alice".into(),
            asset: "Q".into(),
            input: 100.into(),
            min_profit: 1.into(),
            resource_limit: 500.into(),
            resource_price: Amount::ZERO,
        },
    );
    let mut r = Run::new(s).unwrap();
    let original = r.state.clone();
    let decisions = chainarium_core::policy::decisions(&r.scenario, &r.state).unwrap();
    assert_eq!(r.state, original);
    assert_eq!(decisions[0].expected_net_gain, Some(23.into()));
    r.step().unwrap();
    assert_eq!(r.blocks[0].receipts.len(), 1);
    assert_eq!(r.state.balance("alice", "Q"), 10023.into());
    let mut resumed = Run::restore(&r.checkpoint().unwrap()).unwrap();
    r.finish().unwrap();
    resumed.finish().unwrap();
    assert_eq!(r, resumed);
}
#[test]
fn vf_policy_accounts_for_execution_fees_and_absent_valuation() {
    let mut s = scenario();
    second_pool(&mut s);
    let p = ArbitragePolicy {
        account: "alice".into(),
        asset: "Q".into(),
        input: 100.into(),
        min_profit: 1.into(),
        resource_limit: 500.into(),
        resource_price: 1.into(),
    };
    s.policies.insert("trader".into(), p);
    let r = Run::new(s.clone()).unwrap();
    let d = chainarium_core::policy::decisions(&s, &r.state).unwrap();
    assert!(d[0].transaction.is_none());
    s.policies.get_mut("trader").unwrap().asset = "X".into();
    let d = chainarium_core::policy::decisions(&s, &r.state).unwrap();
    assert_eq!(d[0].reason, "fee valuation unavailable in input asset");
}

#[test]
fn vf_policy_positive_profit_after_nonzero_execution_fee() {
    let mut s = scenario();
    second_pool(&mut s);
    s.accounts
        .get_mut("pool-b")
        .unwrap()
        .balances
        .insert("Q".into(), 10000.into());
    s.policies.insert(
        "trader".into(),
        ArbitragePolicy {
            account: "alice".into(),
            asset: "Q".into(),
            input: 100.into(),
            min_profit: 1.into(),
            resource_limit: 1000.into(),
            resource_price: 1.into(),
        },
    );
    s.blocks = vec![vec![]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    // 100 Q -> 90 X -> 823 Q; 723 gross - 150 execution fee = 573.
    assert_eq!(r.blocks[0].decisions[0].expected_net_gain, Some(573.into()));
    assert_eq!(receipt(&r).fee, 150.into());
    assert_eq!(r.state.balance("alice", "Q"), 10573.into());
}
#[test]
fn vf_flash_zero_fee_exhaustion_and_callback_node_limits() {
    let mut s = scenario();
    flash_provider(&mut s, "flash", 0);
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), Amount::ZERO);
    let r = run(s.clone(), vec![loan("flash", vec![])]);
    assert_eq!(receipt(&r).status, "Success");
    assert_eq!(r.state.balance("alice", "Q"), Amount::ZERO);
    let mut t = tx(vec![loan("flash", vec![])]);
    t.limit = 120.into();
    s.blocks = vec![vec![t]];
    let mut r = Run::new(s.clone()).unwrap();
    r.finish().unwrap();
    assert_eq!(receipt(&r).status, "ResourceExhausted");
    assert_eq!(r.state.balance("flash", "Q"), 10000.into());
    let long = loan(
        "flash",
        vec![
            Call::AssertCondition {
                condition: true,
                message: String::new()
            };
            256
        ],
    );
    assert!(s.validate_tx(&tx(vec![long.clone(); 4])).is_err());
}
#[test]
fn vf_quote_stale_execution_and_lp_minimums_are_enforced() {
    let mut s = scenario();
    second_pool(&mut s);
    let w = World::genesis(&s).unwrap();
    let o = Observation::capture(&s, &w, "alice").unwrap();
    let route = search(&o, "Q", "Q", 100.into()).routes.remove(0);
    let first = tx(route.calls(100.into(), 123.into(), ""));
    let mut second = first.clone();
    second.id = "stale-route".into();
    second.nonce = 1.into();
    s.blocks = vec![vec![first, second]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.blocks[0].receipts[0].status, "Success");
    assert_eq!(r.blocks[0].receipts[1].status, "ProtocolRevert");
    assert_eq!(r.state.balance("alice", "Q"), 10023.into());
    let r = run(
        scenario(),
        vec![Call::AddLiquidity {
            pool: "cpmm".into(),
            max_x: 100.into(),
            max_y: 120.into(),
            min_shares: 101.into(),
        }],
    );
    assert_eq!(receipt(&r).status, "ProtocolRevert");
    let r = run(
        scenario(),
        vec![Call::RemoveLiquidity {
            pool: "cpmm".into(),
            shares: 25.into(),
            min_x: 26.into(),
            min_y: 25.into(),
        }],
    );
    assert_eq!(receipt(&r).status, "ProtocolRevert");
}

#[test]
fn vf_policy_fee_to_same_account_is_not_an_external_cost() {
    let mut s = scenario();
    second_pool(&mut s);
    s.chain.fee_account = "alice".into();
    s.policies.insert(
        "trader".into(),
        ArbitragePolicy {
            account: "alice".into(),
            asset: "Q".into(),
            input: 100.into(),
            min_profit: 1.into(),
            resource_limit: 500.into(),
            resource_price: 1.into(),
        },
    );
    s.blocks = vec![vec![]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(r.blocks[0].decisions[0].expected_net_gain, Some(23.into()));
    assert_eq!(receipt(&r).fee, 150.into());
    assert_eq!(r.state.balance("alice", "Q"), 10023.into());
}

#[test]
fn vf_flash_cannot_fund_its_own_envelope_or_spoof_callback() {
    let mut s = scenario();
    flash_provider(&mut s, "flash", 0);
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("Q".into(), Amount::ZERO);
    let mut t = tx(vec![loan("flash", vec![])]);
    t.price = 1.into();
    s.blocks = vec![vec![t]];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert!(r.blocks[0].receipts.is_empty());
    assert_eq!(r.state.accounts["alice"].nonce, Amount::ZERO);
    assert_eq!(r.state.balance("flash", "Q"), 10000.into());
    assert!(
        serde_json::from_str::<Call>(r#"{"type":"FlashCallback","caller":"flash","calls":[]}"#)
            .is_err()
    );
}
