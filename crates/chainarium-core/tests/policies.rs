use chainarium_core::{
    amount::Price,
    book::{Side, TimeInForce},
    engine,
    policy::{self, Decision, DecisionAction as Action, baselines::*},
    routing::{Observation, heterogeneous as h},
    *,
};
use std::{cell::Cell, sync::Arc};

fn lp() -> Scenario {
    Scenario::parse(include_bytes!(
        "../../../scenarios/demos/sc-lp-policies.json"
    ))
    .unwrap()
}
fn mm() -> Scenario {
    Scenario::parse(include_bytes!("../../../scenarios/demos/sc-mm-oracle.json")).unwrap()
}
fn decision<'a>(b: &'a Block, agent: &str) -> &'a Decision {
    b.decisions.iter().find(|d| d.agent == agent).unwrap()
}
fn tx(w: &World, who: &str, program: Vec<Call>) -> Transaction {
    Transaction {
        id: format!("scripted-{}-{who}", w.block),
        agent: who.into(),
        sender: who.into(),
        chain: "lab-1".into(),
        nonce: w.accounts[who].nonce,
        limit: 1000.into(),
        price: Amount::ZERO,
        deadline: Amount::MAX,
        program,
    }
}
fn order(side: Side, price: u64, lots: u64) -> Call {
    Call::PlaceOrder {
        book: "book".into(),
        side,
        price_ticks: price.into(),
        lots: lots.into(),
        tif: TimeInForce::GTC,
    }
}
fn mint() -> Call {
    Call::MintPosition {
        pool: "clmm".into(),
        lower: -100,
        upper: 100,
        liquidity: 1.into(),
        max_x: Amount::MAX,
        max_q: Amount::MAX,
    }
}
fn anchor(s: &mut Scenario, numerator: Amount, denominator: Amount) {
    s.feeds
        .get_mut("public-price")
        .unwrap()
        .initial
        .as_mut()
        .unwrap()
        .price = Price::new("X".into(), "Q".into(), numerator, denominator).unwrap();
}
fn isolated_mm() -> Scenario {
    let mut s = mm();
    s.policies.clear();
    s.blocks = vec![vec![]; 3];
    s
}

#[test]
fn passive_lp_holds_outside_range_and_scheduled_lp_repositions_then_exits() {
    let mut r = Run::new(lp()).unwrap();
    r.finish().unwrap();
    assert_eq!(r.blocks[0].state.clmm["clmm"].positions.len(), 2);
    for who in ["alice", "bob"] {
        assert_eq!(r.blocks[0].state.balance(who, "X"), 95012.into());
        assert_eq!(r.blocks[0].state.balance(who, "Q"), 95012.into());
    }
    assert_eq!(r.blocks[1].state.clmm["clmm"].liquidity, Amount::ZERO);
    let b = &r.blocks[2];
    assert_eq!(decision(b, "alice").action, Action::Hold);
    assert_eq!(decision(b, "bob").action, Action::Submit);
    assert_eq!(decision(b, "bob").observed_block, 2.into());
    assert_eq!(decision(b, "bob").candidates, 1.into());
    let calls = &decision(b, "bob").transaction.as_ref().unwrap().program;
    assert!(
        matches!(calls.as_slice(),[Call::CollectPosition {position_id,..},Call::BurnPosition {..},Call::MintPosition {lower:-200,upper:-50,..}] if *position_id==2.into())
    );
    assert_eq!(b.state.clmm["clmm"].liquidity, 500000.into());
    assert_eq!(b.state.clmm["clmm"].positions[&1.into()].owner, "alice");
    assert_eq!(b.state.clmm["clmm"].positions[&3.into()].owner, "bob");
    assert!(
        r.state.clmm["clmm"]
            .positions
            .values()
            .all(|p| p.owner == "alice")
    );
    assert_eq!(r.state.accounts["alice"].nonce, 1.into());
    assert_eq!(r.state.accounts["bob"].nonce, 3.into());
    assert!(
        r.blocks
            .iter()
            .flat_map(|b| &b.decisions)
            .all(|d| d.expected_net_gain.is_none())
    );
}

#[test]
fn scheduled_lp_failed_simulation_skips_without_fee_or_hidden_retry() {
    let mut s = lp();
    s.lp_policies.remove("bob");
    s.blocks = vec![vec![]; 3];
    if let LpTarget::Target { max_x, .. } =
        &mut s.lp_policies.get_mut("alice").unwrap().schedule[0].target
    {
        *max_x = Amount::ZERO;
    }
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(decision(&r.blocks[0], "alice").action, Action::Skip);
    assert!(
        decision(&r.blocks[0], "alice")
            .reason
            .contains("CLMM max deposit")
    );
    assert!(
        r.blocks[1..]
            .iter()
            .all(|b| decision(b, "alice").action == Action::Hold)
    );
    assert_eq!(r.state.accounts, r.initial.accounts);
    assert!(r.state.clmm["clmm"].positions.is_empty());
}

#[test]
fn scripted_nonce_conflict_rejects_lp_intent_and_event_is_not_retried() {
    let mut s = lp();
    s.lp_policies.remove("bob");
    s.blocks = vec![vec![]; 2];
    let w = World::genesis(&s).unwrap();
    s.blocks[0] = vec![tx(
        &w,
        "alice",
        vec![Call::Transfer {
            to: "bob".into(),
            asset: "X".into(),
            amount: 1.into(),
        }],
    )];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(decision(&r.blocks[0], "alice").action, Action::Submit);
    assert_eq!(r.blocks[0].rejected.len(), 1);
    assert!(r.blocks[0].rejected[0].reason.contains("nonce mismatch"));
    assert_eq!(decision(&r.blocks[1], "alice").action, Action::Hold);
    assert!(r.state.clmm["clmm"].positions.is_empty());
}

#[test]
fn lp_exit_collects_zero_liquidity_position_without_invalid_burn() {
    let mut s = lp();
    s.lp_policies.remove("bob");
    s.lp_policies
        .get_mut("alice")
        .unwrap()
        .schedule
        .push(LpEvent {
            block: 4.into(),
            target: LpTarget::Exit,
        });
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let swap = Call::ClmmSwap {
        pool: "clmm".into(),
        asset_in: "X".into(),
        amount: 1000.into(),
        sqrt_price_limit: chainarium_core::clmm::math::tick_sqrt(-100).unwrap(),
        min_out: 1.into(),
        allow_partial: false,
    };
    r.state = engine::build_block(&r.scenario, &r.state, &[tx(&r.state, "trader", vec![swap])])
        .unwrap()
        .state;
    r.state = engine::build_block(
        &r.scenario,
        &r.state,
        &[tx(
            &r.state,
            "alice",
            vec![Call::BurnPosition {
                pool: "clmm".into(),
                position_id: 1.into(),
                liquidity: 1000000.into(),
                min_x: Amount::ZERO,
                min_q: Amount::ZERO,
            }],
        )],
    )
    .unwrap()
    .state;
    assert_eq!(
        r.state.clmm["clmm"].positions[&1.into()].liquidity,
        Amount::ZERO
    );
    let d = policy::decisions(&r.scenario, &r.state).unwrap().remove(0);
    assert_eq!(d.action, Action::Submit);
    assert!(matches!(
        d.transaction.as_ref().unwrap().program.as_slice(),
        [Call::CollectPosition { .. }]
    ));
    let b = engine::build_block(&r.scenario, &r.state, &[d.transaction.unwrap()]).unwrap();
    assert!(b.state.clmm["clmm"].positions.is_empty());
}

#[test]
fn owned_item_limits_skip_before_any_simulation() {
    let mut s = lp();
    s.lp_policies.remove("bob");
    s.blocks = vec![vec![]; 2];
    s.lp_policies.get_mut("alice").unwrap().schedule[0].block = 2.into();
    let w = World::genesis(&s).unwrap();
    s.blocks[0] = vec![tx(&w, "alice", vec![mint(); 17])];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    let d = decision(&r.blocks[1], "alice");
    assert_eq!(d.action, Action::Skip);
    assert_eq!(d.candidates, Amount::ZERO);
    assert_eq!(r.state.clmm["clmm"].positions.len(), 17);
    let mut s = isolated_mm();
    s.mm_policies.get_mut("alice").unwrap().refresh_every = 2.into();
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("X".into(), 1000.into());
    let w = World::genesis(&s).unwrap();
    s.blocks[0] = vec![tx(
        &w,
        "alice",
        (0..17).map(|i| order(Side::Sell, 100 + i, 1)).collect(),
    )];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    let d = decision(&r.blocks[1], "alice");
    assert_eq!(d.action, Action::Skip);
    assert_eq!(d.candidates, Amount::ZERO);
    assert_eq!(r.state.books["book"].orders.len(), 17);
}

#[test]
fn oracle_shock_changes_real_quotes_with_one_block_observation_delay() {
    let mut r = Run::new(mm()).unwrap();
    r.finish().unwrap();
    for (index, anchor_ticks, bid, ask, locked_q) in [
        (0, 50, 45, 55, 180),
        (1, 50, 45, 55, 180),
        (2, 60, 55, 65, 220),
    ] {
        let b = &r.blocks[index];
        let d = decision(b, "alice");
        assert_eq!(
            d.details.as_ref().unwrap().anchor.as_ref().unwrap().ticks,
            Some(anchor_ticks.into())
        );
        assert_eq!(b.state.balance("book-escrow", "X"), 20.into());
        assert_eq!(b.state.balance("book-escrow", "Q"), locked_q.into());
        assert!(
            b.state.books["book"]
                .orders
                .values()
                .any(|o| o.side == Side::Buy && o.price_ticks == bid.into())
        );
        assert!(
            b.state.books["book"]
                .orders
                .values()
                .any(|o| o.side == Side::Sell && o.price_ticks == ask.into())
        );
    }
    assert_eq!(decision(&r.blocks[1], "alice").action, Action::Hold);
    assert_eq!(decision(&r.blocks[2], "carol").action, Action::Skip);
    let arb = decision(&r.blocks[3], "carol");
    assert_eq!(arb.action, Action::Submit);
    assert_eq!(arb.expected_net_gain, Some(1.into()));
    let route = arb.executed_route.as_ref().unwrap();
    assert_eq!((route.input_spent, route.output), (10.into(), 11.into()));
    assert!(route.residuals.is_empty());
    assert_eq!(route.hops[0].output, 108.into());
    assert_eq!(r.blocks[3].state.balance("book-escrow", "Q"), 110.into());
    assert_eq!(r.blocks[3].state.balance("book-escrow", "X"), 10.into());
    assert_eq!(
        decision(&r.blocks[4], "alice")
            .details
            .as_ref()
            .unwrap()
            .anchor
            .as_ref()
            .unwrap()
            .age,
        Some(2.into())
    );
    let stale = decision(&r.blocks[5], "alice");
    assert_eq!(stale.action, Action::Submit);
    assert_eq!(
        stale.details.as_ref().unwrap().anchor.as_ref().unwrap().age,
        Some(3.into())
    );
    assert!(
        stale
            .transaction
            .as_ref()
            .unwrap()
            .program
            .iter()
            .all(|c| matches!(c, Call::CancelOrder { .. }))
    );
    assert!(r.state.books["book"].orders.is_empty());
    assert_eq!(r.state.balance("book-escrow", "Q"), Amount::ZERO);
    assert_eq!(r.state.balance("book-escrow", "X"), Amount::ZERO);
}

#[test]
fn fixed_anchor_and_refresh_cadence_hold_matching_quotes() {
    let mut s = isolated_mm();
    let p = s.mm_policies.get_mut("alice").unwrap();
    p.anchor = Anchor::Fixed { ticks: 50.into() };
    p.refresh_every = 2.into();
    s.blocks = vec![vec![]; 4];
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert_eq!(
        r.blocks
            .iter()
            .map(|b| decision(b, "alice").action)
            .collect::<Vec<_>>(),
        [Action::Hold, Action::Submit, Action::Hold, Action::Hold]
    );
    assert_eq!(r.state.accounts["alice"].nonce, 1.into());
    assert_eq!(r.state.balance("book-escrow", "Q"), 180.into());
}

#[test]
fn oracle_anchor_is_exact_rational_floor_and_invalid_domain_skips() {
    let mut s = isolated_mm();
    anchor(&mut s, 7.into(), 3.into());
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let d = decision(&r.blocks[0], "alice");
    assert_eq!(
        d.details.as_ref().unwrap().anchor.as_ref().unwrap().ticks,
        Some(11.into())
    );
    assert_eq!(r.state.balance("book-escrow", "Q"), 24.into());
    for (n, d) in [(1.into(), Amount::MAX), (Amount::MAX, 1.into())] {
        let mut s = isolated_mm();
        anchor(&mut s, n, d);
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        assert_eq!(decision(&r.blocks[0], "alice").action, Action::Skip);
        assert_eq!(r.state.accounts, r.initial.accounts);
        assert!(r.state.books["book"].orders.is_empty());
    }
}

#[test]
fn missing_feed_cancels_quotes_and_unfunded_pair_never_half_posts() {
    let mut s = isolated_mm();
    s.feeds.get_mut("public-price").unwrap().initial = None;
    let w = World::genesis(&s).unwrap();
    s.blocks[0] = vec![tx(
        &w,
        "alice",
        vec![order(Side::Buy, 45, 2), order(Side::Sell, 55, 2)],
    )];
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    assert_eq!(decision(&r.blocks[0], "alice").action, Action::Hold);
    r.step().unwrap();
    assert_eq!(decision(&r.blocks[1], "alice").action, Action::Submit);
    assert!(r.state.books["book"].orders.is_empty());
    let mut s = isolated_mm();
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("X".into(), Amount::ZERO);
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let d = decision(&r.blocks[0], "alice");
    assert_eq!(d.action, Action::Skip);
    assert!(d.reason.starts_with("simulation failed:"));
    assert_eq!(r.state.accounts, r.initial.accounts);
    assert!(r.state.books["book"].orders.is_empty());
}

#[test]
fn stale_cancellation_requires_native_fee_budget_and_does_not_claim_success() {
    let mut s = isolated_mm();
    s.accounts
        .get_mut("alice")
        .unwrap()
        .balances
        .insert("X".into(), 2000.into());
    s.mm_policies.get_mut("alice").unwrap().resource_price = 1.into();
    s.mm_policies.get_mut("alice").unwrap().anchor = Anchor::PublicOracle {
        feed: "public-price".into(),
        max_age: Amount::ZERO,
    };
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let amount = r.state.balance("alice", "X");
    r.scenario.blocks[1] = vec![tx(
        &r.state,
        "alice",
        vec![Call::Transfer {
            to: "bob".into(),
            asset: "X".into(),
            amount,
        }],
    )];
    r.step().unwrap();
    assert_eq!(r.blocks[1].rejected.len(), 1);
    r.step().unwrap();
    let d = decision(&r.blocks[2], "alice");
    assert_eq!(d.action, Action::Skip);
    assert!(d.reason.contains("insufficient fee budget"));
    assert_eq!(r.state.books["book"].orders.len(), 2);
}

#[test]
fn maker_gtc_orders_can_take_and_pay_fee_when_earlier_flow_changes_book() {
    let mut s = isolated_mm();
    s.mm_policies.get_mut("alice").unwrap().anchor = Anchor::Fixed { ticks: 50.into() };
    let w = World::genesis(&s).unwrap();
    s.blocks[0] = vec![tx(&w, "bob", vec![order(Side::Sell, 40, 2)])];
    let mut r = Run::new(s).unwrap();
    r.step().unwrap();
    let receipt = &r.blocks[0].receipts[1];
    assert_eq!(receipt.status, "Success");
    assert_eq!(receipt.trace[0].book.as_ref().unwrap().fee, 2.into());
    assert_eq!(r.state.balance("treasury", "Q"), 2.into());
    assert_eq!(r.state.books["book"].orders.len(), 1);
    assert_eq!(
        r.state.books["book"].orders.values().next().unwrap().side,
        Side::Sell
    );
}

fn profitable_cycle() -> Scenario {
    let mut s = mm();
    s.mm_policies.clear();
    s.blocks = vec![vec![]; 3];
    s.accounts
        .get_mut("cpmm-account")
        .unwrap()
        .balances
        .insert("X".into(), 3000.into());
    let p = s.policies.get_mut("carol").unwrap();
    p.resource_price = 1.into();
    p.route_limits = Some(vec![h::DirectionalLimit {
        asset_in: "X".into(),
        hop: h::VenueHop::Book {
            venue: "book".into(),
            limit_price_ticks: 50.into(),
        },
    }]);
    let w = World::genesis(&s).unwrap();
    s.blocks[0] = vec![tx(&w, "alice", vec![order(Side::Buy, 50, 2)])];
    s
}
#[test]
fn heterogeneous_arbitrage_uses_require_full_and_actual_fee_including_self_recipient() {
    for self_fee in [false, true] {
        let mut s = profitable_cycle();
        if self_fee {
            s.chain.fee_account = "carol".into();
        }
        let mut r = Run::new(s).unwrap();
        r.step().unwrap();
        let before = r.state.balance("carol", "X");
        r.step().unwrap();
        let d = decision(&r.blocks[1], "carol");
        assert_eq!(d.action, Action::Submit);
        let expected = if self_fee { 260.into() } else { 89.into() };
        assert_eq!(d.expected_net_gain, Some(expected));
        assert_eq!(
            r.state.balance("carol", "X"),
            before.checked_add(expected).unwrap()
        );
        assert!(matches!(
            d.transaction.as_ref().unwrap().program.last(),
            Some(Call::AssertTransactionNetGain { .. })
        ));
        assert_eq!(
            d.executed_route.as_ref().unwrap().residual_policy,
            h::ResidualPolicy::RequireFull
        );
        assert_eq!(r.blocks[1].receipts[0].fee, 171.into());
    }
    let mut s = profitable_cycle();
    s.policies.get_mut("carol").unwrap().input = 11.into();
    let mut r = Run::new(s).unwrap();
    r.finish().unwrap();
    assert!(
        r.blocks
            .iter()
            .all(|b| decision(b, "carol").action == Action::Skip)
    );
    assert_eq!(r.state.accounts["carol"].nonce, Amount::ZERO);
}

#[test]
fn observation_has_only_own_positions_orders_balances_and_public_oracle() {
    let mut r = Run::new(lp()).unwrap();
    r.step().unwrap();
    let obs = Observation::capture(&r.scenario, &r.state, "alice").unwrap();
    assert_eq!(obs.clmm["clmm"].own_positions.len(), 1);
    assert!(
        obs.clmm["clmm"]
            .own_positions
            .values()
            .all(|p| p.owner == "alice")
    );
    let mut other = r.state.clone();
    Arc::make_mut(&mut other.accounts)
        .get_mut("bob")
        .unwrap()
        .balances
        .insert("X".into(), 77.into());
    assert_eq!(
        obs,
        Observation::capture(&r.scenario, &other, "alice").unwrap()
    );
    let mut future = r.scenario.clone();
    future.blocks[4].clear();
    assert_eq!(
        obs,
        Observation::capture(&future, &r.state, "alice").unwrap()
    );
    let mut r = Run::new(mm()).unwrap();
    r.step().unwrap();
    let own = Observation::capture(&r.scenario, &r.state, "alice").unwrap();
    let stranger = Observation::capture(&r.scenario, &r.state, "bob").unwrap();
    assert_eq!(own.books["book"].own_orders.len(), 2);
    assert!(stranger.books["book"].own_orders.is_empty());
    assert_eq!(own.feeds, stranger.feeds);
}

#[test]
fn simulations_and_checkpoints_preserve_schedules_decisions_and_rng() {
    for s in [lp(), mm()] {
        let mut full = Run::new(s.clone()).unwrap();
        full.finish().unwrap();
        let mut r = Run::new(s).unwrap();
        while r.step().unwrap().is_some() {
            let before = r.checkpoint().unwrap();
            for _ in 0..3 {
                policy::decisions(&r.scenario, &r.state).unwrap();
            }
            assert_eq!(before, r.checkpoint().unwrap());
            let mut restored = Run::restore(&before).unwrap();
            restored.finish().unwrap();
            assert_eq!(restored, full);
            assert!(
                r.blocks
                    .last()
                    .unwrap()
                    .decisions
                    .iter()
                    .all(|d| d.candidates <= 64.into())
            );
        }
    }
}

#[test]
fn policy_configuration_rejects_duplicate_agents_unowned_accounts_and_unbounded_work() {
    let mut s = mm();
    s.mm_policies
        .insert("carol".into(), s.mm_policies["alice"].clone());
    assert_eq!(s.validate().unwrap_err(), "only one policy per AgentId");
    let mut s = lp();
    s.lp_policies.get_mut("alice").unwrap().account = "bob".into();
    assert!(s.validate().is_err());
    let mut s = lp();
    let event = s.lp_policies["alice"].schedule[0].clone();
    s.lp_policies.get_mut("alice").unwrap().schedule = vec![event; 1025];
    assert!(s.validate().is_err());
    let mut s = lp();
    s.lp_policies.get_mut("alice").unwrap().schedule[0].block = Amount::ZERO;
    assert!(s.validate().is_err());
    let mut s = lp();
    let event = s.lp_policies["alice"].schedule[0].clone();
    s.lp_policies.get_mut("alice").unwrap().schedule.push(event);
    assert!(s.validate().is_err());
    let mut s = mm();
    s.mm_policies.get_mut("alice").unwrap().refresh_every = Amount::ZERO;
    assert!(s.validate().is_err());
    let mut s = mm();
    s.mm_policies.get_mut("alice").unwrap().anchor = Anchor::Fixed { ticks: 5.into() };
    assert!(s.validate().is_err());
    let mut s = mm();
    s.mm_policies.get_mut("alice").unwrap().anchor = Anchor::PublicOracle {
        feed: "private-reference".into(),
        max_age: 60.into(),
    };
    assert!(s.validate().is_err());
}

#[test]
fn route_probe_model_error_is_propagated_instead_of_misreported_as_admission_failure() {
    let s = profitable_cycle();
    let w = World::genesis(&s).unwrap();
    let obs = Observation::capture(&s, &w, "carol").unwrap();
    let q = h::Query {
        agent: "carol".into(),
        sender: "carol".into(),
        asset_in: "X".into(),
        asset_out: "X".into(),
        amount: 10.into(),
        min_out: 1.into(),
        residual_policy: h::ResidualPolicy::RequireFull,
        resource_limit: 1000.into(),
        resource_price: 1.into(),
        limits: s.policies["carol"].route_limits.clone().unwrap(),
    };
    let calls = Cell::new(0);
    let result = h::search_with(&obs, &s.chain, &q, &|_| {
        calls.set(calls.get() + 1);
        Err("injected ModelError".into())
    });
    assert_eq!(result.unwrap_err(), "injected ModelError");
    assert_eq!(calls.get(), 1);
}
